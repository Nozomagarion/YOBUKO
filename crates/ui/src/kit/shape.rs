// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO's soft shapes: rounded corners on clips, buttons, strips and panels, a little air
//! between cells, and panels drawn as cards on the window's background.
//!
//! The YOBUKO themes (`yobuko` and its variants) ask for them; every other theme keeps Suhina's
//! square look, so upstream themes look as their authors drew them. Nothing here moves a
//! control: cells are drawn a little inside the rectangle they answer clicks in, and the gaps
//! between panels are painted over their edges, so every hit area stays where it was.

use std::cell::Cell;

use egui::{Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Vec2};

thread_local! {
    /// Whether the theme in use is a rounded one. The UI draws on one thread; tests each get
    /// their own, like the palette.
    static ROUND: Cell<bool> = const { Cell::new(false) };
}

/// Corner radii, in points, of each kind of shape.
pub const CELL: u8 = 8;
pub const BUTTON: u8 = 9;
pub const STRIP: u8 = 10;
/// The DJ page's cards use the same radius.
pub const PANEL: u8 = 14;
pub const CAP: u8 = 4;
pub const PILL: u8 = 255;

/// How far a cell is drawn inside its rectangle on each side.
pub const CELL_INSET: f32 = 3.0;
/// Half the gap between two panels (and the margin around the workspace).
pub const PANEL_GAP: f32 = 4.0;

/// Whether the theme `id` is drawn with soft shapes: YOBUKO's own and its variants.
pub fn theme_is_round(id: &str) -> bool {
    id == super::theme::YOBUKO || id.starts_with("yobuko@")
}

/// Uses soft shapes from the next frame on (or Suhina's square ones).
pub fn set_round(round: bool) {
    ROUND.with(|r| r.set(round));
}

/// Whether soft shapes are in use.
pub fn round() -> bool {
    ROUND.with(Cell::get)
}

/// The corner radius `radius` with soft shapes, square without.
pub fn corner(radius: u8) -> CornerRadius {
    if round() { CornerRadius::same(radius) } else { CornerRadius::ZERO }
}

/// [`corner`] as a number, for painters that take one.
pub fn radius(radius: u8) -> f32 {
    if round() { f32::from(radius) } else { 0.0 }
}

/// A button's corner `radius` (from Suhina or the theme's sheet), made soft with soft shapes:
/// at least [`BUTTON`], never more than half the button's height. A button about as wide as it
/// is tall (play, stop, record) becomes a circle, like the DJ page's transport.
pub fn soften(radius: f32, rect: Rect) -> f32 {
    if !round() {
        return radius;
    }
    let half = rect.height().min(rect.width()) / 2.0;
    let squarish = rect.width() <= rect.height() * 1.25 && rect.height() <= rect.width() * 1.25;
    if squarish && rect.height() <= 40.0 { half } else { radius.max(f32::from(BUTTON)).min(half) }
}

/// `radius` with soft shapes, `square` without: for shapes Suhina already rounds a little.
pub fn corner_or(radius: u8, square: f32) -> CornerRadius {
    if round() { CornerRadius::same(radius) } else { CornerRadius::same(square.round().clamp(0.0, 255.0) as u8) }
}

/// The part of a cell's `rect` that is drawn: a little inside it with soft shapes, so cells
/// stand apart, all of it without.
pub fn cell(rect: Rect) -> Rect {
    inset(rect, CELL_INSET)
}

/// `rect` shrunk by `by` on each side with soft shapes, never to nothing; `rect` without.
pub fn inset(rect: Rect, by: f32) -> Rect {
    if !round() {
        return rect;
    }
    let by = by.min(rect.width() / 4.0).min(rect.height() / 4.0).max(0.0);
    rect.shrink(by)
}

/// The colour around the cards: the window's background, a little deeper so panels stand out
/// on it as they do on the mock-up.
pub fn ground() -> Color32 {
    let window = super::theme::window_background();
    if super::theme::is_light() {
        super::motion::mix(window, super::theme::text_primary(), 0.10)
    } else {
        super::motion::mix(window, Color32::BLACK, 0.38)
    }
}

