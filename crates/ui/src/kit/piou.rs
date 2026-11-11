// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Piou, YOBUKO's mascot: a round baby bird with headphones, drawn from shapes so it scales from
//! the bar (about 18 px) to a page (about 100 px) and moves with what the app does. Its
//! headphones take the colour of the space showing; its mood says what is going on (music
//! playing, a room waiting for friends, a friend arriving, a dropped connection).
//!
//! A still Piou asks for no repaint: at rest it only blinks, which is scheduled, and the moods
//! that move repaint at [`FRAME`] while they last. With animations off (`motion::enabled`)
//! every mood is one still pose.

use egui::{Color32, Context, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use super::motion;

/// What Piou is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mood {
    /// Nothing special: still, with a blink now and then.
    #[default]
    Idle,
    /// Music plays: it nods on the beat.
    Play,
    /// A room is open and waits for friends: it calls out.
    Call,
    /// Something good just happened (a friend arrived, a track reached the DJ library): it hops.
    Joy,
    /// The connection dropped: it worries, headphones greyed.
    Lost,
    /// Nothing has moved for a while: it sleeps.
    Sleep,
}

/// Piou's colours: the headphones (the space's colour) and its down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coat {
    pub headphones: Color32,
    pub body: Color32,
    pub shade: Color32,
}

impl Coat {
    /// Cream down, for the studio and the USB space.
    pub const fn cream(headphones: Color32) -> Self {
        Self {
            headphones,
            body: Color32::from_rgb(0xFF, 0xF9, 0xF0),
            shade: Color32::from_rgb(0xF1, 0xE4, 0xD3),
        }
    }

    /// Yellow down, for Multiplayer.
    pub const fn yellow(headphones: Color32) -> Self {
        Self {
            headphones,
            body: Color32::from_rgb(0xFF, 0xF3, 0xC4),
            shade: Color32::from_rgb(0xF5, 0xDF, 0x95),
        }
    }

    /// Lavender down, for DJ.
    pub const fn lavender(headphones: Color32) -> Self {
        Self {
            headphones,
            body: Color32::from_rgb(0xF4, 0xE6, 0xFF),
            shade: Color32::from_rgb(0xDE, 0xC6, 0xF2),
        }
    }
}

/// How often a moving mood repaints.
pub const FRAME: std::time::Duration = std::time::Duration::from_millis(33);
/// Seconds between two blinks, and how long one lasts.
const BLINK_EVERY: f64 = 4.6;
const BLINK: f64 = 0.14;

const OUTLINE: Color32 = Color32::from_rgb(0x2B, 0x22, 0x50);
const ORANGE: Color32 = Color32::from_rgb(0xFF, 0xA6, 0x3D);
const MOUTH: Color32 = Color32::from_rgb(0xE8, 0x45, 0x6B);
const CHEEK: Color32 = Color32::from_rgb(0xFF, 0x8F, 0xAF);
const GREYED: Color32 = Color32::from_rgb(0x8B, 0x87, 0x94);

/// The pose of one frame: how the whole bird is squashed, turned and lifted, in drawing units
/// (the bird is drawn in a 120 × 120 box, standing on y = 108).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pose {
    scale: Vec2,
    turn: f32,
    lift: f32,
    shift: f32,
    /// How open the eyes are, 0 to 1.
    eyes: f32,
    /// How far the wings are raised, radians.
    flap: f32,
    /// 0 to 1 through the effect around it (notes rising, call arcs spreading).
    effect: f32,
}

impl Pose {
    const STILL: Self = Self { scale: Vec2::splat(1.0), turn: 0.0, lift: 0.0, shift: 0.0, eyes: 1.0, flap: 0.0, effect: 0.35 };
}

