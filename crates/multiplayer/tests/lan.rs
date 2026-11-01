// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.

use serde_json::{Value, json};
use std::time::{Duration, Instant};
use suhina_multiplayer::Service;

fn state(service: &mut Service) -> Value {
    service.execute("multiplayer.state", &json!({})).unwrap()
}
fn wait(service: &mut Service, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let value = state(service);
        assert!(value["mode"] != "error", "LAN service failed: {}", value["error"]);
        if ready(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "LAN state timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn host() -> (Service, String) {
    let mut host = Service::default();
    host.execute("multiplayer.host", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |v| v["mode"] == "hosting");
    let invite = ready["invitation"].as_str().unwrap().to_owned();
    (host, invite)
}

#[test]
fn two_encrypted_services_exchange_edits_reject_stale_requests_and_disconnect() {
    let (mut host, invitation) = host();
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    host.execute("multiplayer.edit", &json!({"base_revision":0,"edit":json!({"command":"track.rename","track":1,"name":"Shared pulse"}).to_string()})).unwrap();
    let remote = wait(&mut guest, |v| v["controls"]["revision"] == 1);
    assert_eq!(remote["controls"]["tracks"][0]["name"], "Shared pulse");
    guest
        .execute(
            "multiplayer.edit",
            &json!({"base_revision":1,"edit":json!({"command":"track.setVolume","track":2,"db":-12.0}).to_string()}),
        )
        .unwrap();
    let local = wait(&mut host, |v| v["controls"]["revision"] == 2);
    assert!((local["controls"]["tracks"][1]["db"].as_f64().unwrap() + 12.0).abs() < 0.001);
    guest
        .execute(
            "multiplayer.edit",
            &json!({"base_revision":0,"edit":json!({"command":"track.setPan","track":2,"pan":0.5}).to_string()}),
        )
        .unwrap();
    let rejected = wait(&mut guest, |v| v["error"].as_str().is_some());
    assert_eq!(rejected["controls"]["revision"], 2);
    host.execute("multiplayer.stop", &json!({})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while guest.active() && Instant::now() < deadline {
        guest.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!guest.active());
    assert_eq!(state(&mut guest)["controls"]["revision"], 2, "disconnect retains confirmed controls");
}

#[test]
fn replaced_invitations_and_public_addresses_are_refused() {
    let mut service = Service::default();
    assert!(service.execute("multiplayer.host", &json!({"address":"8.8.8.8:1234"})).is_err());
    assert!(service.execute("multiplayer.host", &json!({"address":"127.0.0.1:0","extra":true})).is_err());
    assert!(service.execute("multiplayer.join", &json!({"invitation":"not an invitation"})).is_err());
    let (mut owner, invitation) = host();
    let mut a = Service::default();
    a.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut a, |v| v["mode"] == "connected");
    // The same code lets a second friend in; once the host replaces it, it admits nobody.
    let mut friend = Service::default();
    friend.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut friend, |v| v["mode"] == "connected");
    owner.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    wait(&mut owner, |v| v["invitation"].as_str() != Some(invitation.as_str()));
    let mut b = Service::default();
    b.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while b.active() && Instant::now() < deadline {
        b.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!b.active());
    assert!(state(&mut b)["error"].as_str().is_some());
}

#[test]
fn the_starter_is_deterministic_has_no_files_or_plugins_and_real_players_are_independent() {
    use suhina_engine::{AudioPool, Renderer};
    use suhina_session::{Player, Session};
    let project = suhina_multiplayer::starter_project();
    assert_eq!(project, suhina_multiplayer::starter_project());
    assert!(project.validate().is_ok());
    assert!(project.channels.iter().all(|c| c.devices.is_empty()));
    let mut a = Session::new(project.clone());
    let mut b = Session::new(project);
    let mut ra = Renderer::stopped(a.project(), &AudioPool::default(), 8000).unwrap();
    let mut rb = Renderer::stopped(b.project(), &AudioPool::default(), 8000).unwrap();
    a.attach_player(Player::new(ra.controller(), None));
    b.attach_player(Player::new(rb.controller(), None));
    a.execute("transport.setPosition", &json!({"beats":4})).unwrap();
    b.execute("transport.setPosition", &json!({"beats":20})).unwrap();
    a.execute("transport.play", &json!({})).unwrap();
    b.execute("transport.play", &json!({})).unwrap();
    let mut out = [0.0; 512];
    ra.process(&mut out);
    assert!(out.iter().any(|s| s.abs() > 0.00001));
    rb.process(&mut out);
    assert!((ra.monitor().beat() - rb.monitor().beat()).abs() > 10.0);
    a.execute("transport.stop", &json!({})).unwrap();
    ra.process(&mut out);
    rb.process(&mut out);
    assert!(!ra.monitor().running());
    assert!(rb.monitor().running());
}

