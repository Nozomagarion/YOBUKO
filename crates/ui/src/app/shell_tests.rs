// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::Queryable;

fn window(app: App) -> egui_kittest::Harness<'static, App> {
    egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

fn seen(mode: Option<&str>, guests: u64) -> Seen {
    Seen { mode: mode.map(str::to_owned), guests }
}

#[test]
fn each_space_has_a_tab_and_the_usb_space_is_rbxports_page() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = window(app);
    harness.run();
    assert_eq!(harness.state().workspace_mode(), WorkspaceMode::Studio);
    for space in ["Studio space", "Multiplayer space", "DJ space", "USB space"] {
        assert!(harness.query_by_label(space).is_some(), "{space}");
    }
    let before = harness.state().session().unwrap().project().clone();
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    assert_eq!(harness.state().workspace_mode(), WorkspaceMode::Usb);
    assert!(harness.query_by_label("USB export · rbxport").is_some());
    assert!(harness.query_by_label("Open rbxport").is_some());
    assert!(harness.query_by_label("Open Mixxx").is_none(), "Mixxx's page belongs to the DJ space");
    assert!(harness.state().dj.note.is_some(), "entering the USB space starts rbxport or says why not");
    super::super::menu_bar::pick_mode(&mut harness, "DJ");
    assert_eq!(harness.state().workspace_mode(), WorkspaceMode::Dj);
    assert!(harness.query_by_label("Open Mixxx").is_some());
    harness.get_by_label("Prepare a USB stick (rbxport)").click();
    harness.run();
    assert_eq!(harness.state().workspace_mode(), WorkspaceMode::Usb);
    super::super::menu_bar::pick_mode(&mut harness, "Studio");
    assert_eq!(harness.state().workspace_mode(), WorkspaceMode::Studio);
    assert_eq!(harness.state().session().unwrap().project(), &before);
}

#[test]
fn the_tabs_show_without_the_windows_menu_bar_too() {
    let mut app = App::new();
    app.settings.view.menu_bar = false;
    let mut harness = window(app);
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "Multiplayer");
    assert_eq!(harness.state().workspace_mode(), WorkspaceMode::Multiplayer);
    assert!(harness.query_by_label("Piou").is_some());
}

#[test]
fn piou_tells_what_changed_in_the_shared_session() {
    let hosting = |guests| seen(Some("hosting"), guests);
    assert_eq!(news(&hosting(0), &hosting(1)).map(|n| n.0), Some(Mood::Joy));
    assert_eq!(news(&hosting(2), &hosting(1)), None, "a friend leaving is no news to cheer");
    assert_eq!(news(&seen(Some("starting"), 0), &hosting(0)), None);
    let lost = news(&seen(Some("connected"), 0), &seen(Some("reconnecting"), 0)).unwrap();
    assert_eq!(lost.0, Mood::Lost);
    assert!(lost.1.contains("Reconnecting"));
    let back = news(&seen(Some("reconnecting"), 0), &seen(Some("connected"), 0)).unwrap();
    assert_eq!(back, (Mood::Joy, "Back online. Nothing was lost."));
    assert_eq!(news(&seen(Some("starting"), 0), &seen(Some("connected"), 0)).map(|n| n.0), Some(Mood::Joy));
    assert_eq!(news(&seen(Some("reconnecting"), 0), &seen(Some("disconnected"), 0)).map(|n| n.0), Some(Mood::Lost));
    assert_eq!(news(&seen(None, 0), &seen(None, 0)), None);
}

#[test]
fn a_note_from_piou_shows_for_a_while_then_leaves() {
    let mut harness = window(App::new());
    harness.run();
    assert_eq!(harness.state_mut().piou_mood(&egui::Context::default()), Mood::Idle);
    harness.state_mut().shell.say(Mood::Joy, "Night Echo is in your DJ library.");
    harness.run();
    assert!(harness.query_by_label("Night Echo is in your DJ library.").is_some());
    // Each step is a fixed slice of time: well past the note's few seconds.
    harness.run_steps(400);
    assert!(harness.state().shell.note.is_none());
    assert!(harness.query_by_label("Night Echo is in your DJ library.").is_none());
}

#[test]
fn the_lobby_offers_the_last_session_again_while_its_invitation_lasts() {
    let mut app = App::new();
    app.multiplayer.open = true;
    let mut harness = window(app);
    harness.run();
    assert!(harness.query_by_label("Create session").is_some());
    assert!(harness.query_by_label("Join session").is_some());
    assert!(harness.query_by_label("Rejoin last session").is_none());
    harness.state_mut().multiplayer.last_join = Some(("7-piano-tigre-lune".into(), std::time::Instant::now()));
    harness.run();
    assert!(harness.query_by_label("Rejoin last session").is_some());
    let long_ago = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(31 * 60));
    if let Some(long_ago) = long_ago {
        harness.state_mut().multiplayer.last_join = Some(("7-piano-tigre-lune".into(), long_ago));
        harness.run();
        assert!(harness.query_by_label("Rejoin last session").is_none());
    }
}

#[test]
#[ignore = "requires a GPU adapter; renders review artifacts"]
fn render_the_shell_for_review() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    let theme_id = std::env::var("YOBUKO_SHELL_THEME").unwrap_or_else(|_| theme::YOBUKO.to_owned());
    let shoot = |name: &str, set: &dyn Fn(&mut App)| {
        let mut app = App::new();
        app.settings.theme = theme_id.clone();
        app.set_session(Session::new(crate::demo::demo_project()), &[]);
        set(&mut app);
        let mut harness = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .with_size(Vec2::new(1280.0, 800.0))
            .with_pixels_per_point(1.5)
            .wgpu()
            .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
        theme::install(&harness.ctx);
        crate::kit::shape::set_round(crate::kit::shape::theme_is_round(&theme_id));
        theme::set_palette(&harness.ctx, crate::settings::themes::Themes::default().palette(&theme_id));
        harness.run_steps(6);
        harness.render().unwrap().save(folder.join(format!("shell-{name}.png"))).unwrap();
    };
    shoot("studio", &|app| app.shell.say(Mood::Joy, "A friend joined your session."));
    shoot("multiplayer", &|app| {
        app.multiplayer.open = true;
        app.multiplayer.last_join = Some(("7-piano-tigre-lune".into(), std::time::Instant::now()));
    });
    shoot("dj", &|app| app.dj.active = true);
    shoot("usb", &|app| {
        app.dj.active = true;
        app.dj.usb = true;
    });
}
