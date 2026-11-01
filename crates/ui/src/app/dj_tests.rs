// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::Queryable;

#[test]
fn dj_mode_is_available_without_a_project_and_returning_keeps_the_project_intact() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().session().unwrap().project().clone();
    harness.get_by_label("DJ").click();
    harness.run();
    assert!(harness.state().dj.active);
    assert!(harness.query_by_label("DJ library").is_some());
    harness.get_by_label("Studio").click();
    harness.run();
    assert!(!harness.state().dj.active);
    assert_eq!(harness.state().session().unwrap().project(), &before);
    let mut app = App::new();
    app.dj.active = true;
    app.settings.view.menu_bar = false;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    assert!(harness.query_by_label("Import audio…").is_some());
    assert!(harness.query_by_label("Studio").is_some());
    assert!(harness.query_by_label("DJ").is_some());
}

#[test]
fn rendering_to_dj_retains_the_original_source_when_another_tab_is_selected() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    app.dj.next_export_to_dj = true;
    app.open_export();
    assert!(!app.dj.next_export_to_dj, "a normal export must not publish into the DJ library");
    app.dj.running_export_to_dj = true;
    app.dj.export_source = Some(std::env::temp_dir().join("original.suhina"));
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
    assert_eq!(app.dj.pending, Some((audio, Some(std::env::temp_dir().join("original.suhina")))));
    assert!(app.dj.active);
}

#[test]
fn a_picked_audio_file_is_analysed_and_persisted_without_an_open_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    let mono: Vec<f32> = (0..16000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 8000.0).sin() * 0.2).collect();
    let stereo: Vec<f32> = mono.iter().flat_map(|s| [*s, *s]).collect();
    suhina_media::write::write_audio_file(&path, &stereo, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    let mut app = App::new();
    app.set_file_library(suhina_library::Library::new(Some(dir.path().to_path_buf())));
    app.dj.active = true;
    app.set_picked(PickField::DjAudio, path.clone());
    let ctx = egui::Context::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.dj.next_poll = None;
        app.poll_dj(&ctx);
        if app.dj.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 1) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(app.dj.snapshot["tracks"][0]["title"], "tone", "{}", app.dj.snapshot);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("dj-library.json")).unwrap()).unwrap();
    assert_eq!(saved["tracks"][0]["path"], path.canonicalize().unwrap().to_string_lossy().as_ref());
    assert!(app.session().is_none());
}

#[test]
#[ignore = "requires a GPU adapter; renders a review artifact"]
fn render_dj_workspace_for_review() {
    let mut app = App::new();
    app.dj.active = true;
    app.dj.snapshot = json!({"tracks": [], "busy": false, "error": null});
    let mut harness = egui_kittest::Harness::builder()
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

#[test]
fn a_picked_usb_stick_receives_a_rekordbox_export_of_the_dj_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    let spec = suhina_media::write::AudioFileSpec::float_wav(44_100);
    let mono: Vec<f32> =
        (0..88_200).map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / 44_100.0).sin() * 0.2).collect();
    let stereo: Vec<f32> = mono.iter().flat_map(|s| [*s, *s]).collect();
    suhina_media::write::write_audio_file(&path, &stereo, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    let stick = dir.path().join("STICK");
    std::fs::create_dir(&stick).unwrap();
    let mut app = App::new();
    app.set_file_library(suhina_library::Library::new(Some(dir.path().to_path_buf())));
    app.dj.active = true;
    let ctx = egui::Context::default();
    let wait = |app: &mut App, done: &dyn Fn(&serde_json::Value) -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            app.dj.next_poll = None;
            app.poll_dj(&ctx);
            if done(&app.dj.snapshot) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("DJ worker timed out: {}", app.dj.snapshot);
    };
    app.set_picked(PickField::DjAudio, path);
    wait(&mut app, &|s| s["tracks"].as_array().is_some_and(|t| t.len() == 1) && s["busy"] != true);

    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    assert!(harness.query_by_label("Open rbxport").or_else(|| harness.query_by_label("Get rbxport")).is_some());
    harness.get_by_label("Export to USB stick (Rekordbox)…").click();
    harness.run();
    let request = harness.state_mut().take_pick_request().unwrap();
    assert_eq!(request.field, PickField::DjUsb);
    assert!(request.field.folder());
    harness.state_mut().set_picked(PickField::DjUsb, stick.clone());
    let mut app = harness.into_state();
    wait(&mut app, &|s| s["busy"] != true && (!s["usb"]["summary"].is_null() || !s["usb"]["error"].is_null()));
    assert!(app.dj.snapshot["usb"]["error"].is_null(), "{}", app.dj.snapshot);
    assert_eq!(app.dj.snapshot["usb"]["summary"]["tracks"], 1);
    assert!(stick.join("PIONEER/rekordbox/export.pdb").is_file());
}