/// The pose of `mood` at `time` seconds, on a beat of `beat` seconds. `moving` off gives the
/// mood's still pose.
fn pose(mood: Mood, time: f64, beat: f64, moving: bool) -> Pose {
    use std::f64::consts::TAU;
    let wave = |period: f64| if moving { (time * TAU / period).sin() as f32 } else { 0.0 };
    let phase = |period: f64| if moving { (time.rem_euclid(period) / period) as f32 } else { 0.35 };
    let mut pose = Pose::STILL;
    match mood {
        Mood::Idle => {
            if moving && time.rem_euclid(BLINK_EVERY) < BLINK {
                pose.eyes = 0.1;
            }
        }
        Mood::Play => {
            let beat = beat.clamp(0.25, 2.0);
            let nod = wave(beat);
            pose.lift = 2.5 + 2.5 * nod;
            pose.scale = Vec2::new(1.01 - 0.03 * nod, 0.99 + 0.035 * nod);
            pose.turn = -0.05 * (0.5 + 0.5 * nod);
            pose.effect = phase(beat * 2.0);
        }
        Mood::Call => {
            let lean = 0.5 + 0.5 * wave(1.2);
            pose.turn = -0.09 * lean;
            pose.scale = Vec2::splat(1.0 + 0.03 * lean);
            pose.effect = phase(1.2);
        }
        Mood::Joy => {
            let hop = phase(0.55);
            // Up and down once per hop, squashed where it lands.
            let height = if moving { (hop * std::f32::consts::PI).sin() } else { 0.6 };
            pose.lift = 15.0 * height;
            pose.scale = Vec2::new(1.1 - 0.15 * height, 0.88 + 0.19 * height);
            pose.flap = if moving { 0.33 + 0.33 * wave(0.28) } else { 0.5 };
            pose.effect = phase(1.1);
        }
        Mood::Lost => {
            pose.shift = 1.6 * wave(0.35);
            pose.turn = 0.02 * wave(0.35);
            pose.effect = phase(0.7);
        }
        Mood::Sleep => pose.eyes = 0.0,
    }
    pose
}

/// Whether `mood` moves from frame to frame (so needs repaints while it shows).
fn moves(mood: Mood) -> bool {
    matches!(mood, Mood::Play | Mood::Call | Mood::Joy | Mood::Lost)
}

/// Draws Piou in `rect` (it keeps its square shape, centred), in `mood`, and asks for the
/// repaint its next frame needs. `beat` is the length of a beat in seconds, for `Mood::Play`.
pub fn show(ctx: &Context, painter: &Painter, rect: Rect, mood: Mood, coat: Coat, beat: f64) {
    let moving = motion::enabled(ctx);
    let time = ctx.input(|i| i.time);
    if moving {
        if moves(mood) {
            ctx.request_repaint_after(FRAME);
        } else if mood == Mood::Idle {
            let into = time.rem_euclid(BLINK_EVERY);
            let wait = if into < BLINK { FRAME.as_secs_f64() } else { BLINK_EVERY - into };
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.001)));
        }
    }
    painter.extend(shapes(rect, mood, coat, pose(mood, time, beat, moving)));
}

/// A point of the 120 × 120 drawing on screen, under a pose.
#[derive(Clone, Copy)]
struct Space {
    origin: Pos2,
    unit: f32,
    pose: Pose,
}

impl Space {
    /// A point of the bird itself: squashed, turned and lifted about its feet.
    fn body(&self, x: f32, y: f32) -> Pos2 {
        let (dx, dy) = ((x - 60.0) * self.pose.scale.x, (y - 108.0) * self.pose.scale.y);
        let (sin, cos) = self.pose.turn.sin_cos();
        self.fixed(60.0 + dx * cos - dy * sin + self.pose.shift, 108.0 + dx * sin + dy * cos - self.pose.lift)
    }

    /// A point that does not move with the bird (its shadow, what floats around it).
    fn fixed(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.unit
    }

    fn stroke(&self, width: f32, colour: Color32) -> Stroke {
        Stroke::new((width * self.unit).max(1.0), colour)
    }
}