#[test]
fn an_actual_midi_project_crosses_tls_in_chunks_and_keeps_its_ids_notes_and_sound() {
    use suhina_model::{Device, DeviceKind, DeviceParam, Extensions, LaneData, TrackId};
    let mut project = suhina_multiplayer::starter_project();
    project.transport.tempo_bpm = 138.0;
    project.tracks[0].id = TrackId(17);
    project.tracks[0].name = "My own lead".into();
    project.arrangement.lanes[0].track = Some(TrackId(17));
    project.launcher.columns[0].track = TrackId(17);
    let LaneData::Notes(notes) = &mut project.contents[0].lanes[0].data else { panic!("notes") };
    let first = notes[0];
    *notes =
        (0..256).map(|i| suhina_model::Note { time: f64::from(i) * 0.25, key: 40 + (i % 48) as u8, ..first }).collect();
    project.contents[0].length = 64.0;
    project.channels[0].devices.push(Device {
        name: "My FM sound".into(),
        kind: DeviceKind::Builtin("fm-synth".into()),
        enabled: true,
        state: None,
        params: vec![DeviceParam::new("algorithm", 5.0)],
        automation: vec![],
        clip_host: false,
        extensions: Extensions::default(),
    });
    assert!(project.validate().is_ok());
    let before = project.clone();
    let mut host = Service::default();
    host.select_project(&project).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let ready = wait(&mut host, |v| v["mode"] == "hosting");
    assert!(host.take_project().is_some());
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    let received = guest.take_project().unwrap();
    assert_eq!(received.tracks[0].id, TrackId(17));
    assert_eq!(received.transport, project.transport);
    assert_eq!(received.contents, project.contents);
    assert_eq!(received.arrangement, project.arrangement);
    assert_eq!(received.channels[0].devices, project.channels[0].devices);
    assert_eq!(project, before, "the source is never mutated");
    assert!(suhina_project_file::to_json(&received).len() > 32768, "must exercise multiple frames");
    // The receiver prepares a real independent renderer from the received musical document.
    let a = suhina_engine::render_offline(&project, 8000, 512).unwrap();
    let b = suhina_engine::render_offline(&received, 8000, 512).unwrap();
    assert!(b.iter().any(|sample| sample.abs() > 0.00001));
    assert_eq!(a, b);
    guest.execute("multiplayer.edit",&json!({"base_revision":0,"edit":json!({"command":"track.rename","track":17,"name":"Shared real lead"}).to_string()})).unwrap();
    let confirmed = wait(&mut host, |v| v["controls"]["revision"] == 1);
    assert_eq!(confirmed["controls"]["tracks"][0]["name"], "Shared real lead");
    assert_eq!(before.tracks[0].name, "My own lead");
}

fn submit(service: &mut Service, base: u64, edit: suhina_multiplayer::WireEdit) {
    service
        .execute(
            "multiplayer.edit",
            &json!({"base_revision":base,
        "edit":serde_json::to_string(&edit).unwrap()}),
        )
        .unwrap();
}
fn apply(session: &mut suhina_session::Session, revision: &mut u64, state: &Value) {
    let operations: Vec<suhina_multiplayer::Operation> = serde_json::from_value(state["operations"].clone()).unwrap();
    for op in operations {
        assert_eq!(op.revision, *revision + 1);
        let (id, params) = op.edit.command();
        session.execute(id, &params).unwrap();
        *revision = op.revision;
    }
    assert_eq!(state["controls"]["revision"], *revision);
}

