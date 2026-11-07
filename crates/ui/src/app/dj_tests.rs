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
    assert!(harness.query_by_label("DJ mode · rbxport").is_some());
    assert!(harness.state().dj.note.is_some(), "switching to DJ starts rbxport or says why not");
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
    assert!(harness.query_by_label("Open rbxport").is_some());
    assert!(harness.query_by_label("App mode: DJ").is_some());
}

#[test]
fn a_project_rendered_for_dj_mode_goes_to_rbxport_even_when_another_tab_is_selected() {
    let inbox = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.dj.inbox = Some(inbox.path().to_path_buf());
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    app.dj.next_export_to_dj = true;
    app.open_export();
    assert!(!app.dj.next_export_to_dj, "a normal export must not go to rbxport");
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
