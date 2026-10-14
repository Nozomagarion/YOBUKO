//! The splash shown while Suhina starts: a small panel with the logo and the version, drawn
//! centred in the main window over its first frames. The work that makes starting slow (the
//! browser's library, the sample map, the project to open) waits until the splash has been on
//! screen, runs once, and the splash fades out over the window that is ready.
//!
//! It is drawn inside the main window rather than in a window of its own: a second window would
//! need its own GL or wgpu context, the slowest part of starting, and one window that shrinks
//! and grows would fight the main window's remembered size and position.

use egui::{Align2, Color32, Context, Id, Rect, Sense, Ui, Vec2};

use crate::motion;
use crate::theme;
use crate::widgets;

/// The panel's size, made smaller in a window that is smaller still.
pub const SIZE: Vec2 = Vec2::new(300.0, 200.0);
/// Frames the splash is drawn before the start-up work runs, so it is on screen while it does:
/// the window is made visible after its first frame.
const DRAWN_BEFORE_START: u64 = 2;
/// The name, 13 px like a page title, when the logo cannot be drawn.
const NAME: f32 = theme::TEXT + 2.0;
/// The logo's width on the panel, and the least room kept on each side of it.
const LOGO_WIDTH: f32 = 200.0;
const LOGO_MARGIN: f32 = 24.0;

#[derive(Debug, Clone)]
pub struct Splash {
    version: String,
    /// The frame it was first drawn in (`Context::cumulative_frame_nr`).
    first_frame: Option<u64>,
    /// The start-up work has been handed out (`take_start`).
    started: bool,
}

impl Splash {
    /// A splash that shows `version` (the app's `CARGO_PKG_VERSION`).
    pub fn new(version: &str) -> Self {
        Self { version: version.to_owned(), first_frame: None, started: false }
    }

    /// Whether the start-up work runs now: true once, in the first frame after the splash has
    /// been drawn in `DRAWN_BEFORE_START` whole frames. Frames, not passes: egui may run a frame
    /// twice, and only a finished frame is on screen.
    pub fn take_start(&mut self, ctx: &Context) -> bool {
        let shown =
            self.first_frame.is_some_and(|first| ctx.cumulative_frame_nr() >= first.saturating_add(DRAWN_BEFORE_START));
        let now = !self.started && shown;
        self.started |= now;
        now
    }

    /// The start-up work has run, so the window is drawn under the splash as it fades.
    pub fn started(&self) -> bool {
        self.started
    }

    /// Draws the splash over the whole window, above everything else. Until the start-up work
    /// has run it covers the window and keeps frames coming; then it fades out (at once with
    /// animations off). Returns false when it is gone and can be dropped.
    pub fn show(&mut self, ui: &Ui) -> bool {
        let ctx = ui.ctx();
        let opacity = motion::toward(ctx, Id::new("splash-fade"), !self.started, motion::VALUE);
        if opacity <= 0.0 {
            return false;
        }
        if !self.started {
            ctx.request_repaint();
        }
        let area = ctx.content_rect();
        egui::Area::new(Id::new("splash")).order(egui::Order::Tooltip).fixed_pos(area.min).show(ctx, |ui| {
            ui.set_min_size(area.size());
            self.draw(ui, area, opacity);
        });
        self.first_frame.get_or_insert(ctx.cumulative_frame_nr());
        true
    }

    fn draw(&self, ui: &Ui, area: Rect, opacity: f32) {
        // Clicks on the window do nothing until it is there.
        let behind = ui.interact(area, Id::new("splash-behind"), Sense::click_and_drag());
        let _swallowed = behind.clicked();
        let fade = |color: Color32| color.gamma_multiply(opacity);
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0.0, fade(theme::window_background()));

        let rect = Rect::from_center_size(area.center(), SIZE.min(area.size()));
        painter.rect_filled(rect, 0.0, fade(theme::panel_background()));
        painter.rect_stroke(rect, 0.0, theme::hairline(fade(theme::border_strong())), egui::StrokeKind::Inside);

        let line = |center_y: f32, height: f32| {
            Rect::from_center_size(egui::pos2(rect.center().x, center_y), Vec2::new(rect.width(), height))
        };
        // The logo, the name in the theme's text colour; the name as text if it cannot be drawn.
        let width = (rect.width() - 2.0 * LOGO_MARGIN).clamp(0.0, LOGO_WIDTH);
        let size = Vec2::new(width, width / crate::logo::aspect());
        let name = Rect::from_center_size(egui::pos2(rect.center().x, rect.center().y - 12.0), size);
        let pixels = (size * ui.ctx().pixels_per_point()).round();
        match crate::logo::texture(ui.ctx(), pixels.x as u32, pixels.y as u32, theme::text_primary()) {
            Some(logo) => {
                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(logo.id(), name, uv, fade(Color32::WHITE));
            }
            None => widgets::text_in(
                &painter,
                name,
                Align2::CENTER_CENTER,
                "Suhina",
                theme::ui_font(NAME),
                fade(theme::text_primary()),
            ),
        }
        name_for_readers(ui, name, "Suhina");
        let version = line(name.bottom() + 8.0, 16.0);
        let shown = format!("Version {}", self.version);
        widgets::text_in(
            &painter,
            version,
            Align2::CENTER_CENTER,
            &shown,
            theme::mono_font(theme::TEXT_SMALL),
            fade(theme::text_secondary()),
        );
        name_for_readers(ui, version, &shown);
        let status = line(rect.bottom() - theme::PAD - 8.0, 16.0);
        widgets::text_in(
            &painter,
            status,
            Align2::CENTER_CENTER,
            "Starting",
            theme::ui_font(theme::TEXT_SMALL),
            fade(theme::text_hint()),
        );
    }
}

