// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use suhina_engine::{AudioData, AudioPool, RenderInputs, Renderer};
use suhina_model::{AudioRegion, ContentKinds, LaneData, Project};
use suhina_multiplayer::{Operation, Service, WireEdit};
use suhina_session::{Player, Session};

fn source() -> (Project, AudioPool) {
    let mut project = suhina_multiplayer::starter_project();
    for content in &mut project.contents {
        for lane in &mut content.lanes {
            if let LaneData::Notes(notes) = &mut lane.data {
                notes.clear();
            }
        }
    }
    project.tracks[0].content = ContentKinds { notes: false, audio: true };
    project.tracks[1].content = ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new("/a-file-only-on-the-host/recording.flac"));
    project.contents[1].lanes[0].data = LaneData::Audio(AudioRegion::new(""));
    for lane in &mut project.arrangement.lanes {
        for clip in &mut lane.clips {
            clip.length = 8.0;
        }
    }
    let mut audio = AudioPool::new();
    audio.files.insert(
        "/a-file-only-on-the-host/recording.flac".into(),
        Arc::new(AudioData {
            sample_rate: 8000,
            channels: 2,
            samples: (0..24000)
                .flat_map(|i| {
                    let x = (i as f32 * 0.1).sin();
                    [x * 0.2, -x * 0.35]
                })
                .collect(),
        }),
    );
    (project, audio)
}
fn wait(service: &mut Service, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = service.execute("multiplayer.state", &json!({})).unwrap();
        assert_ne!(state["mode"], "error", "{}", state["error"]);
        if ready(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "audio session timed out: mode={}, error={}", state["mode"], state["error"]);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn apply(session: &mut Session, value: &Value, revision: &mut u64) {
    let operations: Vec<Operation> = serde_json::from_value(value["operations"].clone()).unwrap();
    for op in operations {
        assert_eq!(op.revision, *revision + 1);
        let (id, params) = op.edit.command();
        session.execute(id, &params).unwrap();
        *revision = op.revision;
    }
    assert_eq!(value["controls"]["revision"], *revision);
}
fn submit(service: &mut Service, base: u64, edit: WireEdit) {
    service
        .execute("multiplayer.edit", &json!({"base_revision":base,"edit":serde_json::to_string(&edit).unwrap()}))
        .unwrap();
}
fn render(session: &Session) -> Vec<f32> {
    suhina_engine::render_offline_from(session.project(), RenderInputs::from_audio(session.audio()), 8000, 512, None)
        .unwrap()
}

#[test]
fn missing_guest_files_cross_tls_sound_identical_and_survive_disconnect_save_and_reopen() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, audio) = source();
    let before = project.clone();
    let original =
        suhina_engine::render_offline_from(&project, RenderInputs::from_audio(&audio), 8000, 512, None).unwrap();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &audio).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session();
    snapshot.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let folder = snapshot.folder().unwrap().to_path_buf();
    let mut remote = snapshot.session();
    snapshot.keep();
    assert_eq!(local.project(), remote.project());
    assert_eq!(remote.audio().files.len(), 1);
    let asset = remote.audio().files.keys().next().unwrap();
    assert!(asset.starts_with("Audio/"));
    assert!(folder.join(asset).is_file());
    assert!(
        !serde_json::to_string(&suhina_project_file::to_json(remote.project()))
            .unwrap()
            .contains("a-file-only-on-the-host")
    );
    assert!(!Arc::ptr_eq(audio.files.values().next().unwrap(), remote.audio().files.values().next().unwrap()));
    assert_eq!(render(&local), original);
    assert_eq!(render(&remote), original);
    assert!(original.iter().any(|s| s.abs() > 0.001));
    let mut a = Renderer::stopped(local.project(), local.audio(), 8000).unwrap();
    let mut b = Renderer::stopped(remote.project(), remote.audio(), 8000).unwrap();
    local.attach_player(Player::new(a.controller(), None));
    remote.attach_player(Player::new(b.controller(), None));
    local.execute("transport.setPosition", &json!({"beats":1})).unwrap();
    remote.execute("transport.setPosition", &json!({"beats":5})).unwrap();
    local.execute("transport.play", &json!({})).unwrap();
    remote.execute("transport.play", &json!({})).unwrap();
    let clip = local.project().arrangement.lanes[0].clips[0].id.0;
    submit(&mut guest, 0, WireEdit::ClipMove { clip, start: 4.0, track: 2 });
    let (mut ar, mut br) = (0, 0);
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 1), &mut ar);
    apply(&mut remote, &wait(&mut guest, |s| s["controls"]["revision"] == 1), &mut br);
    submit(&mut host, 1, WireEdit::ClipTrim { clip, start: 4.0, end: 8.0 });
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 2), &mut ar);
    apply(&mut remote, &wait(&mut guest, |s| s["controls"]["revision"] == 2), &mut br);
    assert_eq!(local.project(), remote.project());
    assert_eq!(render(&local), render(&remote));
    let mut out = [0.0; 512];
    a.process(&mut out);
    b.process(&mut out);
    assert!(a.monitor().running() && b.monitor().running());
    assert!((a.monitor().beat() - b.monitor().beat()).abs() > 3.0);
    local.execute("transport.stop", &json!({})).unwrap();
    a.process(&mut out);
    b.process(&mut out);
    assert!(!a.monitor().running() && b.monitor().running());
    host.execute("multiplayer.stop", &json!({})).unwrap();
    let saved = folder.join("Saved.suhina");
    remote.save(&saved, false).unwrap();
    drop(host);
    drop(guest);
    drop(local);
    drop(remote);
    let reopened = suhina_project_file::read_file(&saved).unwrap();
    let (audio, warnings) = suhina_media::load_project_audio(&reopened, &folder);
    assert!(warnings.is_empty(), "{warnings:?}");
    let output =
        suhina_engine::render_offline_from(&reopened, RenderInputs::from_audio(&audio), 8000, 512, None).unwrap();
    assert!(output.iter().any(|s| s.abs() > 0.001));
    assert_eq!(project, before);
}

#[test]
fn late_guest_receives_audio_for_the_current_edited_arrangement() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, audio) = source();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &audio).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session();
    snapshot.keep();
    let clip = project.arrangement.lanes[0].clips[0].id.0;
    submit(&mut host, 0, WireEdit::ClipMove { clip, start: 2.0, track: 2 });
    let mut revision = 0;
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 1), &mut revision);
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let received = guest.take_shared().unwrap();
    assert_eq!(received.revision, 1);
    assert_eq!(received.project, local.project().clone());
    assert_eq!(render(&received.session()), render(&local));
}