/// An ellipse turned by `turn` radians, as points of the drawing.
fn ellipse(centre: (f32, f32), radius: (f32, f32), turn: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 24;
    let (sin, cos) = turn.sin_cos();
    (0..STEPS)
        .map(|i| {
            let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
            let (x, y) = (radius.0 * a.cos(), radius.1 * a.sin());
            (centre.0 + x * cos - y * sin, centre.1 + x * sin + y * cos)
        })
        .collect()
}

/// A quadratic curve from `from` through the pull of `control` to `to`, as points.
fn curve(from: (f32, f32), control: (f32, f32), to: (f32, f32)) -> Vec<(f32, f32)> {
    const STEPS: usize = 8;
    (0..=STEPS)
        .map(|i| {
            let t = i as f32 / STEPS as f32;
            let u = 1.0 - t;
            (
                u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0,
                u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1,
            )
        })
        .collect()
}

/// `points` turned by `angle` about `pivot`.
fn turned(points: Vec<(f32, f32)>, pivot: (f32, f32), angle: f32) -> Vec<(f32, f32)> {
    let (sin, cos) = angle.sin_cos();
    points
        .into_iter()
        .map(|(x, y)| {
            let (dx, dy) = (x - pivot.0, y - pivot.1);
            (pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos)
        })
        .collect()
}