/// Names a line of the splash for screen readers and tests.
fn name_for_readers(ui: &Ui, rect: Rect, label: &str) {
    let response = ui.interact(rect, Id::new(("splash-line", label)), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// What a frame of the app does with a splash: the start-up work, then the window.
    #[derive(Default)]
    struct State {
        splash: Option<Splash>,
        /// The frame the splash was first drawn in.
        first: Option<u64>,
        /// The frames the start-up work ran in.
        starts: Vec<u64>,
        /// Frames the window was drawn in, under the splash or without it.
        window_frames: u32,
    }

    fn frame(ui: &mut Ui, state: &mut State) {
        let ctx = ui.ctx().clone();
        if let Some(splash) = state.splash.as_mut()
            && splash.take_start(&ctx)
        {
            state.starts.push(ctx.cumulative_frame_nr());
        }
        if state.splash.as_ref().is_none_or(Splash::started) {
            state.window_frames += 1;
        }
        if let Some(splash) = state.splash.as_mut() {
            state.first.get_or_insert(ctx.cumulative_frame_nr());
            if !splash.show(ui) {
                state.splash = None;
            }
        }
    }

    /// A harness whose next frame is the splash's first. Building it runs frames until nothing
    /// moves, so the splash comes in after.
    fn harness(size: Vec2, animations: bool) -> Harness<'static, State> {
        let mut harness = Harness::builder()
            .with_size(size)
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(60)
            .build_ui_state(frame, State::default());
        motion::set_enabled(&harness.ctx, animations);
        *harness.state_mut() = State { splash: Some(Splash::new("1.2.3")), ..State::default() };
        harness
    }

    const WINDOW: Vec2 = Vec2::new(800.0, 500.0);

    #[test]
    fn it_shows_the_logo_and_version_from_its_first_frame() {
        let mut harness = harness(WINDOW, true);
        harness.step();
        // The name is the logo, at the logo's own proportions, with the version under it.
        let logo = harness.get_by_label("Suhina").rect();
        assert!((logo.width() / logo.height() - crate::logo::aspect()).abs() < 0.05, "{logo:?}");
        assert_eq!(logo.width(), LOGO_WIDTH);
        assert!(harness.get_by_label("Version 1.2.3").rect().top() >= logo.bottom());
        assert!(harness.state().starts.is_empty(), "nothing starts before the splash is on screen");
        assert_eq!(harness.state().window_frames, 0, "the window waits under the splash");
    }

    #[test]
    fn the_start_up_work_runs_once_after_the_splash_was_drawn() {
        let mut harness = harness(WINDOW, true);
        for _ in 0..DRAWN_BEFORE_START {
            harness.step();
        }
        assert!(harness.state().starts.is_empty());
        harness.get_by_label("Suhina");
        harness.step();
        let first = harness.state().first;
        assert_eq!(harness.state().starts, first.map(|f| f + DRAWN_BEFORE_START).into_iter().collect::<Vec<_>>());
        assert_eq!(harness.state().window_frames, 1, "the window is drawn under the fading splash");
        harness.run();
        assert_eq!(harness.state().starts.len(), 1, "once");
    }

    #[test]
    fn it_fades_out_and_goes_away_once_started() {
        let mut harness = harness(WINDOW, true);
        for _ in 0..=DRAWN_BEFORE_START {
            harness.step();
        }
        assert!(harness.state().splash.is_some(), "still fading");
        harness.run();
        assert!(harness.state().splash.is_none(), "gone after the fade");
        assert!(harness.query_by_label("Suhina").is_none());
    }

    #[test]
    fn without_animations_it_goes_at_once() {
        let mut harness = harness(WINDOW, false);
        for _ in 0..=DRAWN_BEFORE_START {
            harness.step();
        }
        assert_eq!(harness.state().starts.len(), 1);
        assert!(harness.state().splash.is_none(), "no fade with animations off");
    }

    #[test]
    fn it_fits_a_window_smaller_than_itself() {
        let size = Vec2::new(200.0, 120.0);
        let mut harness = harness(size, true);
        harness.step();
        let window = Rect::from_min_size(egui::Pos2::ZERO, size);
        for label in ["Suhina", "Version 1.2.3"] {
            let rect = harness.get_by_label(label).rect();
            assert!(window.contains_rect(rect), "{label}: {rect:?}");
        }
    }
}
