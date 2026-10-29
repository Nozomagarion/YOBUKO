// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::dj_playlist_tests::{click, fixture, harness, settle};
use super::*;
use egui_kittest::kittest::Queryable;
#[test]
fn deck_buttons_load_play_pause_and_leave_without_modifying_the_studio() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = fixture(dir.path());
    let file = dir.path().join("dj-library.json");
    let mut c: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    c["tracks"][0]["seconds"] = json!(2.0);
    std::fs::write(&file, serde_json::to_vec(&c).unwrap()).unwrap();
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(
        &dir.path().join("1.wav"),
        &vec![0.4; 32000],
        &spec,
        suhina_media::write::WriteMode::Replace,
    )
    .unwrap();
    let project = suhina_session::blank_project();
    let mut renderer = suhina_engine::Renderer::stopped(&project, &suhina_engine::AudioPool::new(), 8000).unwrap();
    app.set_session(Session::new(project), &[]);
    let id = app.projects.active().unwrap();
    app.projects.get_mut(id).unwrap().attach_player(suhina_session::Player::new(renderer.controller(), None));
    let mut h = harness(app);
    settle(&mut h);
    let before = h.state().session().unwrap().project().clone();
    click(&mut h, "Load track 1 to deck A");
    assert_eq!(h.state().dj.snapshot["performance"]["decks"][0]["track"], 1);
    let mut audio = vec![0.0; 2048];
    renderer.process(&mut audio);
    assert!(audio.iter().all(|s| *s == 0.0));
    click(&mut h, "Play deck A");
    renderer.process(&mut audio);
    assert!(audio[1600] > 0.09);
    click(&mut h, "Load track 1 to deck B");
    click(&mut h, "Play deck B");
    renderer.process(&mut audio);
    assert!(audio[1600] > 0.19);
    click(&mut h, "Pause deck A");
    renderer.process(&mut audio);
    h.state_mut().dj.next_poll = None;
    h.run_steps(3);
    assert_eq!(h.state().dj.snapshot["performance"]["decks"][0]["playing"], false);
    assert_eq!(h.state().session().unwrap().project(), &before);
    h.get_by_label("Studio").click();
    h.run_steps(3);
    renderer.process(&mut audio);
    assert!(audio.iter().all(|s| *s == 0.0));
    assert!(!h.state().dj.decks_owned);
}
#[test]
#[ignore = "requires a GPU adapter; renders the DJ A/B performance panel"]
fn render_dj_decks_for_review() {
    let mut app = App::new();
    app.dj.active = true;
    app.dj.snapshot = json!({"tracks":[],"playlists":[],"cues":[],"grids":[],"busy":false,"revision":0,
        "performance":{"decks":[{"track":1,"title":"Studio render","seconds":240.0,"position":32.0,"playing":true},
        {"track":2,"title":"Next track","seconds":270.0,"position":16.0,"playing":true}],
        "crossfade":0.35,"levelA":1.0,"levelB":0.8,"cueA":false,"cueB":false,"separate_cue":false}});
    let mut h = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 1000.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&h.ctx);
    h.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    h.render().unwrap().save(folder.join("dj-decks.png")).unwrap();
}
