// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::Queryable;

#[test]
fn dj_mode_is_available_without_a_project_and_returning_keeps_the_project_intact() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().session().unwrap().project().clone();
    super::menu_bar::pick_mode(&mut harness, "DJ");
    harness.run();
    assert!(harness.state().dj.active);
    assert!(harness.query_by_label("DJ mode · Mixxx").is_some());
    assert!(harness.state().dj.note.is_some(), "switching to DJ starts Mixxx or says why not");
    super::menu_bar::pick_mode(&mut harness, "Studio");
    harness.run();
    assert!(!harness.state().dj.active);
    assert_eq!(harness.state().session().unwrap().project(), &before);
    let mut app = App::new();
    app.dj.active = true;
    app.settings.view.menu_bar = false;
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    assert!(harness.query_by_label("Open Mixxx").is_some());
    assert!(harness.query_by_label("Prepare a USB stick (rbxport)").is_some());
    assert!(harness.query_by_label("DJ space").is_some());
}

#[test]
fn quitting_mixxx_brings_the_studio_back_with_the_project_intact() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    super::menu_bar::pick_mode(&mut harness, "DJ");
    harness.run();
    let before = harness.state().session().unwrap().project().clone();
    // A Mixxx that ran and has now closed.
    fn ran() -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LOOKS: AtomicUsize = AtomicUsize::new(0);
        LOOKS.fetch_add(1, Ordering::SeqCst) == 0
    }
    let watch = super::dj::mixxx::Watch::start_with(egui::Context::default(), ran);
    let since = std::time::Instant::now();
    while !watch.closed() {
        assert!(since.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    harness.state_mut().dj.mixxx_watch = Some(watch);
    harness.run();
    assert!(!harness.state().dj.active, "the studio is back once Mixxx closes");
    assert!(harness.state().dj.mixxx_watch.is_none());
    assert_eq!(harness.state().session().unwrap().project(), &before);
}

#[test]
fn a_project_rendered_for_dj_mode_joins_the_dj_library_even_when_another_tab_is_selected() {
    let inbox = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.dj.inbox = Some(inbox.path().to_path_buf());
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    app.dj.next_export_to_dj = true;
    app.open_export();
    assert!(!app.dj.next_export_to_dj, "a normal export must not go to the DJ library");
    app.dj.running_export_to_dj = true;
    let id = app.projects.active().unwrap();
    app.export_project = Some((id, "original".into()));
    let (done, receive) = std::sync::mpsc::channel();
    app.export = Some(receive);
    let other = app.projects.add(Session::new(crate::demo::demo_project()), Some("Another".into())).unwrap();
    assert_ne!(app.projects.active(), Some(id));
    assert_eq!(app.projects.active(), Some(other));
    let audio = std::env::temp_dir().join("rendered.wav");
    done.send(Ok(json!({"path": audio, "warnings": []}))).unwrap();
    app.poll_export(&egui::Context::default());
    assert!(app.dj.active);
    assert!(!app.dj.running_export_to_dj);
    let requests: Vec<_> = std::fs::read_dir(inbox.path()).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(std::fs::read_to_string(&requests[0]).unwrap(), format!("{}\n", audio.display()));
}

#[test]
#[ignore = "requires a GPU adapter; renders a review artifact"]
fn render_dj_workspace_for_review() {
    let mut app = App::new();
    app.dj.active = true;
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, 800.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&harness.ctx);
    harness.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    harness.render().unwrap().save(folder.join("dj-workspace.png")).unwrap();
}