#[test]
fn live_notes_cross_tls_in_both_directions_keep_players_and_reject_stale_indices() {
    use suhina_engine::{AudioPool, Renderer};
    use suhina_multiplayer::WireEdit;
    use suhina_session::{Player, Session};
    let (mut host, invitation) = host();
    let (project, mut host_revision) = host.take_snapshot().unwrap();
    let original = project.clone();
    let clip = project.arrangement.lanes[0].clips[0].id.0;
    let mut local = Session::new(project);
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    let (project, mut guest_revision) = guest.take_snapshot().unwrap();
    let mut remote = Session::new(project);
    let mut ra = Renderer::stopped(local.project(), &AudioPool::default(), 8000).unwrap();
    let mut rb = Renderer::stopped(remote.project(), &AudioPool::default(), 8000).unwrap();
    local.attach_player(Player::new(ra.controller(), None));
    remote.attach_player(Player::new(rb.controller(), None));
    local.execute("transport.setPosition", &json!({"beats":4})).unwrap();
    remote.execute("transport.setPosition", &json!({"beats":20})).unwrap();
    local.execute("transport.play", &json!({})).unwrap();
    remote.execute("transport.play", &json!({})).unwrap();
    submit(&mut guest, 0, WireEdit::NoteAdd { clip, time: 0.0, length: 0.75, key: 90, velocity: 110.0 });
    apply(&mut local, &mut host_revision, &wait(&mut host, |v| v["controls"]["revision"] == 1));
    apply(&mut remote, &mut guest_revision, &wait(&mut guest, |v| v["controls"]["revision"] == 1));
    submit(
        &mut host,
        1,
        WireEdit::NoteUpdate { clip, note: 4, time: 1.5, length: 1.0, key: 85, velocity: 65.0, muted: false },
    );
    apply(&mut local, &mut host_revision, &wait(&mut host, |v| v["controls"]["revision"] == 2));
    apply(&mut remote, &mut guest_revision, &wait(&mut guest, |v| v["controls"]["revision"] == 2));
    submit(&mut guest, 2, WireEdit::NoteDelete { clip, note: 0 });
    apply(&mut local, &mut host_revision, &wait(&mut host, |v| v["controls"]["revision"] == 3));
    apply(&mut remote, &mut guest_revision, &wait(&mut guest, |v| v["controls"]["revision"] == 3));
    let before = local.project().clone();
    submit(
        &mut host,
        2,
        WireEdit::NoteUpdate { clip, note: 0, time: 0.0, length: 1.0, key: 1, velocity: 1.0, muted: true },
    );
    let refused = wait(&mut host, |v| v["error"].is_string());
    assert_eq!(refused["controls"]["revision"], 3);
    assert_eq!(local.project(), &before);
    assert_eq!(local.project(), remote.project());
    let mut out = [0.0; 512];
    ra.process(&mut out);
    rb.process(&mut out);
    assert!(ra.monitor().running() && rb.monitor().running());
    assert!((ra.monitor().beat() - rb.monitor().beat()).abs() > 10.0);
    local.execute("transport.stop", &json!({})).unwrap();
    ra.process(&mut out);
    rb.process(&mut out);
    assert!(!ra.monitor().running() && rb.monitor().running());
    let a = suhina_engine::render_offline(local.project(), 8000, 512).unwrap();
    let b = suhina_engine::render_offline(remote.project(), 8000, 512).unwrap();
    assert_eq!(a, b);
    assert!(a.iter().any(|s| s.abs() > 0.00001));
    assert_ne!(
        a,
        suhina_engine::render_offline(&original, 8000, 512).unwrap(),
        "shared notes actually change the sound"
    );
}

#[test]
fn joining_after_more_than_one_journal_receives_the_current_music_snapshot() {
    use suhina_multiplayer::WireEdit;
    let (mut host, invitation) = host();
    let (project, _) = host.take_snapshot().unwrap();
    let clip = project.arrangement.lanes[0].clips[0].id.0;
    for base in 0..40 {
        submit(
            &mut host,
            base,
            WireEdit::NoteUpdate {
                clip,
                note: 0,
                time: 0.0,
                length: 0.5,
                key: 40 + base as u8,
                velocity: 90.0,
                muted: false,
            },
        );
        wait(&mut host, |v| v["controls"]["revision"] == base + 1);
    }
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    let state = wait(&mut guest, |v| v["mode"] == "connected");
    let (project, revision) = guest.take_snapshot().unwrap();
    assert_eq!(revision, 40);
    assert_eq!(state["controls"]["revision"], 40);
    let mut session = suhina_session::Session::new(project);
    let notes = session.execute("note.list", &json!({"clip":clip})).unwrap();
    assert_eq!(notes[0]["key"], 79);
}

struct ClipPair {
    host: Service,
    guest: Service,
    local: suhina_session::Session,
    remote: suhina_session::Session,
    revision: u64,
}
impl ClipPair {
    fn new(project: &suhina_model::Project) -> Self {
        let mut host = Service::default();
        host.select_project(project).unwrap();
        host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
        let ready = wait(&mut host, |s| s["mode"] == "hosting");
        let local = suhina_session::Session::new(host.take_project().unwrap());
        let mut guest = Service::default();
        guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
        wait(&mut guest, |s| s["mode"] == "connected");
        let remote = suhina_session::Session::new(guest.take_project().unwrap());
        Self { host, guest, local, remote, revision: 0 }
    }
    fn step(&mut self, from_guest: bool, edit: suhina_multiplayer::WireEdit) {
        submit(if from_guest { &mut self.guest } else { &mut self.host }, self.revision, edit);
        let next = self.revision + 1;
        let local = wait(&mut self.host, |v| v["controls"]["revision"] == next);
        let remote = wait(&mut self.guest, |v| v["controls"]["revision"] == next);
        let (mut a, mut b) = (self.revision, self.revision);
        apply(&mut self.local, &mut a, &local);
        apply(&mut self.remote, &mut b, &remote);
        assert_eq!(self.local.project(), self.remote.project());
        self.revision = next;
    }
}

