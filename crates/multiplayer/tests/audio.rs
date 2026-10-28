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
    let mut local = snapshot.session().unwrap();
    snapshot.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let folder = snapshot.folder().unwrap().to_path_buf();
    let mut remote = snapshot.session().unwrap();
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
    let mut local = snapshot.session().unwrap();
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
    assert_eq!(render(&received.session().unwrap()), render(&local));
}

fn accept_audio(service: &mut Service, session: &mut Session) -> usize {
    let mut count = 0;
    for batch in service.take_audio() {
        count += batch.audio.files.len();
        session.add_collected_audio(&batch.audio, batch.folder().unwrap()).unwrap();
        batch.keep();
    }
    count
}
fn publish_take(service: &mut Service, data: Arc<AudioData>, base: u64, track: u32, start: f64) {
    service.select_take(data).unwrap();
    service.execute("multiplayer.publishTake", &json!({"base_revision":base,"track":track,"start":start})).unwrap();
}

#[test]
fn new_takes_publish_without_reconnecting_reuse_sound_and_preserve_players_and_saved_files() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, audio) = source();
    let source_before = project.clone();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &audio).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session().unwrap();
    snapshot.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let folder = snapshot.folder().unwrap().to_path_buf();
    let mut remote = snapshot.session().unwrap();
    snapshot.keep();
    let mut a = Renderer::stopped(local.project(), local.audio(), 8000).unwrap();
    let mut b = Renderer::stopped(remote.project(), remote.audio(), 8000).unwrap();
    local.attach_player(Player::new(a.controller(), None));
    remote.attach_player(Player::new(b.controller(), None));
    local.execute("transport.setPosition", &json!({"beats":2})).unwrap();
    remote.execute("transport.setPosition", &json!({"beats":7})).unwrap();
    local.execute("transport.play", &json!({})).unwrap();
    remote.execute("transport.play", &json!({})).unwrap();
    let take = Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 8000] });
    assert!(guest.select_take(Arc::clone(&take)).is_ok());
    // The raw operation is not a path/upload backdoor, even through the owner's ordinary edit entry.
    let raw = WireEdit::AudioInsert {
        track: 2,
        start: 8.0,
        file: format!("Audio/{}.wav", "a".repeat(64)),
        rate: 8000,
        channels: 1,
        frames: 8000,
    };
    for service in [&mut host, &mut guest] {
        assert!(
            service
                .execute("multiplayer.edit", &json!({"base_revision":0,"edit":serde_json::to_string(&raw).unwrap()}))
                .is_err()
        );
    }
    let (mut ar, mut br) = (0, 0);
    for revision in 1..=2 {
        publish_take(&mut host, Arc::clone(&take), revision - 1, 2, 8.0 + (revision - 1) as f64 * 4.0);
        let state = wait(&mut host, |s| s["controls"]["revision"] == revision);
        assert_eq!(accept_audio(&mut host, &mut local), usize::from(revision == 1));
        apply(&mut local, &state, &mut ar);
        let state = wait(&mut guest, |s| s["controls"]["revision"] == revision);
        assert_eq!(
            accept_audio(&mut guest, &mut remote),
            usize::from(revision == 1),
            "verified sound is not transferred twice"
        );
        apply(&mut remote, &state, &mut br);
        assert_eq!(local.project(), remote.project());
        assert_eq!(local.audio().files.len(), 2);
        assert_eq!(render(&local), render(&remote));
    }
    let mut out = [0.0; 512];
    a.process(&mut out);
    b.process(&mut out);
    assert!(a.monitor().running() && b.monitor().running());
    assert!((a.monitor().beat() - b.monitor().beat()).abs() > 4.0);
    // Guest can edit the just-published clip through the same contiguous revision journal.
    let inserted = local.project().arrangement.lanes.iter().flat_map(|l| &l.clips).max_by_key(|c| c.id).unwrap().id.0;
    submit(&mut guest, 2, WireEdit::ClipMove { clip: inserted, start: 16.0, track: 1 });
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 3), &mut ar);
    apply(&mut remote, &wait(&mut guest, |s| s["controls"]["revision"] == 3), &mut br);
    assert_eq!(local.project(), remote.project());
    let rendered = render(&remote);
    host.execute("multiplayer.stop", &json!({})).unwrap();
    guest.execute("multiplayer.stop", &json!({})).unwrap();
    let saved = folder.join("Saved-with-new-takes.suhina");
    remote.save(&saved, false).unwrap();
    let trusted_paths = remote.loaded_audio_paths();
    assert_eq!(trusted_paths.len(), 2);
    assert!(trusted_paths.iter().all(|p| p.is_file()));
    drop(remote);
    let reopened = suhina_project_file::read_file(&saved).unwrap();
    // Native trust is explicit, as for local files saved outside a project's own folder.
    let trusted = suhina_media::Trusted { files: trusted_paths.into_iter().collect(), ..Default::default() };
    let (pool, warnings) = suhina_media::load_project_audio_trusted(&reopened, &folder, &trusted);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        suhina_engine::render_offline_from(&reopened, RenderInputs::from_audio(&pool), 8000, 512, None).unwrap(),
        rendered
    );
    assert_eq!(project, source_before);
    assert_eq!(take.samples, vec![0.25; 8000]);
}

