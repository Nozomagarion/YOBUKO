// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::panic)]
use super::dj_playlist_tests::{click, fixture, harness, settle, type_name};
use super::*;
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn cue_buttons_add_rename_move_confirm_delete_and_keep_audio_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(dir.path()));
    settle(&mut h);
    let before = h.state().session().unwrap().project().clone();
    click(&mut h, "Prepare track 1");
    type_name(&mut h, "Cue name", "Intro");
    h.state_mut().dj.cue_seconds = 12.5;
    h.run_steps(2);
    click(&mut h, "Add cue");
    assert_eq!(h.state().dj.snapshot["cues"][0]["points"][0]["seconds"], 12.5);
    click(&mut h, "Select cue 1");
    type_name(&mut h, "Cue name", "Opening");
    click(&mut h, "Save cue name");
    h.state_mut().dj.cue_seconds = 24.0;
    h.run_steps(2);
    click(&mut h, "Move cue to position");
    assert_eq!(h.state().dj.snapshot["cues"][0]["points"][0]["seconds"], 24.0);
    assert_eq!(h.state().session().unwrap().project(), &before);
    drop(h);
    let mut app = App::new();
    app.dj.active = true;
    app.set_file_library(suhina_library::Library::new(Some(dir.path().to_owned())));
    let mut h = harness(app);
    settle(&mut h);
    click(&mut h, "Prepare track 1");
    click(&mut h, "Select cue 1");
    assert_eq!(h.state().dj.cue_name, "Opening");
    assert_eq!(h.state().dj.cue_seconds, 24.0);
    click(&mut h, "Delete selected cue…");
    click(&mut h, "Cancel cue deletion");
    assert_eq!(h.state().dj.snapshot["cues"][0]["points"].as_array().unwrap().len(), 1);
    click(&mut h, "Delete selected cue…");
    click(&mut h, "Confirm delete cue");
    assert_eq!(h.state().dj.snapshot["cues"][0]["points"], json!([]));
    assert_eq!(std::fs::read_to_string(dir.path().join("1.wav")).unwrap(), "original audio 1");
}

#[test]
fn stale_and_failed_cue_writes_keep_drafts_and_never_target_another_track() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(dir.path()));
    settle(&mut h);
    click(&mut h, "Prepare track 1");
    type_name(&mut h, "Cue name", "Unsaved");
    let revision = h.state().dj.snapshot["revision"].as_u64().unwrap();
    h.state_mut().projects.execute("dj.playlist.create", &json!({"name":"Other edit","revision":revision})).unwrap();
    h.state_mut().dj.snapshot["busy"] = json!(true);
    settle(&mut h);
    assert_eq!(h.state().dj.cue_name, "Unsaved");
    assert!(h.get_by_label("Add cue").accesskit_node().is_disabled());
    click(&mut h, "Refresh cue draft");
    type_name(&mut h, "Cue name", "Keep on failure");
    let path = dir.path().join("dj-library.json");
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["revision"] = json!(99);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    click(&mut h, "Add cue");
    assert_eq!(h.state().dj.cue_name, "Keep on failure");
    assert!(h.state().dj.snapshot["error"].is_string());
    click(&mut h, "Close preparation");
    h.get_by_label("DJ collection").click();
    h.run_steps(2);
    h.get_by_label("All tracks").click();
    h.run_steps(2);
    click(&mut h, "Prepare track 2");
    assert!(h.state().dj.cue_name.is_empty());
    assert_eq!(h.state().dj.preparing, Some(2));
}

#[test]
fn audition_buttons_play_seek_and_stop_through_the_existing_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = fixture(dir.path());
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(
        &dir.path().join("1.wav"),
        &vec![0.4; 32000],
        &spec,
        suhina_media::write::WriteMode::Replace,
    )
    .unwrap();
    let file = dir.path().join("dj-library.json");
    let mut catalog: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    catalog["tracks"][0]["seconds"] = json!(2.0);
    std::fs::write(&file, serde_json::to_vec(&catalog).unwrap()).unwrap();
    let project = suhina_session::blank_project();
    let mut renderer = suhina_engine::Renderer::stopped(&project, &suhina_engine::AudioPool::new(), 8000).unwrap();
    app.set_session(Session::new(project), &[]);
    let id = app.projects.active().unwrap();
    app.projects.get_mut(id).unwrap().attach_player(suhina_session::Player::new(renderer.controller(), None));
    let mut h = harness(app);
    settle(&mut h);
    click(&mut h, "Prepare track 1");
    h.state_mut().dj.seek = 0.5;
    h.run_steps(2);
    click(&mut h, "Play from position");
    let mut block = vec![0.0; 1024];
    renderer.process(&mut block);
    assert!(block[600] > 0.19);
    h.state_mut().dj.next_poll = None;
    h.run_steps(3);
    assert_eq!(h.state().dj.snapshot["preview"]["playing"], true);
    click(&mut h, "Use playback position");
    assert!(h.state().dj.cue_seconds > 0.5);
    click(&mut h, "Stop audition");
    renderer.process(&mut block);
    assert!(block.iter().all(|s| *s == 0.0));
    assert_eq!(h.state().dj.snapshot["preview"]["playing"], false);
    click(&mut h, "Play from position");
    h.get_by_label("Studio").click();
    h.run_steps(3);
    renderer.process(&mut block);
    assert!(block.iter().all(|s| *s == 0.0));
    assert!(!h.state().dj.preview_owned);
}

#[test]
#[ignore = "requires a GPU adapter; renders a cue preparation review artifact"]
fn render_dj_cues_for_review() {
    let mut app = App::new();
    app.dj.active = true;
    app.dj.preparing = Some(1);
    app.dj.snapshot = json!({"tracks":[{"id":1,"title":"Night session","seconds":240.0,"bpm":124.0,"key":"Am"}],
        "playlists":[],"cues":[{"track":1,"next_id":3,"points":[{"id":1,"name":"Intro","seconds":0.0},{"id":2,"name":"Main drop","seconds":64.0}]}],
        "revision":2,"busy":false,"preview":{"playing":false,"loading":false,"position":0.0}});
    let mut h = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&h.ctx);
    h.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    h.render().unwrap().save(folder.join("dj-cues.png")).unwrap();
}
