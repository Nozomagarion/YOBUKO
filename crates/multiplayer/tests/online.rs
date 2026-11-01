// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.

//! Online sessions through the peer-to-peer transport, kept on this computer: no relay, no lookup.

use serde_json::{Value, json};
use std::time::{Duration, Instant};
use suhina_multiplayer::{ONLINE_LOOPBACK, Service};

fn state(service: &mut Service) -> Value {
    service.execute("multiplayer.state", &json!({})).unwrap()
}
fn wait(service: &mut Service, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let value = state(service);
        assert!(value["mode"] != "error", "online service failed: {}", value["error"]);
        if ready(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "online state timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn refused(service: &mut Service) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    while service.active() && Instant::now() < deadline {
        service.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!service.active());
    state(service)
}
fn host() -> (Service, String) {
    let mut host = Service::default();
    host.execute("multiplayer.host", &json!({"address":ONLINE_LOOPBACK})).unwrap();
    let ready = wait(&mut host, |v| v["mode"] == "hosting");
    let invite = ready["invitation"].as_str().unwrap().to_owned();
    assert!(invite.starts_with("suhina-net://"));
    (host, invite)
}
fn edit(service: &mut Service, base: u64, edit: Value) {
    service.execute("multiplayer.edit", &json!({"base_revision":base,"edit":edit.to_string()})).unwrap();
}

#[test]
fn an_online_guest_joins_by_code_exchanges_edits_and_sees_the_host_leave() {
    let (mut host, invitation) = host();
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    edit(&mut host, 0, json!({"command":"track.rename","track":1,"name":"From afar"}));
    let remote = wait(&mut guest, |v| v["controls"]["revision"] == 1);
    assert_eq!(remote["controls"]["tracks"][0]["name"], "From afar");
    edit(&mut guest, 1, json!({"command":"track.setVolume","track":2,"db":-6.0}));
    let local = wait(&mut host, |v| v["controls"]["revision"] == 2);
    assert!((local["controls"]["tracks"][1]["db"].as_f64().unwrap() + 6.0).abs() < 0.001);
    edit(&mut guest, 0, json!({"command":"track.setPan","track":2,"pan":0.5}));
    let stale = wait(&mut guest, |v| v["error"].as_str().is_some());
    assert_eq!(stale["controls"]["revision"], 2);
    host.execute("multiplayer.stop", &json!({})).unwrap();
    let left = refused(&mut guest);
    assert_eq!(left["controls"]["revision"], 2, "disconnect retains confirmed controls");
    assert_eq!(left["mode"], "disconnected");
    assert!(left["error"].as_str().unwrap().starts_with("The host ended the session"), "{left}");
}

#[test]
fn a_replaced_online_code_admits_nobody_and_guests_can_join_with_fresh_codes() {
    let (mut host, first) = host();
    let mut guests = Vec::new();
    let mut code = first.clone();
    for count in 1..=3u64 {
        let mut guest = Service::default();
        guest.execute("multiplayer.join", &json!({"invitation":code})).unwrap();
        wait(&mut guest, |v| v["mode"] == "connected");
        wait(&mut host, |v| v["guests_connected"] == count);
        guests.push(guest);
        if count < 3 {
            host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
            let previous = code.clone();
            code = wait(&mut host, |v| v["invitation"].as_str().is_some_and(|i| i != previous))["invitation"]
                .as_str()
                .unwrap()
                .to_owned();
        }
    }
    let mut late = Service::default();
    late.execute("multiplayer.join", &json!({"invitation":first})).unwrap();
    let spent = refused(&mut late);
    let error = spent["error"].as_str().unwrap();
    assert!(error.starts_with("This invitation code is not valid for this session"), "{error}");
    edit(&mut host, 0, json!({"command":"track.rename","track":1,"name":"Everyone"}));
    for guest in &mut guests {
        let seen = wait(guest, |v| v["controls"]["revision"] == 1);
        assert_eq!(seen["controls"]["tracks"][0]["name"], "Everyone");
    }
}

/// When a scheduled start falls on this process's clock, read from one state call.
fn start_instant(value: &Value, read_at: Instant) -> Instant {
    let starts_in = value["transport_starts_in_ms"].as_i64().unwrap();
    let magnitude = Duration::from_millis(starts_in.unsigned_abs());
    if starts_in >= 0 { read_at + magnitude } else { read_at - magnitude }
}

