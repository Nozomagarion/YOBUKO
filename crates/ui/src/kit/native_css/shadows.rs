// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded native inset/outset box shadows. Blur uses a Gaussian distance-field approximation.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use egui::{Color32, Painter, Pos2, Rect, TextureHandle, Vec2};
use style::color::AbsoluteColor;

const RESOLUTION: usize = 256;
const MAX_TEXTURES: usize = 32;

#[derive(Clone, Debug)]
pub(super) struct Shadow {
    offset: Vec2,
    blur: f32,
    spread: f32,
    color: Color32,
    pub inset: bool,
}

fn distance(point: Pos2, rect: Rect, radius: f32) -> f32 {
    if !rect.is_positive() {
        return f32::INFINITY;
    }
    let radius = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
    let q = (point - rect.center()).abs() - rect.size() * 0.5 + Vec2::splat(radius);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - radius
}

fn coverage(distance: f32, blur: f32) -> f32 {
    if blur <= f32::EPSILON {
        return f32::from(distance <= 0.0);
    }
    // erf approximation, with sigma = half the CSS blur radius. The same CPU
    // image is consumed by both egui rendering backends.
    let x = distance / (blur * std::f32::consts::FRAC_1_SQRT_2);
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let erf =
        1.0 - (((((1.0614054 * t - 1.4531521) * t + 1.4214138) * t - 0.28449672) * t + 0.2548296) * t) * (-x * x).exp();
    (0.5 * (1.0 - x.signum() * erf)).clamp(0.0, 1.0)
}

#[derive(Default)]
struct Textures(HashMap<u64, TextureHandle>);

impl Shadow {
    pub fn from_css(shadow: &style::values::computed::effects::BoxShadow, foreground: &AbsoluteColor) -> Self {
        Self {
            offset: Vec2::new(
                shadow.base.horizontal.px().clamp(-2048.0, 2048.0),
                shadow.base.vertical.px().clamp(-2048.0, 2048.0),
            ),
            blur: shadow.base.blur.0.px().clamp(0.0, 256.0),
            spread: shadow.spread.px().clamp(-512.0, 512.0),
            color: super::rgba(&shadow.base.color.resolve_to_absolute(foreground)),
            inset: shadow.inset,
        }
    }

    fn alpha(&self, point: Pos2, caster: Rect, radius: f32) -> f32 {
        let original = coverage(distance(point, caster, radius), 0.0);
        let spread = if self.inset { -self.spread } else { self.spread };
        let shape = caster.expand(spread).translate(self.offset);
        let shadow = coverage(distance(point, shape, (radius + spread).max(0.0)), self.blur);
        if self.inset { original * (1.0 - shadow) } else { (1.0 - original) * shadow }
    }

    pub fn paint(&self, painter: &Painter, caster: Rect, radius: f32, opacity: f32) -> bool {
        if !caster.is_finite() || !caster.is_positive() || self.color.a() == 0 {
            return false;
        }
        let bounds = if self.inset {
            caster
        } else {
            caster.expand(self.spread).translate(self.offset).expand(self.blur * 2.0).union(caster)
        };
        if !bounds.is_finite() || !bounds.is_positive() || !painter.clip_rect().intersects(bounds) {
            return false;
        }
        let local = Rect::from_min_size(Pos2::ZERO, caster.size());
        let local_bounds = bounds.translate(-caster.min.to_vec2());
        let mut hasher = std::hash::DefaultHasher::new();
        [caster.width(), caster.height(), radius, self.offset.x, self.offset.y, self.blur, self.spread]
            .map(f32::to_bits)
            .hash(&mut hasher);
        (self.color, self.inset).hash(&mut hasher);
        let key = hasher.finish();
        let cache = painter.ctx().data_mut(|data| {
            data.get_temp_mut_or_default::<Arc<Mutex<Textures>>>(egui::Id::new("css-shadow-textures")).clone()
        });
        let Ok(mut cache) = cache.lock() else {
            return false;
        };
        let texture = if let Some(texture) = cache.0.get(&key) {
            texture.clone()
        } else {
            let rgba = self.color.to_srgba_unmultiplied();
            let mut pixels = Vec::with_capacity(RESOLUTION * RESOLUTION);
            for y in 0..RESOLUTION {
                for x in 0..RESOLUTION {
                    let at = local_bounds.min
                        + Vec2::new(x as f32, y as f32) / (RESOLUTION - 1) as f32 * local_bounds.size();
                    let alpha = (self.alpha(at, local, radius) * f32::from(rgba[3])).round() as u8;
                    pixels.push(Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], alpha));
                }
            }
            let texture = painter.ctx().load_texture(
                format!("css-shadow-{key}"),
                egui::ColorImage::new([RESOLUTION, RESOLUTION], pixels),
                egui::TextureOptions::LINEAR,
            );
            if cache.0.len() >= MAX_TEXTURES {
                cache.0.clear();
            }
            cache.0.insert(key, texture.clone());
            texture
        };
        drop(cache);
        painter.image(
            texture.id(),
            bounds,
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(opacity),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outset_excludes_the_caster_and_inset_excludes_its_exterior() {
        let caster = Rect::from_min_size(Pos2::ZERO, Vec2::splat(40.0));
        let mut shadow =
            Shadow { offset: Vec2::new(4.0, 0.0), blur: 0.0, spread: 4.0, color: Color32::RED, inset: false };
        assert_eq!(shadow.alpha(egui::pos2(44.0, 20.0), caster, 0.0), 1.0);
        assert_eq!(shadow.alpha(caster.center(), caster, 0.0), 0.0);
        shadow.inset = true;
        assert_eq!(shadow.alpha(egui::pos2(2.0, 20.0), caster, 0.0), 1.0);
        assert_eq!(shadow.alpha(egui::pos2(-2.0, 20.0), caster, 0.0), 0.0);
        assert_eq!(shadow.alpha(caster.center(), caster, 0.0), 0.0);
        shadow.blur = 8.0;
        let soft = shadow.alpha(egui::pos2(8.0, 20.0), caster, 0.0);
        assert!((0.4..0.6).contains(&soft));
    }
}
