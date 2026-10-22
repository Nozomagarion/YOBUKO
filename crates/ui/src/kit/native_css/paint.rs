// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native painting of computed linear gradients. Stylo owns parsing and colour interpolation.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Color32, Painter, Pos2, Rect, Vec2};
use style::color::AbsoluteColor;
use style::color::mix::{ColorInterpolationMethod, ColorMixItem, mix_many};
use style::values::computed::image::{Gradient as CssGradient, LineDirection};
use style::values::computed::{Length, LengthPercentage};
use style::values::generics::color::ColorMixFlags;
use style::values::generics::image::{GradientFlags, GradientItem};
use style::values::specified::position::{HorizontalPositionKeyword as X, VerticalPositionKeyword as Y};

const SAMPLES: usize = 1024;
const MAX_TEXTURES: usize = 64;

#[derive(Clone, Debug)]
struct Stop {
    color: AbsoluteColor,
    position: Option<LengthPercentage>,
    hint: Option<LengthPercentage>,
}

#[derive(Clone, Debug)]
pub(super) struct Gradient {
    key: u64,
    direction: LineDirection,
    interpolation: ColorInterpolationMethod,
    repeating: bool,
    stops: Vec<Stop>,
}

impl Gradient {
    pub fn from_css(css: &CssGradient, foreground: &AbsoluteColor, key: u64) -> Option<Self> {
        let CssGradient::Linear { direction, color_interpolation_method, items, flags, .. } = css else {
            return None;
        };
        if items.len() > 64 {
            return None;
        }
        let mut stops: Vec<Stop> = Vec::new();
        for item in items.iter() {
            let (color, position) = match item {
                GradientItem::SimpleColorStop(color) => (color, None),
                GradientItem::ComplexColorStop { color, position } => (color, Some(position.clone())),
                GradientItem::InterpolationHint(hint) => {
                    stops.last_mut()?.hint = Some(hint.clone());
                    continue;
                }
            };
            stops.push(Stop { color: color.resolve_to_absolute(foreground), position, hint: None });
        }
        if stops.len() < 2 {
            return None;
        }
        Some(Self {
            key,
            direction: *direction,
            interpolation: *color_interpolation_method,
            repeating: flags.contains(GradientFlags::REPEATING),
            stops,
        })
    }

    fn axis(&self, size: Vec2) -> Vec2 {
        let x = |value: X| if value == X::Left { -1.0 } else { 1.0 };
        let y = |value: Y| if value == Y::Top { -1.0 } else { 1.0 };
        match &self.direction {
            LineDirection::Angle(angle) => Vec2::new(angle.radians().sin(), -angle.radians().cos()),
            LineDirection::Horizontal(value) => Vec2::new(x(*value), 0.0),
            LineDirection::Vertical(value) => Vec2::new(0.0, y(*value)),
            LineDirection::Corner(horizontal, vertical) => {
                Vec2::new(x(*horizontal) * size.y, y(*vertical) * size.x).normalized()
            }
        }
    }

    fn positions(&self, basis: f32) -> Vec<f32> {
        let position = |value: &LengthPercentage| value.resolve(Length::new(basis)).px() / basis;
        let mut points: Vec<_> = self.stops.iter().map(|stop| stop.position.as_ref().map(position)).collect();
        let last = points.len().saturating_sub(1);
        if let Some(first) = points.first_mut() {
            *first = Some(first.unwrap_or(0.0));
        }
        if let Some(last) = points.last_mut() {
            *last = Some(last.unwrap_or(1.0));
        }
        let mut maximum = f32::NEG_INFINITY;
        for point in points.iter_mut().flatten() {
            *point = point.max(maximum);
            maximum = *point;
        }
        let mut from = 0;
        while from < last {
            let next =
                ((from + 1)..=last).find(|index| points.get(*index).is_some_and(Option::is_some)).unwrap_or(last);
            let a = points.get(from).copied().flatten().unwrap_or(0.0);
            let b = points.get(next).copied().flatten().unwrap_or(1.0);
            for index in (from + 1)..next {
                if let Some(point) = points.get_mut(index) {
                    *point = Some(a + (b - a) * (index - from) as f32 / (next - from) as f32);
                }
            }
            from = next;
        }
        points.into_iter().map(|point| point.unwrap_or(0.0)).collect()
    }

