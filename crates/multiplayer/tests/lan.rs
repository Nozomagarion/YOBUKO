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
fn consumed_invitations_and_public_addresses_are_refused() {
    let mut service = Service::default();
    assert!(service.execute("multiplayer.host", &json!({"address":"8.8.8.8:1234"})).is_err());
    assert!(service.execute("multiplayer.host", &json!({"address":"127.0.0.1:0","extra":true})).is_err());
    assert!(service.execute("multiplayer.join", &json!({"invitation":"not an invitation"})).is_err());
    let (_host, invitation) = host();
    let mut a = Service::default();
    a.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut a, |v| v["mode"] == "connected");
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