#[test]
fn rejected_publication_keeps_revision_and_late_join_gets_the_new_take() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, audio) = source();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &audio).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    let initial = host.take_shared().unwrap();
    let mut local = initial.session().unwrap();
    initial.keep();
    let data = Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.125; 4000] });
    for (base, track) in [(3, 1), (0, 3)] {
        publish_take(&mut host, Arc::clone(&data), base, track, 0.0);
        let rejected = wait(&mut host, |s| s["error"].is_string());
        assert_eq!(rejected["controls"]["revision"], 0);
        assert!(host.take_audio().is_empty());
        assert_eq!(std::fs::read_dir(host_dir.path()).unwrap().count(), 1, "failed take folders are removed");
    }
    publish_take(&mut host, Arc::clone(&data), 0, 2, 4.0);
    let state = wait(&mut host, |s| s["controls"]["revision"] == 1);
    accept_audio(&mut host, &mut local);
    let mut rev = 0;
    apply(&mut local, &state, &mut rev);
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let remote = snapshot.session().unwrap();
    snapshot.keep();
    assert_eq!(local.project(), remote.project());
    assert_eq!(render(&local), render(&remote));
    assert_eq!(remote.audio().files.len(), 2);
}

#[test]
fn a_native_microphone_take_recorded_after_join_can_be_published_without_changing_its_source() {
    use std::sync::atomic::AtomicU64;
    use suhina_model::{AudioInput, InputChannels, Monitor};
    let root = tempfile::tempdir().unwrap();
    let receive = tempfile::tempdir().unwrap();
    let (mut project, _) = source();
    project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new(""));
    project.tracks[0].audio_input = AudioInput { channels: InputChannels::Mono(1), monitor: Monitor::Off };
    let mut studio = Session::new(project);
    studio.save(&root.path().join("Original.suhina"), false).unwrap();
    let mut host = Service::default();
    host.set_storage_root(Some(root.path().join("Host"))).unwrap();
    host.select_audio_project(studio.project(), studio.audio()).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session().unwrap();
    snapshot.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(receive.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let mut remote = snapshot.session().unwrap();
    snapshot.keep();
    // Drive the normal recorder with a synthetic microphone, after both services have joined.
    let mut renderer = Renderer::stopped(studio.project(), studio.audio(), 8000).unwrap();
    let (mut input, consumer) = rtrb::RingBuffer::new(32768);
    renderer.set_audio_input(consumer, 1, Arc::new(AtomicU64::new(0)));
    studio.attach_player(Player::new(renderer.controller(), None));
    studio.execute("track.setArm", &json!({"track":1,"armed":true})).unwrap();
    studio.execute("transport.play", &json!({})).unwrap();
    let mut process = |blocks| {
        for _ in 0..blocks {
            for _ in 0..256 {
                input.push(0.25).unwrap();
            }
            renderer.process(&mut [0.0; 512]);
        }
    };
    process(2);
    studio.execute("transport.record", &json!({"on":true})).unwrap();
    process(24);
    studio.execute("transport.record", &json!({"on":false})).unwrap();
    process(8);
    let deadline = Instant::now() + Duration::from_secs(5);
    while studio.is_recording() && Instant::now() < deadline {
        let notices = studio.poll_recording();
        assert!(notices.is_empty(), "{notices:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!studio.is_recording());
    let (file, data) = studio.audio().files.iter().find(|(key, _)| key.contains("Recorded")).unwrap();
    let bytes = std::fs::read(file).unwrap();
    let before = studio.project().clone();
    publish_take(&mut guest, Arc::clone(data), 0, 2, 16.0);
    let state = wait(&mut host, |s| s["controls"]["revision"] == 1);
    accept_audio(&mut host, &mut local);
    apply(&mut local, &state, &mut 0);
    let state = wait(&mut guest, |s| s["controls"]["revision"] == 1);
    accept_audio(&mut guest, &mut remote);
    apply(&mut remote, &state, &mut 0);
    assert_eq!(render(&local), render(&remote));
    assert!(render(&remote).iter().any(|s| s.abs() > 0.001));
    assert_eq!(studio.project(), &before);
    assert_eq!(std::fs::read(file).unwrap(), bytes);
    assert_eq!(remote.audio().files.len(), 1);
}

#[test]
fn a_guest_adds_an_audio_track_and_the_host_publishes_a_new_sound_to_it_without_rejoining() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.execute("multiplayer.host", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session().unwrap();
    snapshot.keep();
    let original = local.project().clone();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let mut remote = snapshot.session().unwrap();
    snapshot.keep();
    submit(&mut guest, 0, WireEdit::TrackCreate { kind: "audio".into(), name: "New vocal take".into() });
    let mut ar = 0;
    let mut br = 0;
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 1), &mut ar);
    apply(&mut remote, &wait(&mut guest, |s| s["controls"]["revision"] == 1), &mut br);
    let track = local.project().tracks.iter().find(|t| t.name == "New vocal take").unwrap().id.0;
    publish_take(
        &mut host,
        Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 8000] }),
        1,
        track,
        4.0,
    );
    let state = wait(&mut host, |s| s["controls"]["revision"] == 2);
    accept_audio(&mut host, &mut local);
    apply(&mut local, &state, &mut ar);
    let state = wait(&mut guest, |s| s["controls"]["revision"] == 2);
    accept_audio(&mut guest, &mut remote);
    apply(&mut remote, &state, &mut br);
    assert_eq!(local.project(), remote.project());
    assert_eq!(local.project().tracks.len(), original.tracks.len() + 1);
    assert_eq!(local.audio().files.len(), 1);
    assert_eq!(render(&local), render(&remote));
    let lane =
        remote.project().arrangement.lanes.iter().find(|l| l.track == Some(suhina_model::TrackId(track))).unwrap();
    assert_eq!(lane.clips.len(), 1);
    assert_eq!(lane.clips[0].start, 4.0);
}