    fn sample(&self, points: &[f32], basis: f32, position: f32) -> Color32 {
        let mut index = 0;
        for at in 1..points.len() {
            if position < points.get(at).copied().unwrap_or(1.0) {
                break;
            }
            index = at;
        }
        let Some(left) = self.stops.get(index) else {
            return Color32::TRANSPARENT;
        };
        let Some(right) = self.stops.get(index + 1) else {
            return super::rgba(&left.color);
        };
        let a = points.get(index).copied().unwrap_or(0.0);
        let b = points.get(index + 1).copied().unwrap_or(1.0);
        let mut weight = if b > a { ((position - a) / (b - a)).clamp(0.0, 1.0) } else { 1.0 };
        if let Some(hint) = &left.hint {
            let hint = (hint.resolve(Length::new(basis)).px() / basis - a) / (b - a).max(f32::EPSILON);
            weight = if hint <= 0.0 {
                if weight > 0.0 { 1.0 } else { 0.0 }
            } else if hint >= 1.0 {
                if weight >= 1.0 { 1.0 } else { 0.0 }
            } else {
                weight.powf(0.5_f32.ln() / hint.ln())
            };
        }
        let color = mix_many(
            self.interpolation,
            [ColorMixItem::new(left.color, 1.0 - weight), ColorMixItem::new(right.color, weight)],
            ColorMixFlags::empty(),
        );
        super::rgba(&color)
    }
}

#[derive(Clone)]
struct Ramp {
    texture: egui::TextureHandle,
    start: f32,
    length: f32,
}
#[derive(Default)]
struct Textures(HashMap<(u64, u32, u32), Ramp>);

fn draw(painter: &Painter, rect: Rect, gradient: &Gradient, radius: f32, opacity: f32) -> bool {
    if !rect.is_finite() || !rect.is_positive() {
        return false;
    }
    let direction = gradient.axis(rect.size());
    let basis = direction.x.abs() * rect.width() + direction.y.abs() * rect.height();
    if !basis.is_finite() || basis <= 0.0 {
        return false;
    }
    let key = (gradient.key, rect.width().to_bits(), rect.height().to_bits());
    let cache = painter.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<Textures>>>(egui::Id::new("css-gradient-textures")).clone()
    });
    let Ok(mut cache) = cache.lock() else {
        return false;
    };
    let ramp = if let Some(ramp) = cache.0.get(&key) {
        ramp.clone()
    } else {
        let points = gradient.positions(basis);
        let start = if gradient.repeating { points.first().copied().unwrap_or(0.0) } else { 0.0 };
        let length = if gradient.repeating { points.last().copied().unwrap_or(1.0) - start } else { 1.0 };
        if !length.is_finite() || length <= f32::EPSILON {
            return false;
        }
        let pixels = (0..SAMPLES)
            .map(|index| gradient.sample(&points, basis, start + length * index as f32 / (SAMPLES - 1) as f32))
            .collect();
        let options = egui::TextureOptions {
            wrap_mode: if gradient.repeating {
                egui::TextureWrapMode::Repeat
            } else {
                egui::TextureWrapMode::ClampToEdge
            },
            ..egui::TextureOptions::LINEAR
        };
        let ramp = Ramp {
            texture: painter.ctx().load_texture(
                format!("css-gradient-{}", gradient.key),
                egui::ColorImage::new([SAMPLES, 1], pixels),
                options,
            ),
            start,
            length,
        };
        if cache.0.len() >= MAX_TEXTURES {
            cache.0.clear();
        }
        cache.0.insert(key, ramp.clone());
        ramp
    };
    drop(cache);
    let mut mesh = egui::Mesh::with_texture(ramp.texture.id());
    let uv = |point: Pos2| {
        egui::pos2((((point - rect.center()).dot(direction) / basis + 0.5) - ramp.start) / ramp.length, 0.5)
    };
    let color = Color32::WHITE.gamma_multiply(opacity);
    mesh.vertices.push(egui::epaint::Vertex { pos: rect.center(), uv: uv(rect.center()), color });
    let radius = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
    let corners = [
        (rect.left_top() + Vec2::splat(radius), std::f32::consts::PI),
        (rect.right_top() + Vec2::new(-radius, radius), -std::f32::consts::FRAC_PI_2),
        (rect.right_bottom() - Vec2::splat(radius), 0.0),
        (rect.left_bottom() + Vec2::new(radius, -radius), std::f32::consts::FRAC_PI_2),
    ];
    for (center, angle) in corners {
        for index in 0..=8 {
            let angle = angle + index as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
            let pos = center + Vec2::angled(angle) * radius;
            mesh.vertices.push(egui::epaint::Vertex { pos, uv: uv(pos), color });
        }
    }
    let count = mesh.vertices.len().saturating_sub(1) as u32;
    for index in 1..=count {
        mesh.indices.extend_from_slice(&[0, index, if index == count { 1 } else { index + 1 }]);
    }
    painter.add(mesh);
    true
}

pub fn background(painter: &Painter, rect: Rect) -> bool {
    background_rounded(painter, rect, 0.0)
}
pub fn background_rounded(painter: &Painter, rect: Rect, radius: f32) -> bool {
    super::super::style_class::computed().is_some_and(|mut style| {
        if !style.authored.radius {
            style.radius = radius;
        }
        background_with(painter, rect, &style)
    })
}

pub fn background_with(painter: &Painter, rect: Rect, style: &super::Computed) -> bool {
    let mut painted = false;
    for gradient in style.gradients.iter().rev() {
        painted |= draw(painter, rect, gradient, style.radius, style.opacity);
    }
    painted
}
