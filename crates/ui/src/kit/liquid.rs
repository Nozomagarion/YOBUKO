// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opt-in surface materials. The native host supplies the live GPU backdrop; views only
//! describe the surface and paint sharp controls afterward. No audio-thread code lives here.

use std::sync::Arc;

use egui::{Color32, Context, Id, Painter, Rect, Stroke, StrokeKind, TextureId, pos2};

use super::{motion, theme};

/// Surfaces can be enabled independently without changing a colour theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub panels: bool,
    pub tracks: bool,
    pub clips: bool,
}

/// The affected surface; track and clip materials retain their content colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Panel,
    Track,
    Clip,
}

/// An opaque, gamma-encoded backdrop registered with the host's egui renderer.
#[derive(Clone, Copy)]
pub struct Backdrop {
    pub texture: TextureId,
    pub screen: Rect,
}

/// Host-side GPU work, always requested on the UI thread before the floating panel is drawn.
pub trait Provider: Send + Sync {
    fn capture(&self, ctx: &Context) -> Option<Backdrop>;
}

#[derive(Clone)]
struct Host(Arc<dyn Provider>);

const OPTIONS: &str = "liquid-options";
const HOST: &str = "liquid-host";
const BACKDROP: &str = "liquid-backdrop";
const ACCESSIBLE: &str = "liquid-effects-allowed";

/// Updated by the app when reduced effects or a high-contrast theme is selected.
pub fn set_effects_allowed(ctx: &Context, allowed: bool) {
    ctx.data_mut(|data| data.insert_temp(Id::new(ACCESSIBLE), allowed));
}

fn effects_allowed(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(Id::new(ACCESSIBLE))).unwrap_or(true)
}

/// Install an optional native provider. A host without one keeps solid surfaces.
pub fn install(ctx: &Context, provider: Arc<dyn Provider>, options: Options) {
    ctx.data_mut(|data| {
        data.insert_temp(Id::new(HOST), Host(provider));
        data.insert_temp(Id::new(OPTIONS), options);
    });
}

/// Forget the previous panel's image, then capture this frame before any modal chrome.
pub fn prepare(ctx: &Context) {
    ctx.data_mut(|data| data.remove::<Backdrop>(Id::new(BACKDROP)));
    let options = ctx.data(|data| data.get_temp::<Options>(Id::new(OPTIONS))).unwrap_or_default();
    if !options.panels || !effects_allowed(ctx) {
        return;
    }
    let host = ctx.data(|data| data.get_temp::<Host>(Id::new(HOST)));
    if let Some(backdrop) = host.and_then(|host| host.0.capture(ctx)) {
        ctx.data_mut(|data| data.insert_temp(Id::new(BACKDROP), backdrop));
    }
}

/// Paint the base first, then this material, then the surface's text, waveform and controls.
/// Non-overlapping track/clip surfaces use a restrained optical coating; floating panels
/// additionally use the real live backdrop. Tiny surfaces stay solid at distant zooms.
pub fn paint(painter: &Painter, rect: Rect, base: Color32, surface: Surface) {
    let options = painter.ctx().data(|data| data.get_temp::<Options>(Id::new(OPTIONS))).unwrap_or_default();
    let enabled = match surface {
        Surface::Panel => options.panels,
        Surface::Track => options.tracks,
        Surface::Clip => options.clips,
    };
    if !enabled || !effects_allowed(painter.ctx()) || rect.width() < 12.0 || rect.height() < 12.0 {
        return;
    }
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(rect));
    if surface == Surface::Panel {
        let backdrop = painter.ctx().data(|data| data.get_temp::<Backdrop>(Id::new(BACKDROP)));
        let Some(backdrop) = backdrop else { return };
        let uv_min = (rect.min - backdrop.screen.min) / backdrop.screen.size();
        let uv_max = (rect.max - backdrop.screen.min) / backdrop.screen.size();
        painter.image(backdrop.texture, rect, Rect::from_min_max(uv_min.to_pos2(), uv_max.to_pos2()), Color32::WHITE);
        painter.rect_filled(rect, 0.0, base.gamma_multiply(0.84));
    }
    let light = motion::mix(base, Color32::WHITE, if surface == Surface::Panel { 0.12 } else { 0.085 });
    let shade = motion::mix(base, Color32::BLACK, 0.035);
    let height = (rect.height() * 0.34).min(22.0);
    let top = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + height));
    let mut mesh = egui::Mesh::default();
    for (position, color) in [
        (top.left_top(), light.gamma_multiply(0.5)),
        (top.right_top(), light.gamma_multiply(0.5)),
        (top.left_bottom(), Color32::TRANSPARENT),
        (top.right_bottom(), Color32::TRANSPARENT),
    ] {
        mesh.colored_vertex(position, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    painter.add(mesh);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, light));
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, shade));
    if surface == Surface::Panel {
        painter.rect_stroke(
            rect,
            0.0,
            theme::hairline(motion::mix(base, theme::text_primary(), 0.12)),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_reduced_effects_and_tiny_surfaces_remain_solid() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let ctx = ui.ctx();
            let painter = ctx.layer_painter(egui::LayerId::background());
            let area = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0));
            let count =
                || ctx.graphics(|graphics| graphics.get(painter.layer_id()).map_or(0, |list| list.all_entries().len()));
            let before = count();
            paint(&painter, area, Color32::GRAY, Surface::Track);
            assert_eq!(count(), before);
            ctx.data_mut(|data| data.insert_temp(Id::new(OPTIONS), Options { tracks: true, ..Default::default() }));
            paint(&painter, area, Color32::GRAY, Surface::Track);
            assert!(count() > before);
            let coated = count();
            set_effects_allowed(ctx, false);
            paint(&painter, area, Color32::GRAY, Surface::Track);
            assert_eq!(count(), coated);
            set_effects_allowed(ctx, true);
            paint(&painter, Rect::from_min_size(area.min, egui::vec2(10.0, 10.0)), Color32::GRAY, Surface::Track);
            assert_eq!(count(), coated);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn failed_capture_does_not_reuse_a_previous_backdrop() {
        let ctx = Context::default();
        ctx.data_mut(|data| {
            data.insert_temp(Id::new(BACKDROP), Backdrop { texture: TextureId::User(42), screen: Rect::EVERYTHING })
        });
        prepare(&ctx);
        assert!(ctx.data(|data| data.get_temp::<Backdrop>(Id::new(BACKDROP))).is_none());
    }
}