#[test]
fn clip_creation_movement_trim_duplicate_notes_and_delete_converge_over_tls() {
    use suhina_engine::{AudioPool, Renderer};
    use suhina_multiplayer::WireEdit;
    use suhina_session::Player;
    let mut source = suhina_multiplayer::starter_project();
    for lane in &mut source.arrangement.lanes {
        for clip in &mut lane.clips {
            clip.length = 8.0;
        }
    }
    let before = source.clone();
    let clip = source.arrangement.lanes[0].clips[0].id.0;
    let neighbour = source.arrangement.lanes[1].clips[0].clone();
    let mut pair = ClipPair::new(&source);
    let mut ra = Renderer::stopped(pair.local.project(), &AudioPool::default(), 8000).unwrap();
    let mut rb = Renderer::stopped(pair.remote.project(), &AudioPool::default(), 8000).unwrap();
    pair.local.attach_player(Player::new(ra.controller(), None));
    pair.remote.attach_player(Player::new(rb.controller(), None));
    pair.local.execute("transport.setPosition", &json!({"beats":1})).unwrap();
    pair.remote.execute("transport.setPosition", &json!({"beats":5})).unwrap();
    pair.local.execute("transport.play", &json!({})).unwrap();
    pair.remote.execute("transport.play", &json!({})).unwrap();
    pair.step(true, WireEdit::ClipCreate { track: 1, start: 2.0, length: 4.0 });
    let new = pair.local.project().arrangement.lanes[0].clips.iter().find(|c| c.id.0 != clip).unwrap().id.0;
    pair.step(false, WireEdit::NoteAdd { clip: new, time: 0.0, length: 0.5, key: 90, velocity: 100.0 });
    pair.step(true, WireEdit::ClipMove { clip, start: 4.0, track: 2 });
    assert!(
        pair.local.project().arrangement.lanes[1].clips.contains(&neighbour),
        "moving never overwrites the neighbour"
    );
    pair.step(false, WireEdit::ClipTrim { clip, start: 4.0, end: 8.0 });
    pair.step(true, WireEdit::ClipDuplicate { clip });
    let copy = pair.local.project().arrangement.lanes[1]
        .clips
        .iter()
        .find(|c| c.id.0 != clip && c.id != neighbour.id)
        .unwrap()
        .id
        .0;
    pair.step(
        false,
        WireEdit::NoteUpdate { clip: copy, note: 0, time: 0.0, length: 0.5, key: 95, velocity: 100.0, muted: false },
    );
    let original = pair.local.execute("note.list", &json!({"clip":clip})).unwrap();
    assert_eq!(original[0]["key"], 48, "editing the duplicate protects its source");
    pair.step(true, WireEdit::ClipEnabled { clip: copy, enabled: false });
    let before_stale = pair.local.project().clone();
    submit(&mut pair.guest, 1, WireEdit::ClipDelete { clip });
    let refused = wait(&mut pair.guest, |s| s["error"].is_string());
    assert_eq!(refused["controls"]["revision"], 7);
    assert_eq!(pair.local.project(), &before_stale);
    pair.step(false, WireEdit::ClipDelete { clip });
    let mut out = [0.0; 512];
    ra.process(&mut out);
    rb.process(&mut out);
    assert!(ra.monitor().running() && rb.monitor().running());
    assert!((ra.monitor().beat() - rb.monitor().beat()).abs() > 3.0);
    pair.local.execute("transport.stop", &json!({})).unwrap();
    ra.process(&mut out);
    rb.process(&mut out);
    assert!(!ra.monitor().running() && rb.monitor().running());
    let a = suhina_engine::render_offline(pair.local.project(), 8000, 512).unwrap();
    let b = suhina_engine::render_offline(pair.remote.project(), 8000, 512).unwrap();
    assert_eq!(a, b);
    assert!(a.iter().any(|s| s.abs() > 0.00001));
    assert_ne!(a, suhina_engine::render_offline(&before, 8000, 512).unwrap());
    assert_eq!(source, before, "the original is untouched");
}