#[test]
fn linked_playback_starts_host_and_guest_together_and_stops_both() {
    let (mut host, invitation) = host();
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(&mut guest, |v| v["mode"] == "connected");
    assert!(guest.execute("multiplayer.transport", &json!({"action":"play","beats":4})).is_err(), "unlinked");
    host.execute("multiplayer.linkPlayback", &json!({"linked":true})).unwrap();
    wait(&mut host, |v| v["linked"] == true);
    let linked = wait(&mut guest, |v| v["linked"] == true);
    assert_eq!(linked["transport"]["playing"], false);
    assert!(linked["transport_starts_in_ms"].is_null());
    // Let the guest collect a few round trips for its clock estimate.
    std::thread::sleep(Duration::from_millis(400));
    guest.execute("multiplayer.transport", &json!({"action":"play","beats":4})).unwrap();
    let remote = wait(&mut guest, |v| v["transport"]["playing"] == true);
    let remote_at = Instant::now();
    let local = wait(&mut host, |v| v["transport"]["playing"] == true);
    let local_at = Instant::now();
    assert_eq!(remote["transport"], local["transport"], "the same seq, beat and host start time");
    assert_eq!(local["transport"]["beat"], 4.0);
    let (guest_start, host_start) = (start_instant(&remote, remote_at), start_instant(&local, local_at));
    let gap = if guest_start > host_start { guest_start - host_start } else { host_start - guest_start };
    assert!(gap < Duration::from_millis(30), "starts differ by {gap:?}");
    let lead = host_start.saturating_duration_since(local_at);
    assert!(lead <= Duration::from_millis(suhina_multiplayer::LEAD_MS), "{lead:?}");
    // A late reader sees a negative delay and can catch up from the elapsed time.
    std::thread::sleep(Duration::from_millis(400));
    assert!(state(&mut guest)["transport_starts_in_ms"].as_i64().unwrap() < 0);
    let seq = local["transport"]["seq"].as_u64().unwrap();
    host.execute("multiplayer.transport", &json!({"action":"stop","beats":0})).unwrap();
    let stopped = wait(&mut guest, |v| v["transport"]["playing"] == false);
    assert_eq!(stopped["transport"]["seq"], seq + 1);
    assert_eq!(stopped["transport"]["beat"], 4.0, "stop keeps the start position");
    guest.execute("multiplayer.linkPlayback", &json!({"linked":false})).unwrap();
    wait(&mut host, |v| v["linked"] == false && v["transport"].is_null());
    wait(&mut guest, |v| v["linked"] == false);
    assert!(host.execute("multiplayer.transport", &json!({"action":"play","beats":0})).is_err());
}

#[test]
fn malformed_online_codes_are_refused_before_connecting() {
    let mut guest = Service::default();
    assert!(guest.execute("multiplayer.join", &json!({"invitation":"suhina-net://not-base64!"})).is_err());
    assert!(guest.execute("multiplayer.join", &json!({"invitation":"suhina-net://e30"})).is_err());
    for code in ["7-piano-tigre", "7-piano-tigre-xyzzy", "1000-piano-tigre-lune", "7-piano-tigre-lune-soleil"] {
        let error = guest.execute("multiplayer.join", &json!({"invitation":code})).unwrap_err();
        assert!(!error.to_string().is_empty(), "{code}");
    }
    assert!(!guest.active(), "nothing was started for a malformed code");
}

/// The host's short code once its lobby is reachable, with the long invitation beside it.
fn short_code(host: &mut Service, previous: Option<&str>) -> String {
    let ready = wait(host, |v| v["short_code"].as_str().is_some_and(|c| Some(c) != previous));
    assert!(ready["invitation"].as_str().is_some_and(|i| i.starts_with("suhina-net://")), "both stay available");
    ready["short_code"].as_str().unwrap().to_owned()
}
fn join(code: &str) -> Service {
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":code})).unwrap();
    guest
}

#[test]
fn a_guest_joins_with_the_short_code_alone_and_edits_together() {
    let (mut host, _) = host();
    let code = short_code(&mut host, None);
    assert!(code.starts_with("0-") && code.split('-').count() == 4, "a loopback code: {code}");
    // Read out loud and typed back: other case and spaces instead of hyphens.
    let mut guest = join(&format!("  {}  ", code.replace('-', " ").to_uppercase()));
    wait(&mut guest, |v| v["mode"] == "connected");
    wait(&mut host, |v| v["guests_connected"] == 1);
    edit(&mut guest, 0, json!({"command":"track.rename","track":1,"name":"Said out loud"}));
    let local = wait(&mut host, |v| v["controls"]["revision"] == 1);
    assert_eq!(local["controls"]["tracks"][0]["name"], "Said out loud");
}

#[test]
fn one_short_code_lets_the_group_in_and_a_wrong_one_admits_nobody() {
    let (mut host, _) = host();
    let code = short_code(&mut host, None);
    let mut wrong = join(&format!("0-{}", if code.ends_with("-zoo") { "piano-tigre-lune" } else { "zoo-zoo-zoo" }));
    let error = refused(&mut wrong)["error"].as_str().unwrap().to_owned();
    assert!(error.contains("no session"), "{error}");
    let mut first = join(&code);
    wait(&mut first, |v| v["mode"] == "connected");
    // The friends all type the same code.
    let mut second = join(&code);
    wait(&mut second, |v| v["mode"] == "connected");
    let mut third = join(&code);
    wait(&mut third, |v| v["mode"] == "connected");
    wait(&mut host, |v| v["guests_connected"] == 3);
    edit(&mut third, 0, json!({"command":"track.rename","track":1,"name":"Same code"}));
    for guest in [&mut first, &mut second] {
        let seen = wait(guest, |v| v["controls"]["revision"] == 1);
        assert_eq!(seen["controls"]["tracks"][0]["name"], "Same code");
    }
}

#[test]
fn a_new_invitation_replaces_the_short_code_and_revokes_the_old_one() {
    let (mut host, first_invitation) = host();
    let old = short_code(&mut host, None);
    host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    let new = short_code(&mut host, Some(&old));
    assert_ne!(state(&mut host)["invitation"].as_str().unwrap(), first_invitation);
    let mut late = join(&old);
    assert!(refused(&mut late)["error"].as_str().is_some(), "the replaced code admits nobody");
    let mut guest = join(&new);
    wait(&mut guest, |v| v["mode"] == "connected");
    host.execute("multiplayer.stop", &json!({})).unwrap();
    assert!(state(&mut host)["short_code"].is_null(), "leaving forgets the code");
}