#[test]
fn guest_uploads_reuse_verified_sounds_preserve_players_and_save_both_copies() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let before = project.clone();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &pool).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let invitation = wait(&mut host, |s| s["mode"] == "hosting")["invitation"].clone();
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session().unwrap();
    let host_folder = snapshot.folder().unwrap().to_path_buf();
    snapshot.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let mut remote = snapshot.session().unwrap();
    let guest_folder = snapshot.folder().unwrap().to_path_buf();
    snapshot.keep();
    let mut a = Renderer::stopped(local.project(), local.audio(), 8000).unwrap();
    let mut b = Renderer::stopped(remote.project(), remote.audio(), 8000).unwrap();
    local.attach_player(Player::new(a.controller(), None));
    remote.attach_player(Player::new(b.controller(), None));
    local.execute("transport.setPosition", &json!({"beats":1})).unwrap();
    remote.execute("transport.setPosition", &json!({"beats":7})).unwrap();
    local.execute("transport.play", &json!({})).unwrap();
    remote.execute("transport.play", &json!({})).unwrap();
    let take = Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 12000] });
    let (mut ar, mut br) = (0, 0);
    for revision in 1..=2 {
        publish_take(&mut guest, Arc::clone(&take), revision - 1, 2, 8.0 + revision as f64 * 4.0);
        assert!(
            guest.execute("multiplayer.publishTake", &json!({"base_revision":revision-1,"track":2,"start":0})).is_err(),
            "one local upload at a time"
        );
        let s = wait(&mut host, |s| s["controls"]["revision"] == revision);
        assert_eq!(accept_audio(&mut host, &mut local), usize::from(revision == 1));
        apply(&mut local, &s, &mut ar);
        let s = wait(&mut guest, |s| s["controls"]["revision"] == revision);
        assert_eq!(accept_audio(&mut guest, &mut remote), usize::from(revision == 1));
        apply(&mut remote, &s, &mut br);
        assert!(s["sending"].is_null());
        assert_eq!(local.project(), remote.project());
        assert_eq!(render(&local), render(&remote));
        assert_eq!(local.audio().files.len(), 2);
    }
    assert_eq!(std::fs::read_dir(host_dir.path()).unwrap().count(), 2, "duplicate creates no new host folder");
    assert_eq!(std::fs::read_dir(guest_dir.path()).unwrap().count(), 2, "duplicate local preparation is discarded");
    publish_take(
        &mut host,
        Arc::new(AudioData { sample_rate: 8000, channels: 2, samples: vec![0.1; 8000] }),
        2,
        1,
        24.0,
    );
    let s = wait(&mut host, |s| s["controls"]["revision"] == 3);
    accept_audio(&mut host, &mut local);
    apply(&mut local, &s, &mut ar);
    let s = wait(&mut guest, |s| s["controls"]["revision"] == 3);
    accept_audio(&mut guest, &mut remote);
    apply(&mut remote, &s, &mut br);
    let folders = std::fs::read_dir(guest_dir.path()).unwrap().count();
    let candidate = Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.4; 8000] });
    publish_take(&mut guest, Arc::clone(&candidate), 0, 2, 32.0);
    let s = wait(&mut guest, |s| s["error"].is_string());
    assert_eq!(s["controls"]["revision"], 3);
    assert!(guest.take_audio().is_empty());
    assert_eq!(std::fs::read_dir(guest_dir.path()).unwrap().count(), folders, "stale local preparation is cleaned up");
    publish_take(&mut guest, Arc::clone(&candidate), 3, 2, 32.0);
    let s = wait(&mut host, |s| s["controls"]["revision"] == 4);
    accept_audio(&mut host, &mut local);
    apply(&mut local, &s, &mut ar);
    let s = wait(&mut guest, |s| s["controls"]["revision"] == 4);
    accept_audio(&mut guest, &mut remote);
    apply(&mut remote, &s, &mut br);
    assert_eq!(local.project(), remote.project());
    assert_eq!(render(&local), render(&remote));
    let mut out = [0.0; 512];
    a.process(&mut out);
    b.process(&mut out);
    assert!(a.monitor().running() && b.monitor().running());
    assert!((a.monitor().beat() - b.monitor().beat()).abs() > 5.0);
    host.execute("multiplayer.stop", &json!({})).unwrap();
    guest.execute("multiplayer.stop", &json!({})).unwrap();
    for (session, folder) in [(&mut local, host_folder), (&mut remote, guest_folder)] {
        let path = folder.join("With-guest-takes.suhina");
        session.save(&path, false).unwrap();
        let trusted =
            suhina_media::Trusted { files: session.loaded_audio_paths().into_iter().collect(), ..Default::default() };
        let opened = suhina_project_file::read_file(&path).unwrap();
        let (audio, warnings) = suhina_media::load_project_audio_trusted(&opened, &folder, &trusted);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            suhina_engine::render_offline_from(&opened, RenderInputs::from_audio(&audio), 8000, 512, None).unwrap(),
            render(session)
        );
    }
    assert_eq!(project, before);
    assert_eq!(take.samples, vec![0.25; 12000]);
}