#[test]
fn late_admission_receives_the_edited_arrangement_and_its_new_ids() {
    use suhina_multiplayer::WireEdit;
    let (mut host, invitation) = host();
    let (initial, _) = host.take_snapshot().unwrap();
    let clip = initial.arrangement.lanes[0].clips[0].id.0;
    let mut expected = suhina_session::Session::new(initial);
    let mut expected_revision = 0;
    for (base, edit) in [
        WireEdit::ClipMove { clip, start: 4.0, track: 2 },
        WireEdit::ClipTrim { clip, start: 4.0, end: 8.0 },
        WireEdit::ClipDuplicate { clip },
        WireEdit::ClipCreate { track: 1, start: 0.0, length: 4.0 },
    ]
    .into_iter()
    .enumerate()
    {
        submit(&mut host, base as u64, edit);
        let state = wait(&mut host, |s| s["controls"]["revision"] == base + 1);
        apply(&mut expected, &mut expected_revision, &state);
    }
    let deleted = expected.project().arrangement.lanes[0].clips[0].id.0;
    submit(&mut host, 4, WireEdit::ClipDelete { clip: deleted });
    let state = wait(&mut host, |s| s["controls"]["revision"] == 5);
    apply(&mut expected, &mut expected_revision, &state);
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let (project, revision) = guest.take_snapshot().unwrap();
    assert_eq!(revision, 5);
    assert!(project.arrangement.lanes[0].clips.is_empty());
    assert_eq!(project.arrangement.lanes[1].clips.len(), 3);
    assert!(project.arrangement.lanes[1].clips.iter().any(|c| c.id.0 == clip && c.start == 4.0 && c.length == 4.0));
    assert!(project.validate().is_ok());
    assert_eq!(&project, expected.project());
    let mut received = suhina_session::Session::new(project);
    let mut received_revision = revision;
    submit(&mut guest, 5, WireEdit::ClipCreate { track: 1, start: 0.0, length: 4.0 });
    let local = wait(&mut host, |s| s["controls"]["revision"] == 6);
    let remote = wait(&mut guest, |s| s["controls"]["revision"] == 6);
    apply(&mut expected, &mut expected_revision, &local);
    apply(&mut received, &mut received_revision, &remote);
    assert_eq!(received.project(), expected.project());
    assert!(
        received.project().arrangement.lanes[0].clips[0].id.0 > deleted,
        "deleted IDs are not reused after admission"
    );
}

#[test]
fn an_empty_midi_project_can_be_composed_together_without_a_demo_clip() {
    use suhina_multiplayer::WireEdit;
    let mut source = suhina_multiplayer::starter_project();
    source.contents.clear();
    for lane in &mut source.arrangement.lanes {
        lane.clips.clear();
    }
    let mut pair = ClipPair::new(&source);
    assert!(pair.local.project().contents.is_empty());
    pair.step(true, WireEdit::ClipCreate { track: 1, start: 0.0, length: 4.0 });
    let clip = pair.local.project().arrangement.lanes[0].clips[0].id.0;
    pair.step(false, WireEdit::NoteAdd { clip, time: 0.0, length: 1.0, key: 60, velocity: 100.0 });
    let audio = suhina_engine::render_offline(pair.remote.project(), 8000, 512).unwrap();
    assert!(audio.iter().any(|s| s.abs() > 0.00001));
    assert!(source.contents.is_empty());
}

#[test]
fn both_authors_add_tracks_then_compose_on_the_new_midi_track_without_replacing_players() {
    use suhina_engine::{AudioPool, Renderer};
    use suhina_multiplayer::WireEdit;
    use suhina_session::Player;
    let mut source = suhina_multiplayer::starter_project();
    source.contents.clear();
    for lane in &mut source.arrangement.lanes {
        lane.clips.clear();
    }
    let before = source.clone();
    let mut pair = ClipPair::new(&source);
    let mut a = Renderer::stopped(pair.local.project(), &AudioPool::new(), 8000).unwrap();
    let mut b = Renderer::stopped(pair.remote.project(), &AudioPool::new(), 8000).unwrap();
    pair.local.attach_player(Player::new(a.controller(), None));
    pair.remote.attach_player(Player::new(b.controller(), None));
    pair.local.execute("transport.setPosition", &json!({"beats":1})).unwrap();
    pair.remote.execute("transport.setPosition", &json!({"beats":6})).unwrap();
    pair.local.execute("transport.play", &json!({})).unwrap();
    pair.remote.execute("transport.play", &json!({})).unwrap();
    pair.step(true, WireEdit::TrackCreate { kind: "audio".into(), name: "New voice".into() });
    pair.step(false, WireEdit::TrackCreate { kind: "midi".into(), name: "New keys".into() });
    let p = pair.local.project();
    let keys = p.tracks.iter().find(|t| t.name == "New keys").unwrap().id.0;
    assert_eq!(p.tracks.len(), before.tracks.len() + 2);
    assert_eq!(&p.arrangement.lanes[..before.arrangement.lanes.len()], &before.arrangement.lanes);
    for name in ["New voice", "New keys"] {
        let track = p.tracks.iter().find(|t| t.name == name).unwrap();
        assert_eq!(track.midi_input.port, suhina_model::MidiPort::Nothing);
        assert_eq!(track.audio_input.monitor, suhina_model::Monitor::Off);
    }
    submit(&mut pair.guest, 0, WireEdit::TrackCreate { kind: "midi".into(), name: "Stale request".into() });
    let rejected = wait(&mut pair.guest, |s| s["error"].is_string());
    assert_eq!(rejected["controls"]["revision"], 2);
    pair.step(true, WireEdit::ClipCreate { track: keys, start: 0.0, length: 4.0 });
    let clip = pair
        .local
        .project()
        .arrangement
        .lanes
        .iter()
        .find(|l| l.track == Some(suhina_model::TrackId(keys)))
        .unwrap()
        .clips[0]
        .id
        .0;
    pair.step(false, WireEdit::NoteAdd { clip, time: 0.0, length: 1.0, key: 60, velocity: 100.0 });
    let local = suhina_engine::render_offline(pair.local.project(), 8000, 512).unwrap();
    assert_eq!(local, suhina_engine::render_offline(pair.remote.project(), 8000, 512).unwrap());
    assert!(local.iter().any(|s| s.abs() > 0.00001), "new MIDI track has an audible default instrument");
    let mut out = [0.0; 512];
    a.process(&mut out);
    b.process(&mut out);
    assert!(a.monitor().running() && b.monitor().running());
    assert!((a.monitor().beat() - b.monitor().beat()).abs() > 4.0);
    assert_eq!(source, before);
}