/// Every shape of one frame, back to front.
fn shapes(rect: Rect, mood: Mood, coat: Coat, pose: Pose) -> Vec<Shape> {
    let side = rect.width().min(rect.height());
    let unit = side / 120.0;
    let space = Space { origin: rect.center() - Vec2::splat(side / 2.0), unit, pose };
    // Small in a bar, the fine marks (cheeks, shine) would only blur.
    let fine = side >= 30.0;
    let line = space.stroke(3.2, OUTLINE);
    let thin = space.stroke(2.6, OUTLINE);
    let headphones = if mood == Mood::Lost { GREYED } else { coat.headphones };
    let on_body = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| space.body(x, y)).collect::<Vec<_>>();
    let on_page = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| space.fixed(x, y)).collect::<Vec<_>>();
    let filled = |points: Vec<(f32, f32)>, fill: Color32, stroke: Stroke| Shape::convex_polygon(on_body(&points), fill, stroke);
    let mut out = Vec::with_capacity(40);

    // Its shadow, smaller while it is in the air.
    let grounded = 1.0 - (pose.lift / 30.0).clamp(0.0, 0.5);
    out.push(Shape::convex_polygon(
        on_page(&ellipse((60.0, 111.0), (25.0 * grounded, 3.5 * grounded), 0.0)),
        Color32::from_black_alpha(48),
        Stroke::NONE,
    ));
    for x in [48.0, 72.0] {
        out.push(filled(ellipse((x, 105.0), (8.0, 4.2), 0.0), ORANGE, line));
    }
    // The headband, over the head from ear to ear.
    let band: Vec<(f32, f32)> = (0..=20)
        .map(|i| {
            let a = i as f32 / 20.0 * std::f32::consts::PI;
            (60.0 - 37.0 * a.cos(), 64.0 - 40.0 * a.sin())
        })
        .collect();
    out.push(Shape::line(on_body(&band), space.stroke(11.5, OUTLINE)));
    out.push(Shape::line(on_body(&band), space.stroke(5.5, headphones)));
    // The tuft, two feathers behind the top of the head.
    out.push(filled(ellipse((56.0, 25.0), (5.5, 11.0), -0.3), coat.body, line));
    out.push(filled(ellipse((67.0, 25.0), (5.0, 9.5), 0.75), coat.body, line));
    out.push(filled(ellipse((60.0, 68.5), (36.0, 35.5), 0.0), coat.body, line));
    for (x, lean, pivot, raise) in [(27.0, 0.42, (31.0, 80.0), pose.flap), (93.0, -0.42, (89.0, 80.0), -pose.flap)] {
        out.push(filled(turned(ellipse((x, 90.0), (8.0, 12.5), lean), pivot, raise), coat.shade, line));
    }
    for x in [20.5, 99.5] {
        // An ear cup: a tall rounded pad.
        let cup: Vec<(f32, f32)> = ellipse((x, 68.5), (8.5, 8.5), 0.0)
            .into_iter()
            .map(|(px, py)| (px, if py < 68.5 { py - 6.0 } else { py + 6.0 }))
            .collect();
        out.push(filled(cup, headphones, line));
        if fine {
            out.push(filled(ellipse((x - 3.2, 64.5), (1.75, 5.5), 0.0), Color32::from_white_alpha(140), Stroke::NONE));
        }
    }
    if fine {
        for x in [38.0, 82.0] {
            out.push(filled(ellipse((x, 78.0), (6.5, 3.8), 0.0), CHEEK.gamma_multiply(0.75), Stroke::NONE));
        }
    }
    // The eyes: round and shiny, smiling arcs when it is happy, lowered lids when it sleeps.
    for x in [46.0, 74.0] {
        match mood {
            Mood::Joy => out.push(Shape::line(on_body(&curve((x - 6.0, 68.0), (x, 60.0), (x + 6.0, 68.0))), line)),
            Mood::Sleep => out.push(Shape::line(on_body(&curve((x - 6.0, 65.0), (x, 71.0), (x + 6.0, 65.0))), line)),
            _ => {
                out.push(filled(ellipse((x, 66.0), (6.6, 6.6 * pose.eyes.max(0.08)), 0.0), OUTLINE, Stroke::NONE));
                if fine && pose.eyes > 0.5 {
                    out.push(filled(ellipse((x + 2.2, 63.4), (2.3, 2.3), 0.0), Color32::WHITE, Stroke::NONE));
                    out.push(filled(ellipse((x - 2.0, 68.6), (1.1, 1.1), 0.0), Color32::WHITE, Stroke::NONE));
                }
            }
        }
    }
    // The beak: shut, or open on a pink mouth when it sings, calls or cheers.
    if matches!(mood, Mood::Play | Mood::Call | Mood::Joy) {
        let mut mouth = curve((53.5, 75.0), (60.0, 89.0), (66.5, 75.0));
        mouth.push((53.5, 75.0));
        out.push(filled(mouth, MOUTH, thin));
        let mut top = curve((52.5, 74.0), (60.0, 67.0), (67.5, 74.0));
        top.extend(curve((67.5, 74.0), (60.0, 77.5), (52.5, 74.0)));
        out.push(filled(top, ORANGE, thin));
    } else {
        let mut beak = curve((53.5, 73.0), (60.0, 68.0), (66.5, 73.0));
        beak.extend(curve((66.5, 73.0), (60.0, 80.5), (53.5, 73.0)));
        out.push(filled(beak, ORANGE, thin));
    }

    // What floats around it. These fade in and out through `effect` and stay off the bird.
    let fade = |at: f32| (1.0 - (at * 2.0 - 1.0).abs()).clamp(0.0, 1.0);
    let ink = |alpha: f32| super::theme::text_primary().gamma_multiply(alpha);
    match mood {
        Mood::Play | Mood::Joy => {
            for (x, y, offset) in [(100.0, 34.0, 0.0), (14.0, 40.0, 0.5)] {
                let at = (pose.effect + offset).fract();
                let (nx, ny) = (x + 6.0 * at, y - 14.0 * at);
                let colour = ink(fade(at));
                out.push(Shape::convex_polygon(on_page(&ellipse((nx, ny), (3.4, 2.6), -0.4)), colour, Stroke::NONE));
                out.push(Shape::line(on_page(&[(nx + 2.9, ny - 0.8), (nx + 2.9, ny - 11.0), (nx + 7.5, ny - 9.0)]), space.stroke(1.8, colour)));
            }
        }
        Mood::Call => {
            for (i, (from, control, to)) in
                [((100.0, 40.0), (108.0, 44.0), (109.0, 56.0)), ((106.0, 30.0), (119.0, 37.0), (120.0, 55.0))].into_iter().enumerate()
            {
                let at = (pose.effect + 0.8 * i as f32).fract();
                out.push(Shape::line(on_page(&curve(from, control, to)), space.stroke(3.0, ink(fade(at)))));
            }
        }
        Mood::Lost => {
            let at = pose.effect;
            let y = 34.0 + 10.0 * at;
            let mut drop = curve((97.0, y), (101.5, y + 7.0), (97.0, y + 9.0));
            drop.extend(curve((97.0, y + 9.0), (92.5, y + 7.0), (97.0, y)));
            out.push(Shape::convex_polygon(
                on_page(&drop),
                Color32::from_rgb(0x7C, 0xC7, 0xFF).gamma_multiply(fade(at).max(0.3)),
                space.stroke(1.6, OUTLINE.gamma_multiply(fade(at).max(0.3))),
            ));
        }
        Mood::Sleep => {
            // A small "z" drawn from three strokes: no font has to carry it.
            let z = [(92.0, 28.0), (100.0, 28.0), (92.0, 37.0), (100.0, 37.0)];
            out.push(Shape::line(on_page(&z), space.stroke(2.4, ink(0.9))));
        }
        Mood::Idle => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COAT: Coat = Coat::cream(Color32::from_rgb(0x5B, 0x8C, 0xFF));
    const MOODS: [Mood; 6] = [Mood::Idle, Mood::Play, Mood::Call, Mood::Joy, Mood::Lost, Mood::Sleep];

    fn bounds(shapes: &[Shape]) -> Rect {
        shapes.iter().fold(Rect::NOTHING, |all, shape| all.union(shape.visual_bounding_rect()))
    }

    #[test]
    fn every_mood_draws_inside_its_box_at_bar_and_page_sizes() {
        for side in [18.0, 56.0, 120.0] {
            let rect = Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::splat(side));
            for mood in MOODS {
                for step in 0..24 {
                    let frame = shapes(rect, mood, COAT, pose(mood, f64::from(step) * 0.05, 0.5, true));
                    assert!(frame.len() >= 12, "{mood:?} draws {} shapes", frame.len());
                    let drawn = bounds(&frame);
                    // A hop and the notes may leave the box by a little, never by much.
                    assert!(rect.expand(side * 0.2).contains_rect(drawn), "{mood:?} at {side}: {drawn:?} outside {rect:?}");
                }
            }
        }
    }

    #[test]
    fn the_headphones_take_the_spaces_colour_and_grey_out_when_the_connection_drops() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(96.0));
        let has = |mood: Mood, colour: Color32| {
            shapes(rect, mood, COAT, Pose::STILL).iter().any(|shape| match shape {
                Shape::Path(path) => path.fill == colour || path.stroke.color == egui::epaint::ColorMode::Solid(colour),
                _ => false,
            })
        };
        assert!(has(Mood::Idle, COAT.headphones));
        assert!(!has(Mood::Lost, COAT.headphones));
        assert!(has(Mood::Lost, GREYED));
    }

    #[test]
    fn with_animations_off_every_mood_is_one_still_pose() {
        for mood in MOODS {
            assert_eq!(pose(mood, 0.3, 0.5, false), pose(mood, 7.9, 0.5, false), "{mood:?}");
        }
        // With them on, the moods that move do, and the bird at rest only blinks.
        assert_ne!(pose(Mood::Play, 0.1, 0.5, true), pose(Mood::Play, 0.3, 0.5, true));
        assert_eq!(pose(Mood::Idle, 1.0, 0.5, true), pose(Mood::Idle, 2.0, 0.5, true));
        assert!(pose(Mood::Idle, 0.05, 0.5, true).eyes < 0.5);
    }

    #[test]
    fn a_still_piou_never_asks_for_an_immediate_repaint() {
        // `harness.run()` waits until nothing repaints at once: a mascot must not hold it.
        for mood in MOODS {
            let mut harness = egui_kittest::Harness::builder().with_size(Vec2::splat(200.0)).build_ui(move |ui| {
                let rect = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::splat(96.0));
                show(ui.ctx(), ui.painter(), rect, mood, COAT, 0.5);
            });
            harness.run();
        }
    }
}