#[test]
fn a_failed_guest_preparation_keeps_the_connection_and_next_publication_sequence_usable() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let (project, audio) = source();
    let mut host = Service::default();
    host.set_storage_root(Some(a.path().into())).unwrap();
    host.select_audio_project(&project, &audio).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let invite = wait(&mut host, |s| s["mode"] == "hosting")["invitation"].clone();
    let snapshot = host.take_shared().unwrap();
    let mut local = snapshot.session().unwrap();
    snapshot.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(b.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":invite})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let mut remote = snapshot.session().unwrap();
    snapshot.keep();
    publish_take(
        &mut guest,
        Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![f32::NAN; 100] }),
        0,
        2,
        8.0,
    );
    let state = wait(&mut guest, |s| s["error"].is_string());
    assert_eq!(state["mode"], "connected");
    assert_eq!(state["controls"]["revision"], 0);
    assert!(state["sending"].is_null());
    assert!(guest.take_audio().is_empty());
    assert_eq!(
        std::fs::read_dir(b.path()).unwrap().count(),
        1,
        "bad local data leaves only the original collected folder"
    );
    publish_take(
        &mut guest,
        Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.2; 8000] }),
        0,
        2,
        8.0,
    );
    let state = wait(&mut host, |s| s["controls"]["revision"] == 1);
    accept_audio(&mut host, &mut local);
    apply(&mut local, &state, &mut 0);
    let state = wait(&mut guest, |s| s["controls"]["revision"] == 1);
    accept_audio(&mut guest, &mut remote);
    apply(&mut remote, &state, &mut 0);
    assert_eq!(local.project(), remote.project());
    assert_eq!(render(&local), render(&remote));
}

