// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::dj_playlist_tests::{click, fixture, harness, settle};
use super::*;
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn grid_buttons_preview_save_restore_and_confirm_deletion_without_editing_audio_or_cues() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(dir.path()));
    settle(&mut h);
    let project = h.state().session().unwrap().project().clone();
    let file = dir.path().join("dj-library.json");
    let before = std::fs::read(&file).unwrap();
    click(&mut h, "Prepare track 1");
    h.state_mut().dj.seek = 0.25;
    click(&mut h, "Use start as first beat");
    click(&mut h, "Half grid tempo");
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().bpm, 60.0);
    click(&mut h, "Double grid tempo");
    click(&mut h, "Preview beat grid");
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert_eq!(h.state().dj.snapshot["grids"], json!([]));
    click(&mut h, "Save beat grid");
    assert_eq!(h.state().dj.snapshot["grids"][0]["bpm"], 120.0);
    assert_eq!(h.state().dj.snapshot["grids"][0]["first_beat"], 0.25);
    assert_eq!(h.state().session().unwrap().project(), &project);
    let cues = h.state().dj.snapshot["cues"].clone();
    drop(h);
    let mut app = App::new();
    app.dj.active = true;
    app.set_file_library(suhina_library::Library::new(Some(dir.path().to_owned())));
    let mut h = harness(app);
    settle(&mut h);
    click(&mut h, "Prepare track 1");
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().first, 0.25);
    click(&mut h, "Delete beat grid…");
    click(&mut h, "Cancel grid deletion");
    assert_eq!(h.state().dj.snapshot["grids"].as_array().unwrap().len(), 1);
    click(&mut h, "Delete beat grid…");
    click(&mut h, "Confirm delete grid");
    assert_eq!(h.state().dj.snapshot["grids"], json!([]));
    assert_eq!(h.state().dj.snapshot["cues"], cues);
    assert_eq!(std::fs::read_to_string(dir.path().join("1.wav")).unwrap(), "original audio 1");
}
#[test]
fn grid_drafts_refuse_stale_and_failed_saves_and_never_target_the_next_track() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(dir.path()));
    settle(&mut h);
    click(&mut h, "Prepare track 1");
    h.state_mut().dj.grid_draft.as_mut().unwrap().bpm = 123.0;
    let revision = h.state().dj.snapshot["revision"].clone();
    h.state_mut().projects.execute("dj.playlist.create", &json!({"name":"Concurrent","revision":revision})).unwrap();
    h.state_mut().dj.snapshot["busy"] = json!(true);
    settle(&mut h);
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().bpm, 123.0);
    assert!(h.get_by_label("Save beat grid").accesskit_node().is_disabled());
    click(&mut h, "Refresh grid draft");
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().bpm, 120.0);
    h.state_mut().dj.grid_draft.as_mut().unwrap().bpm = 125.0;
    let file = dir.path().join("dj-library.json");
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    value["revision"] = json!(99);
    std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    click(&mut h, "Save beat grid");
    assert!(h.state().dj.snapshot["error"].is_string());
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().bpm, 125.0);
    click(&mut h, "Close preparation");
    h.get_by_label("DJ collection").click();
    h.run_steps(2);
    h.get_by_label("All tracks").click();
    h.run_steps(2);
    click(&mut h, "Prepare track 2");
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().track, 2);
    assert_eq!(h.state().dj.grid_draft.as_ref().unwrap().bpm, 120.0);
}
#[test]
#[ignore = "requires a GPU adapter; renders a saved grid with an unsaved correction"]
fn render_dj_grid_for_review() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = fixture(dir.path());
    let file = dir.path().join("dj-library.json");
    let mut c: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    c["tracks"][0]["seconds"] = json!(8.0);
    c["tracks"][0]["title"] = json!("Grid preparation");
    std::fs::write(&file, serde_json::to_vec(&c).unwrap()).unwrap();
    let mut audio = vec![0.0; 128000];
    for frame in (2000..64000).step_by(4000) {
        for i in frame..frame + 240 {
            audio[i * 2] = (1.0 - (i - frame) as f32 / 240.0) * 0.8;
            audio[i * 2 + 1] = audio[i * 2] * 0.6;
        }
    }
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(
        &dir.path().join("1.wav"),
        &audio,
        &spec,
        suhina_media::write::WriteMode::Replace,
    )
    .unwrap();
    let ctx = egui::Context::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.dj.next_poll = None;
        app.poll_dj(&ctx);
        if app.dj.snapshot["busy"] == false {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    app.prepare_dj_track(1);
    loop {
        app.dj.next_poll = None;
        app.poll_dj(&ctx);
        if app.dj.snapshot["busy"] == false {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    app.dj.snapshot["grids"] = json!([{"track":1,"bpm":120.0,"first_beat":0.25}]);
    app.dj.snapshot["cues"] =
        json!([{"track":1,"points":[{"id":1,"name":"Intro","seconds":0.25},{"id":2,"name":"Drop","seconds":4.25}]}]);
    let mut h = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 1000.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&h.ctx);
    h.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    h.render().unwrap().save(folder.join("dj-grid.png")).unwrap();
    h.state_mut().dj.grid_draft.as_mut().unwrap().bpm = 125.0;
    h.get_by_label("Preview beat grid").click();
    h.run();
    h.render().unwrap().save(folder.join("dj-grid-draft.png")).unwrap();
}
