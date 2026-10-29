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
    assert_eq!(refused(&mut guest)["controls"]["revision"], 2, "disconnect retains confirmed controls");
}

#[test]
fn an_online_code_works_once_and_three_guests_can_join_with_fresh_codes() {
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
    assert!(refused(&mut late)["error"].as_str().is_some(), "a spent code admits nobody");
    edit(&mut host, 0, json!({"command":"track.rename","track":1,"name":"Everyone"}));
    for guest in &mut guests {
        let seen = wait(guest, |v| v["controls"]["revision"] == 1);
        assert_eq!(seen["controls"]["tracks"][0]["name"], "Everyone");
    }
}

#[test]
fn malformed_online_codes_are_refused_before_connecting() {
    let mut guest = Service::default();
    assert!(guest.execute("multiplayer.join", &json!({"invitation":"suhina-net://not-base64!"})).is_err());
    assert!(guest.execute("multiplayer.join", &json!({"invitation":"suhina-net://e30"})).is_err());
    assert!(!guest.active());
}