#[test]
fn returning_guest_gets_current_tracks_and_takes_while_both_previous_copies_survive() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &pool).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let invitation = wait(&mut host, |s| s["mode"] == "hosting")["invitation"].clone();
    let initial = host.take_shared().unwrap();
    let mut local = initial.session().unwrap();
    initial.keep();
    let mut player = Renderer::stopped(local.project(), local.audio(), 8000).unwrap();
    local.attach_player(Player::new(player.controller(), None));
    local.execute("transport.setPosition", &json!({"beats":3})).unwrap();
    local.execute("transport.play", &json!({})).unwrap();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let mut previous = snapshot.session().unwrap();
    let previous_folder = snapshot.folder().unwrap().to_path_buf();
    snapshot.keep();
    wait(&mut host, |s| s["guest_connected"] == true);
    guest.execute("multiplayer.stop", &json!({})).unwrap();
    wait(&mut host, |s| s["guest_connected"] == false);
    previous.execute("track.rename", &json!({"track":1,"name":"Solo version kept"})).unwrap();
    let old_music = previous.project().clone();
    let old_render = render(&previous);
    let mut revision = 0;
    submit(&mut host, 0, WireEdit::TrackCreate { kind: "audio".into(), name: "New while absent".into() });
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 1), &mut revision);
    let track = local.project().tracks.iter().find(|t| t.name == "New while absent").unwrap().id.0;
    let take = Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.125; 8000] });
    publish_take(&mut host, Arc::clone(&take), 1, track, 4.0);
    let state = wait(&mut host, |s| s["controls"]["revision"] == 2);
    assert_eq!(accept_audio(&mut host, &mut local), 1);
    apply(&mut local, &state, &mut revision);
    host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    let ready = wait(&mut host, |s| s["invitation"] != invitation);
    assert!(host.take_shared().is_none(), "renewal must not replace the host snapshot/player");
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let mut remote = snapshot.session().unwrap();
    let mut remote_revision = snapshot.revision;
    assert_ne!(snapshot.folder().unwrap(), previous_folder);
    snapshot.keep();
    assert_eq!(remote_revision, 2);
    assert_eq!(local.project(), remote.project());
    assert_eq!(render(&local), render(&remote));
    // A new admission resets the guest author sequence, not the canonical musical revision.
    publish_take(&mut guest, Arc::new(AudioData { samples: vec![-0.25; 4000], ..(*take).clone() }), 2, track, 8.0);
    let state = wait(&mut host, |s| s["controls"]["revision"] == 3);
    assert_eq!(accept_audio(&mut host, &mut local), 1);
    apply(&mut local, &state, &mut revision);
    let state = wait(&mut guest, |s| s["controls"]["revision"] == 3);
    accept_audio(&mut guest, &mut remote);
    apply(&mut remote, &state, &mut remote_revision);
    assert_eq!(local.project(), remote.project());
    assert_eq!(render(&local), render(&remote));
    assert_eq!(previous.project(), &old_music);
    assert_eq!(render(&previous), old_render);
    assert!(previous.loaded_audio_paths().iter().all(|p| p.is_file()));
    let mut output = [0.0; 512];
    player.process(&mut output);
    assert!(player.monitor().running());
    assert!(player.monitor().beat() >= 3.0);
    assert_eq!(project, source().0);
}