#[test]
fn a_late_guest_receives_created_tracks_with_the_same_ids_and_default_devices() {
    use suhina_multiplayer::WireEdit;
    let (mut host, invitation) = host();
    let mut expected = suhina_session::Session::new(host.take_project().unwrap());
    let mut revision = 0;
    for kind in ["audio", "midi"] {
        submit(&mut host, revision, WireEdit::TrackCreate { kind: kind.into(), name: format!("Late {kind}") });
        let state = wait(&mut host, |s| s["controls"]["revision"] == revision + 1);
        apply(&mut expected, &mut revision, &state);
    }
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let (project, at) = guest.take_snapshot().unwrap();
    assert_eq!(at, revision);
    assert_eq!(&project, expected.project());
}

#[test]
fn renewed_invitations_reuse_the_host_and_rejoining_editors_start_fresh_sequences() {
    let (mut host, first) = host();
    assert!(Service::default().execute("multiplayer.renewInvitation", &json!({})).is_err());
    assert!(host.execute("multiplayer.renewInvitation", &json!({"extra":true})).is_err());
    let mut previous = first;
    let mut revision = 0;
    for cycle in 0..4 {
        host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
        let ready = wait(&mut host, |v| v["invitation"].as_str().is_some_and(|s| s != previous));
        assert_eq!(ready["controls"]["revision"], revision);
        let next = ready["invitation"].as_str().unwrap().to_owned();
        // An old hello must not redeem the replacement capability.
        let mut obsolete = Service::default();
        obsolete.execute("multiplayer.join", &json!({"invitation":previous})).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while obsolete.active() && Instant::now() < deadline {
            obsolete.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!obsolete.active());
        assert!(state(&mut obsolete)["error"].is_string());
        let mut guest = Service::default();
        guest.execute("multiplayer.join", &json!({"invitation":next})).unwrap();
        let joined = wait(&mut guest, |v| v["mode"] == "connected");
        assert_eq!(joined["controls"]["revision"], revision);
        wait(&mut host, |v| v["guest_connected"] == true);
        assert!(guest.execute("multiplayer.renewInvitation", &json!({})).is_err());
        assert_eq!(state(&mut host)["guests_connected"], 1);
        submit(&mut guest, revision, suhina_multiplayer::WireEdit::Rename { track: 1, name: format!("Guest {cycle}") });
        revision += 1;
        wait(&mut host, |v| v["controls"]["revision"] == revision);
        wait(&mut guest, |v| v["controls"]["revision"] == revision);
        guest.execute("multiplayer.stop", &json!({})).unwrap();
        wait(&mut host, |v| v["guest_connected"] == false);
        submit(
            &mut host,
            revision,
            suhina_multiplayer::WireEdit::Rename { track: 2, name: format!("Host continued {cycle}") },
        );
        revision += 1;
        wait(&mut host, |v| v["controls"]["revision"] == revision);
        previous = next;
    }
    assert_eq!(revision, 8);
}