/// A hairline of a grid between cells: with soft shapes cells stand apart on their own and the
/// line is not drawn; without, Suhina's hairline.
pub fn grid_line(color: Color32) -> egui::Stroke {
    if round() { egui::Stroke::NONE } else { egui::Stroke::new(1.0, color) }
}

/// Paints the edges of the panel `rect` in `ground`, the window's background, so the panel
/// reads as a card with rounded corners set apart from its neighbours, outlined with a hairline
/// like the DJ page's cards. It paints over the panel's outermost points only and moves
/// nothing. Nothing is drawn without soft shapes.
pub fn card_edges(painter: &Painter, rect: Rect, ground: Color32) {
    if !round() || !rect.is_positive() {
        return;
    }
    let gap = PANEL_GAP.min(rect.width() / 4.0).min(rect.height() / 4.0);
    // The band along the four sides.
    for side in [
        Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + gap)),
        Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - gap), rect.max),
        Rect::from_min_max(rect.min, Pos2::new(rect.min.x + gap, rect.max.y)),
        Rect::from_min_max(Pos2::new(rect.max.x - gap, rect.min.y), rect.max),
    ] {
        painter.rect_filled(side, 0.0, ground);
    }
    // The corners outside the rounded card.
    let card = rect.shrink(gap);
    let radius = f32::from(PANEL).min(card.width() / 2.0).min(card.height() / 2.0);
    if radius <= 0.5 {
        return;
    }
    let corners = [
        (card.left_top(), Vec2::new(radius, radius), std::f32::consts::PI),
        (card.right_top(), Vec2::new(-radius, radius), 1.5 * std::f32::consts::PI),
        (card.right_bottom(), Vec2::new(-radius, -radius), 0.0),
        (card.left_bottom(), Vec2::new(radius, -radius), 0.5 * std::f32::consts::PI),
    ];
    let mut mesh = Mesh::default();
    const STEPS: u32 = 8;
    for (tip, toward_centre, start) in corners {
        let centre = tip + toward_centre;
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(tip, ground);
        for step in 0..=STEPS {
            let angle = start + std::f32::consts::FRAC_PI_2 * step as f32 / STEPS as f32;
            mesh.colored_vertex(centre + radius * Vec2::angled(angle), ground);
        }
        for step in 0..STEPS {
            mesh.add_triangle(base, base + 1 + step, base + 2 + step);
        }
    }
    painter.add(mesh);
    painter.rect_stroke(
        card,
        CornerRadius::same(radius.round() as u8),
        egui::Stroke::new(1.0, super::theme::border()),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_yobuko_themes_are_round() {
        assert!(theme_is_round("yobuko"));
        assert!(theme_is_round("yobuko@sakura"));
        assert!(!theme_is_round("suhina-studio"));
        assert!(!theme_is_round("yobuko-copy"));
        assert!(!theme_is_round("nord@snow-storm"));
    }

    #[test]
    fn square_themes_draw_cells_whole_and_square() {
        set_round(false);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 20.0));
        assert_eq!(cell(rect), rect);
        assert_eq!(corner(CELL), CornerRadius::ZERO);
        assert_eq!(corner_or(CELL, 2.0), CornerRadius::same(2));
    }

    #[test]
    fn round_themes_draw_cells_inside_their_rectangle() {
        set_round(true);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 20.0));
        let drawn = cell(rect);
        assert!(rect.contains_rect(drawn) && drawn != rect);
        assert_eq!(corner(CELL), CornerRadius::same(CELL));
        // A tiny cell is never shrunk to nothing.
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(4.0, 4.0));
        assert!(cell(tiny).is_positive());
        // A square transport button is a circle; a wide one or a tall panel keeps soft corners.
        let play = Rect::from_min_size(Pos2::ZERO, Vec2::new(28.0, 28.0));
        assert_eq!(soften(0.0, play), 14.0);
        assert_eq!(soften(0.0, rect), f32::from(BUTTON));
        let column = Rect::from_min_size(Pos2::ZERO, Vec2::new(150.0, 160.0));
        assert_eq!(soften(0.0, column), f32::from(BUTTON));
        set_round(false);
    }
}