#[test]
fn shared_chain_edits_cross_tls_preserve_collected_takes_notes_and_independent_players() {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (mut project, pool) = source();
    project.contents[2].lanes[0].data = LaneData::Notes(vec![suhina_model::Note::new(0., 2., 60, 0.8)]);
    let before = project.clone();
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.path().into())).unwrap();
    host.select_audio_project(&project, &pool).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let invitation = wait(&mut host, |s| s["mode"] == "hosting")["invitation"].clone();
    let snap = host.take_shared().unwrap();
    let mut local = snap.session().unwrap();
    snap.keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snap = guest.take_shared().unwrap();
    let mut remote = snap.session().unwrap();
    snap.keep();
    let mut a = Renderer::stopped(local.project(), local.audio(), 8000).unwrap();
    let mut b = Renderer::stopped(remote.project(), remote.audio(), 8000).unwrap();
    local.attach_player(Player::new(a.controller(), None));
    remote.attach_player(Player::new(b.controller(), None));
    for (s, beat) in [(&mut local, 1.), (&mut remote, 5.)] {
        s.execute("transport.setPosition", &json!({"beats":beat})).unwrap();
        s.execute("transport.play", &json!({})).unwrap();
    }
    let (mut ar, mut br) = (0, 0);
    let steps = [
        (true, WireEdit::DeviceAdd { track: 1, kind: "utility".into() }),
        (false, WireEdit::DeviceParam { track: 1, device: 0, param: "gain".into(), value: -12. }),
        (true, WireEdit::DeviceParam { track: 1, device: 0, param: "width".into(), value: 0.5 }),
        (false, WireEdit::DeviceRemove { track: 1, device: 0 }),
        (false, WireEdit::DeviceAdd { track: 3, kind: "fm-synth".into() }),
        (true, WireEdit::DeviceParam { track: 3, device: 0, param: "volume".into(), value: -9. }),
        (true, WireEdit::DeviceRemove { track: 3, device: 0 }),
    ];
    for (index, (from_guest, edit)) in steps.into_iter().enumerate() {
        let previous = local.project().clone();
        let structural = matches!(edit, WireEdit::DeviceAdd { .. } | WireEdit::DeviceRemove { .. });
        submit(if from_guest { &mut guest } else { &mut host }, ar, edit);
        let next = ar + 1;
        apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == next), &mut ar);
        apply(&mut remote, &wait(&mut guest, |s| s["controls"]["revision"] == next), &mut br);
        assert_eq!(local.project(), remote.project());
        assert_eq!(render(&local), render(&remote));
        if structural {
            assert_eq!(local.project().contents, previous.contents);
            assert_eq!(local.project().arrangement, previous.arrangement);
            assert_eq!(local.project().tracks, previous.tracks);
        }
        let mut output = [0.; 256];
        a.process(&mut output);
        b.process(&mut output);
        assert!(a.monitor().running() && b.monitor().running());
        assert!((b.monitor().beat() - a.monitor().beat() - 4.).abs() < 0.00001);
        if index == 2 {
            let take = Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.125; 8000] });
            publish_take(&mut guest, take, ar, 2, 4.);
            let next = ar + 1;
            let state = wait(&mut host, |s| s["controls"]["revision"] == next);
            accept_audio(&mut host, &mut local);
            apply(&mut local, &state, &mut ar);
            let state = wait(&mut guest, |s| s["controls"]["revision"] == next);
            accept_audio(&mut guest, &mut remote);
            apply(&mut remote, &state, &mut br);
            a.process(&mut output);
            b.process(&mut output);
        }
    }
    assert_eq!(ar, 8);
    assert_eq!(local.audio().files.len(), 2);
    assert_eq!(remote.audio().files.len(), 2);
    assert!(local.loaded_audio_paths().iter().all(|p| p.is_file()));
    assert!(remote.loaded_audio_paths().iter().all(|p| p.is_file()));
    let stable = local.project().clone();
    submit(&mut guest, 0, WireEdit::DeviceRemove { track: 1, device: 0 });
    let rejected = wait(&mut guest, |s| s["error"].is_string());
    assert_eq!(rejected["controls"]["revision"], 8);
    assert_eq!(local.project(), &stable);
    submit(&mut guest, 8, WireEdit::DeviceAdd { track: 1, kind: "utility".into() });
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 9), &mut ar);
    apply(&mut remote, &wait(&mut guest, |s| s["controls"]["revision"] == 9), &mut br);
    guest.execute("multiplayer.stop", &json!({})).unwrap();
    wait(&mut host, |s| s["guest_connected"] == false);
    let old = remote.project().clone();
    submit(&mut host, 9, WireEdit::DeviceAdd { track: 3, kind: "fm-synth".into() });
    apply(&mut local, &wait(&mut host, |s| s["controls"]["revision"] == 10), &mut ar);
    host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    let next = wait(&mut host, |s| s["invitation"] != invitation)["invitation"].clone();
    guest.execute("multiplayer.join", &json!({"invitation":next})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snap = guest.take_shared().unwrap();
    let fresh = snap.session().unwrap();
    snap.keep();
    assert_eq!(fresh.project(), local.project());
    assert_eq!(render(&fresh), render(&local));
    assert_eq!(remote.project(), &old);
    assert_eq!(project, before);
}