#[test]
fn device_parameters_cross_tls_both_ways_keep_players_and_rejoin_with_current_sound() {
    use suhina_engine::{AudioPool, Renderer};
    use suhina_multiplayer::WireEdit;
    use suhina_session::{Player, Session};
    let mut source = Session::new(suhina_multiplayer::starter_project());
    for kind in ["fm-synth", "utility"] {
        source.execute("device.add", &json!({"track":1,"id":kind})).unwrap();
    }
    let mut project = source.project().clone();
    let mut point = suhina_model::AutomationPoint::new(0., -24.);
    point.curve = suhina_model::automation::Curve::new(0.2, 0.9, 0.8, 0.1).unwrap();
    project.channels[0].devices[0].automation.push(suhina_model::AutomationLane {
        param: "volume".into(),
        enabled: true,
        points: vec![point, suhina_model::AutomationPoint::new(4., -6.)],
    });
    let source = Session::new(project);
    let original = source.project().clone();
    let mut host = Service::default();
    host.select_project(source.project()).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let invitation = wait(&mut host, |v| v["mode"] == "hosting")["invitation"].clone();
    let mut local = Session::new(host.take_project().unwrap());
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    let mut remote = Session::new(guest.take_project().unwrap());
    let initial = suhina_engine::render_offline(local.project(), 8000, 256).unwrap();
    let mut a = Renderer::stopped(local.project(), &AudioPool::default(), 8000).unwrap();
    let mut b = Renderer::stopped(remote.project(), &AudioPool::default(), 8000).unwrap();
    local.attach_player(Player::new(a.controller(), None));
    remote.attach_player(Player::new(b.controller(), None));
    for (s, beat) in [(&mut local, 1.), (&mut remote, 9.)] {
        s.execute("transport.setPosition", &json!({"beats":beat})).unwrap();
        s.execute("transport.play", &json!({})).unwrap();
    }
    let (mut ar, mut br) = (0, 0);
    for (base, from_guest, device, param, value) in [
        (0, false, 0, "volume", -12.),
        (1, true, 1, "gain", -6.),
        (2, true, 0, "transpose", 12.),
        (3, false, 1, "mono", 1.),
    ] {
        submit(
            if from_guest { &mut guest } else { &mut host },
            base,
            WireEdit::DeviceParam { track: 1, device, param: param.into(), value },
        );
        apply(&mut local, &mut ar, &wait(&mut host, |v| v["controls"]["revision"] == base + 1));
        apply(&mut remote, &mut br, &wait(&mut guest, |v| v["controls"]["revision"] == base + 1));
        assert_eq!(local.project(), remote.project());
        assert!(!local.project().channels[0].devices[0].automation[0].enabled);
        assert_eq!(
            local.project().channels[0].devices[0].automation[0].points,
            original.channels[0].devices[0].automation[0].points
        );

        let left = suhina_engine::render_offline(local.project(), 8000, 256).unwrap();
        let right = suhina_engine::render_offline(remote.project(), 8000, 256).unwrap();
        assert_eq!(left, right);
        assert_ne!(left, initial);
        let mut out = [0.; 256];
        a.process(&mut out);
        b.process(&mut out);
        assert!(a.monitor().running() && b.monitor().running());
        assert!((b.monitor().beat() - a.monitor().beat() - 8.).abs() < 0.00001);
    }
    let before = local.project().clone();
    submit(&mut guest, 4, WireEdit::DeviceParam { track: 1, device: 0, param: "algorithm".into(), value: 8.5 });
    let rejected = wait(&mut guest, |v| v["error"].is_string());
    assert_eq!(rejected["controls"]["revision"], 4);
    assert_eq!(local.project(), &before);
    submit(&mut guest, 4, WireEdit::DeviceParam { track: 1, device: 0, param: "algorithm".into(), value: 8. });
    apply(&mut local, &mut ar, &wait(&mut host, |v| v["controls"]["revision"] == 5));
    apply(&mut remote, &mut br, &wait(&mut guest, |v| v["controls"]["revision"] == 5));
    guest.execute("multiplayer.stop", &json!({})).unwrap();
    wait(&mut host, |v| v["guest_connected"] == false);
    let old_copy = remote.project().clone();
    submit(&mut host, 5, WireEdit::DeviceParam { track: 1, device: 0, param: "volume".into(), value: -24. });
    apply(&mut local, &mut ar, &wait(&mut host, |v| v["controls"]["revision"] == 6));
    host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    let next = wait(&mut host, |v| v["invitation"] != invitation)["invitation"].clone();
    guest.execute("multiplayer.join", &json!({"invitation":next})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    let (fresh, revision) = guest.take_snapshot().unwrap();
    assert_eq!(revision, 6);
    assert_eq!(&fresh, local.project());
    assert_eq!(remote.project(), &old_copy);
    assert_eq!(source.project(), &original);
    let parsed = suhina_project_file::from_json(&suhina_project_file::to_json(&fresh)).unwrap();
    assert_eq!(parsed, fresh);
}

#[test]
fn launcher_scenes_cross_tls_keep_originals_private_launches_and_rejoin_state() {
    use suhina_engine::{AudioPool, Renderer};
    use suhina_multiplayer::WireEdit;
    use suhina_session::{Player, Session};
    let (mut host, invitation) = host();
    let (project, mut ar) = host.take_snapshot().unwrap();
    let original = project.clone();
    let source = project.arrangement.lanes[0].clips[0].id.0;
    let mut local = Session::new(project);
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    let (project, mut br) = guest.take_snapshot().unwrap();
    let mut remote = Session::new(project);
    let mut a = Renderer::stopped(local.project(), &AudioPool::default(), 8000).unwrap();
    let mut b = Renderer::stopped(remote.project(), &AudioPool::default(), 8000).unwrap();
    local.attach_player(Player::new(a.controller(), None));
    remote.attach_player(Player::new(b.controller(), None));
    local.execute("transport.setPosition", &json!({"beats":4})).unwrap();
    remote.execute("transport.setPosition", &json!({"beats":20})).unwrap();
    local.execute("transport.play", &json!({})).unwrap();
    remote.execute("transport.play", &json!({})).unwrap();
    macro_rules! exchange {
        ($guest:expr, $edit:expr) => {{
            submit(if $guest { &mut guest } else { &mut host }, ar, $edit);
            let next = ar + 1;
            apply(&mut local, &mut ar, &wait(&mut host, |v| v["controls"]["revision"] == next));
            apply(&mut remote, &mut br, &wait(&mut guest, |v| v["controls"]["revision"] == next));
            assert_eq!(local.project(), remote.project());
        }};
    }
    exchange!(true, WireEdit::SceneRename { scene: 1, name: "Intro".into() });
    exchange!(false, WireEdit::SlotCopy { source, track: 1, scene: 1 });
    let copied = local.project().launcher.columns[0].slots[0].clip.as_ref().unwrap().id.0;
    exchange!(true, WireEdit::NoteAdd { clip: copied, time: 2., length: 0.5, key: 90, velocity: 100. });
    assert_eq!(local.project().arrangement, original.arrangement);
    assert_eq!(
        local.project().content(original.arrangement.lanes[0].clips[0].content),
        original.content(original.arrangement.lanes[0].clips[0].content)
    );
    exchange!(false, WireEdit::SceneDuplicate { scene: 1 });
    let duplicated = local.project().launcher.columns[0].slots[1].clip.as_ref().unwrap().clone();
    exchange!(true, WireEdit::NoteDelete { clip: copied, note: 0 });
    assert_ne!(
        local.project().content(duplicated.content).unwrap().lanes,
        local
            .project()
            .content(local.project().launcher.columns[0].slots[0].clip.as_ref().unwrap().content)
            .unwrap()
            .lanes
    );
    exchange!(true, WireEdit::SlotCreate { track: 2, scene: 1, length: 8. });
    let other = local.project().launcher.columns[1].slots[0].clip.as_ref().unwrap().id;
    exchange!(false, WireEdit::SlotMove { track: 1, scene: 1, to_track: 2, to_scene: 1 });
    assert_eq!(local.project().launcher.columns[0].slots[0].clip.as_ref().unwrap().id, other);
    let before = local.project().clone();
    submit(&mut guest, ar - 1, WireEdit::SceneDelete { scene: 1 });
    let denied = wait(&mut guest, |v| v["error"].is_string());
    assert_eq!(denied["controls"]["revision"], ar);
    assert_eq!(local.project(), &before);
    exchange!(true, WireEdit::SceneInsert { scene: 1, name: "Opening".into() });
    exchange!(false, WireEdit::ClipDelete { clip: other.0 });
    exchange!(true, WireEdit::SceneDelete { scene: 3 });
    let mut out = [0.; 512];
    a.process(&mut out);
    b.process(&mut out);
    assert!((a.monitor().beat() - b.monitor().beat()).abs() > 10.);
    assert!(a.monitor().running() && b.monitor().running());
    local.execute("launcher.setQuantization", &json!({"value":"none"})).unwrap();
    local.execute("scene.launch", &json!({"scene":2})).unwrap();
    a.process(&mut out);
    b.process(&mut out);
    assert_eq!(a.monitor().column(1).unwrap().playing, Some(1));
    assert_eq!(b.monitor().column(1).unwrap().playing, None);
    assert_eq!(ar, br);
    assert_eq!(local.project(), remote.project());
    guest.execute("multiplayer.stop", &json!({})).unwrap();
    wait(&mut host, |v| v["guest_connected"] == false);
    let previous_invitation = state(&mut host)["invitation"].clone();
    host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    let ready = wait(&mut host, |v| v["invitation"].is_string() && v["invitation"] != previous_invitation);
    let mut late = Service::default();
    late.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut late, |v| v["mode"] == "connected");
    let (joined, rev) = late.take_snapshot().unwrap();
    assert_eq!(rev, ar);
    assert_eq!(&joined, local.project());
}
