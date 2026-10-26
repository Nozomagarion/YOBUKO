// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::Queryable;
use std::time::{Duration, Instant};

#[test]
fn the_visible_lan_entry_creates_a_separate_tab_and_keeps_other_projects() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let original = app.projects.active().unwrap();
    app.settings.view.menu_bar = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Multiplayer").click();
    harness.run();
    assert!(harness.query_by_label("Create LAN session").is_some());
    harness.get_by_label("Create LAN session").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    assert_ne!(original, shared);
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    assert!(harness.query_by_label("Copy invitation").is_some());
    assert!(harness.query_by_label("Play locally").is_some());
    harness.get_by_label("Leave session").click();
    harness.run_steps(3);
    assert!(harness.state().multiplayer.bound.is_none());
    assert!(harness.state().projects.get(shared).is_some());
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
}

#[test]
fn two_desktop_projects_apply_confirmed_controls_and_unlock_on_disconnect() {
    let ctx = egui::Context::default();
    let mut host = App::new();
    let mut guest = App::new();
    host.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.multiplayer.invitation.is_empty() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let local = host.multiplayer.bound.unwrap();
    let remote = guest.multiplayer.bound.unwrap();
    guest.multiplayer_command("multiplayer.edit", json!({"base_revision":0,"edit":json!({"command":"track.rename","track":1,"name":"From the other computer"}).to_string()}));
    while host.multiplayer.controls.as_ref().is_none_or(|c| c.revision < 1) && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(host.projects.get(local).unwrap().project().tracks[0].name, "From the other computer");
    assert!(host.projects.execute("track.rename", &json!({"track":1,"name":"Bypass"})).is_err());
    host.leave_multiplayer();
    assert!(host.projects.execute("track.rename", &json!({"track":1,"name":"Local"})).is_ok());
    while guest.multiplayer.service.active() && Instant::now() < deadline {
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    guest.poll_multiplayer(&ctx);
    assert!(guest.multiplayer.bound.is_none());
    assert_eq!(guest.projects.get(remote).unwrap().project().tracks[0].name, "From the other computer");
    assert!(guest.projects.execute("track.rename", &json!({"track":1,"name":"Retained"})).is_ok());
}

#[test]
#[ignore = "requires a GPU adapter; renders the LAN entry point"]
fn render_multiplayer_panel_for_review() {
    let mut app = App::new();
    app.set_session(Session::new(suhina_multiplayer::starter_project()), &[]);
    app.multiplayer.address = "127.0.0.1:0".into();
    app.multiplayer.open = true;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&harness.ctx);
    harness.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    harness.render().unwrap().save(folder.join("multiplayer-lan.png")).unwrap();
    harness.get_by_label("Share current MIDI project").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(harness.state().multiplayer.bound.is_some());
    harness.run_steps(2);
    harness.render().unwrap().save(folder.join("multiplayer-midi-copy.png")).unwrap();
}

#[test]
fn sharing_the_current_midi_project_opens_a_copy_of_its_real_music() {
    let mut app = App::new();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].name = "My own composition".into();
    project.transport.tempo_bpm = 137.0;
    if let suhina_engine::model::LaneData::Notes(notes) = &mut project.contents[0].lanes[0].data {
        notes[0].key = 77;
    }
    let folder = tempfile::tempdir().unwrap();
    let source_file = folder.path().join("Original.suhina");
    let mut session = Session::new(project);
    session.execute("project.save", &json!({"path":source_file})).unwrap();
    let original_bytes = std::fs::read(&source_file).unwrap();
    app.set_session(session, &[]);
    app.multiplayer.open = true;
    app.multiplayer.address = "127.0.0.1:0".into();
    let original = app.projects.active().unwrap();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Share current MIDI project").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(2);
    let rows: Vec<_> = harness.query_all_by_label("Set volume").collect();
    assert_eq!(rows.len(), before.tracks.len());
    let mut previous_y = f32::NEG_INFINITY;
    for button in rows {
        assert!(button.rect().left() < 800.0, "track controls must fit in the viewport");
        assert!(button.rect().top() > previous_y, "each track must occupy its own grid row");
        previous_y = button.rect().top();
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    assert_ne!(shared, original);
    let session = harness.state().projects.get(shared).unwrap();
    assert!(session.has_unsaved_changes(), "received music must prompt for a first save");
    assert!(session.source().is_none(), "a copy must not inherit the original's file path");
    let copy = session.project();
    assert_eq!(copy.tracks[0].name, "My own composition");
    assert_eq!(copy.transport.tempo_bpm, 137.0);
    assert_eq!(copy.contents, before.contents);
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    let copy_file = folder.path().join("Shared copy.suhina");
    harness.state_mut().projects.execute_in(Some(shared), "project.save", &json!({"path":copy_file})).unwrap();
    assert!(!harness.state().projects.get(shared).unwrap().has_unsaved_changes());
    assert_eq!(std::fs::read(&source_file).unwrap(), original_bytes);
    assert_eq!(harness.state().projects.get(original).unwrap().source(), Some(source_file.as_path()));
    harness.get_by_label("Leave session").click();
    harness.run_steps(3);
    assert!(harness.state().projects.get(shared).is_some());
}

#[test]
fn an_audio_project_is_refused_without_opening_a_listener_or_replacing_its_tab() {
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].content.audio = true;
    let mut app = App::new();
    app.set_session(Session::new(project), &[]);
    app.multiplayer.open = true;
    let original = app.projects.active().unwrap();
    let before = app.projects.get(original).unwrap().project().clone();
    app.host_current_midi();
    assert!(app.multiplayer.error.as_ref().is_some_and(|e| e.contains("audio")));
    assert!(!app.multiplayer.service.active());
    assert!(app.multiplayer.bound.is_none());
    assert_eq!(app.projects.active(), Some(original));
    assert_eq!(app.projects.get(original).unwrap().project(), &before);
}
