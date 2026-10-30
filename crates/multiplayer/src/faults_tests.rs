// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whole sessions over degraded links, online and on the LAN: latency, a slow link, cuts, resumes.

use super::*;
use crate::resume::Tuning;
use crate::transport::{ONLINE_LOOPBACK, Service};
use serde_json::{Value, json};
use std::sync::Arc;

const PLACES: [&str; 2] = [ONLINE_LOOPBACK, "127.0.0.1:0"];

fn tuned(faults: Option<Arc<Faults>>, grace: Duration, retry: Duration) -> Service {
    let mut service = Service::default();
    service.set_tuning(Tuning { grace, retry, faults });
    service
}
fn plain() -> Service {
    tuned(None, Duration::from_secs(120), Duration::from_secs(60))
}
fn state(service: &mut Service) -> Value {
    service.execute("multiplayer.state", &json!({})).unwrap()
}
fn wait(service: &mut Service, what: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let value = state(service);
        assert!(value["mode"] != "error", "{what}: service failed: {}", value["error"]);
        if ready(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "{what} timed out: {value}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn edit(service: &mut Service, base: u64, edit: Value) {
    service.execute("multiplayer.edit", &json!({"base_revision":base,"edit":edit.to_string()})).unwrap();
}
fn host(service: &mut Service, place: &str) -> String {
    service.execute("multiplayer.host", &json!({"address":place})).unwrap();
    wait(service, "hosting", |v| v["mode"] == "hosting")["invitation"].as_str().unwrap().to_owned()
}
fn join(service: &mut Service, invitation: &str) {
    service.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    wait(service, "joining", |v| v["mode"] == "connected");
}
/// The same shared controls; volumes may differ by rounding through the portable copy.
fn same(a: &Value, b: &Value, place: &str) {
    assert_eq!(a["revision"], b["revision"], "{place}");
    let (a, b) = (a["tracks"].as_array().unwrap(), b["tracks"].as_array().unwrap());
    assert_eq!(a.len(), b.len(), "{place}");
    for (a, b) in a.iter().zip(b) {
        assert_eq!((&a["track"], &a["name"], &a["pan"]), (&b["track"], &b["name"], &b["pan"]), "{place}");
        assert!((a["db"].as_f64().unwrap() - b["db"].as_f64().unwrap()).abs() < 1e-6, "{place}");
    }
}
/// A project with an audio-only first track, so a guest can publish a take to it.
fn audio_project() -> suhina_model::Project {
    let mut project = crate::starter_project();
    project.tracks[0].content = suhina_model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = suhina_model::LaneData::Audio(suhina_model::AudioRegion::new(""));
    project
}

#[test]
fn a_whole_session_survives_latency_and_a_slow_link_online_and_on_the_lan() {
    for place in PLACES {
        let (host_dir, guest_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        // 150 ms each way on the guest's side, and about 64 KiB/s through both ends.
        let slow = Arc::new(Faults::new(Duration::from_millis(150), Some(64 * 1024)));
        let throttled = Arc::new(Faults::new(Duration::ZERO, Some(64 * 1024)));
        let mut owner = tuned(Some(throttled), Duration::from_secs(120), Duration::from_secs(60));
        owner.set_storage_root(Some(host_dir.path().into())).unwrap();
        owner.select_audio_project(&audio_project(), &suhina_engine::AudioPool::new()).unwrap();
        owner.execute("multiplayer.hostProject", &json!({"address":place})).unwrap();
        let invitation =
            wait(&mut owner, "hosting", |v| v["mode"] == "hosting")["invitation"].as_str().unwrap().to_owned();
        owner.take_shared().unwrap().keep();
        let mut guest = tuned(Some(Arc::clone(&slow)), Duration::from_secs(120), Duration::from_secs(60));
        guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
        join(&mut guest, &invitation);
        guest.take_shared().unwrap().keep();
        edit(&mut owner, 0, json!({"command":"track.rename","track":2,"name":"Far away"}));
        let seen = wait(&mut guest, "host edit", |v| v["controls"]["revision"] == 1);
        assert_eq!(seen["controls"]["tracks"][1]["name"], "Far away");
        edit(&mut guest, 1, json!({"command":"track.setVolume","track":2,"db":-9.0}));
        wait(&mut owner, "guest edit", |v| v["controls"]["revision"] == 2);
        // A take crosses the slow link in both directions: guest to host, then host to guest.
        let take = Arc::new(suhina_engine::AudioData { sample_rate: 8000, channels: 1, samples: vec![0.2; 8000] });
        guest.select_take(take).unwrap();
        guest.execute("multiplayer.publishTake", &json!({"base_revision":2,"track":1,"start":4.0})).unwrap();
        wait(&mut owner, "published take", |v| v["controls"]["revision"] == 3);
        wait(&mut guest, "take confirmed", |v| v["controls"]["revision"] == 3 && v["sending"].is_null());
        assert_eq!(owner.take_audio().len(), 1, "{place}: the host received the sound");
        // Linked playback still starts everyone on the same shared transport.
        owner.execute("multiplayer.linkPlayback", &json!({"linked":true})).unwrap();
        wait(&mut guest, "linked", |v| v["linked"] == true);
        guest.execute("multiplayer.transport", &json!({"action":"play","beats":8})).unwrap();
        let remote = wait(&mut guest, "guest playing", |v| v["transport"]["playing"] == true);
        let local = wait(&mut owner, "host playing", |v| v["transport"]["playing"] == true);
        assert_eq!(remote["transport"], local["transport"], "{place}");
        same(&remote["controls"], &local["controls"], place);
        assert!(remote["transport_starts_in_ms"].is_i64());
        assert_eq!(slow.cuts(), 0);
    }
}

#[test]
fn a_cut_that_lasts_ends_with_a_clear_error_and_the_place_frees_after_its_grace() {
    for place in PLACES {
        let faults = Arc::new(Faults::default());
        let mut owner = tuned(None, Duration::from_secs(2), Duration::from_secs(60));
        let invitation = host(&mut owner, place);
        let mut guest = tuned(Some(Arc::clone(&faults)), Duration::from_secs(120), Duration::from_secs(2));
        join(&mut guest, &invitation);
        edit(&mut guest, 0, json!({"command":"track.rename","track":1,"name":"Before the cut"}));
        wait(&mut owner, "guest edit", |v| v["controls"]["revision"] == 1);
        let before = state(&mut owner)["controls"].clone();
        faults.cut();
        // The guest retries for its window, then says what happened and keeps its copy.
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut reconnecting = false;
        while guest.active() {
            reconnecting |= state(&mut guest)["mode"] == "reconnecting";
            assert!(Instant::now() < deadline, "{place}: the guest kept retrying");
            std::thread::sleep(Duration::from_millis(10));
        }
        let lost = state(&mut guest);
        assert!(reconnecting, "{place}: the guest showed it was reconnecting");
        assert_eq!(lost["mode"], "error");
        assert!(lost["error"].as_str().unwrap().starts_with("Lost the connection to the host"), "{lost}");
        assert_eq!(lost["controls"]["revision"], 1, "{place}: the last copy is kept");
        // The host counts the guest out at once and keeps its place for the grace period only.
        wait(&mut owner, "guest gone", |v| v["guests_connected"] == 0);
        wait(&mut owner, "place freed", |v| v["guests_reconnecting"] == 0);
        let after = state(&mut owner);
        assert_eq!(after["controls"], before, "{place}: the host state is intact");
        assert_eq!(after["mode"], "hosting");
        // The host goes on and a new guest joins the same state with a fresh code.
        owner.execute("multiplayer.renewInvitation", &json!({})).unwrap();
        let code = wait(&mut owner, "renewed", |v| v["invitation"].as_str().is_some_and(|i| i != invitation));
        edit(&mut owner, 1, json!({"command":"track.setPan","track":1,"pan":0.5}));
        wait(&mut owner, "host edit", |v| v["controls"]["revision"] == 2);
        let mut late = plain();
        join(&mut late, code["invitation"].as_str().unwrap());
        let joined = wait(&mut late, "late copy", |v| v["controls"]["revision"] == 2);
        assert_eq!(joined["controls"]["tracks"][0]["name"], "Before the cut");
        assert!(faults.cuts() >= 1);
    }
}

#[test]
fn a_dropped_guest_resumes_without_a_new_code_and_its_unanswered_edit_applies_once() {
    for place in PLACES {
        // The host answers late, so the link can be cut after an edit applied and before its answer.
        let faults = Arc::new(Faults::new(Duration::from_millis(300), None));
        let mut owner = tuned(Some(Arc::clone(&faults)), Duration::from_secs(120), Duration::from_secs(60));
        let invitation = host(&mut owner, place);
        let mut guest = tuned(None, Duration::from_secs(120), Duration::from_secs(30));
        join(&mut guest, &invitation);
        wait(&mut owner, "guest counted", |v| v["guests_connected"] == 1);
        edit(&mut guest, 0, json!({"command":"track.rename","track":1,"name":"Exactly once"}));
        wait(&mut owner, "edit applied", |v| v["controls"]["revision"] == 1);
        faults.cut();
        let deadline = Instant::now() + Duration::from_secs(10);
        while faults.cuts() == 0 {
            assert!(Instant::now() < deadline, "{place}: the link was never cut");
            std::thread::sleep(Duration::from_millis(5));
        }
        faults.heal();
        let mut reconnecting = false;
        let deadline = Instant::now() + Duration::from_secs(30);
        let resumed = loop {
            let value = state(&mut guest);
            assert!(value["mode"] != "error", "{place}: {}", value["error"]);
            reconnecting |= value["mode"] == "reconnecting";
            if reconnecting && value["mode"] == "connected" && value["controls"]["revision"] == 1 {
                break value;
            }
            assert!(Instant::now() < deadline, "{place}: no resume: {value}");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(resumed["controls"]["tracks"][0]["name"], "Exactly once");
        assert!(resumed["error"].is_null(), "{place}: the resent edit was not refused: {resumed}");
        // The resent edit hit the host's retry cache: one revision, and the next sequence works.
        edit(&mut guest, 1, json!({"command":"track.setPan","track":1,"pan":-0.5}));
        let host_view = wait(&mut owner, "next edit", |v| v["controls"]["revision"] == 2);
        let guest_view = wait(&mut guest, "next edit seen", |v| v["controls"]["revision"] == 2);
        same(&host_view["controls"], &guest_view["controls"], place);
        assert_eq!(host_view["guests_connected"], 1);
        assert_eq!(host_view["guests_reconnecting"], 0);
        assert!(guest.take_shared().is_some(), "{place}: the first copy only");
        // Leaving on purpose frees the place at once instead of holding it.
        guest.execute("multiplayer.stop", &json!({})).unwrap();
        wait(&mut owner, "left", |v| v["guests_connected"] == 0);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(state(&mut owner)["guests_reconnecting"], 0, "{place}");
    }
}

#[test]
fn a_host_that_ends_the_session_is_told_apart_from_a_lost_network() {
    for place in PLACES {
        let mut owner = plain();
        let invitation = host(&mut owner, place);
        let mut guest = plain();
        join(&mut guest, &invitation);
        owner.execute("multiplayer.stop", &json!({})).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while guest.active() {
            assert_ne!(state(&mut guest)["mode"], "reconnecting", "{place}: an ended session is not retried");
            assert!(Instant::now() < deadline, "{place}: the guest never noticed");
            std::thread::sleep(Duration::from_millis(10));
        }
        let ended = state(&mut guest);
        assert_eq!(ended["mode"], "disconnected", "{place}: {ended}");
        assert!(ended["error"].as_str().unwrap().starts_with("The host ended the session"), "{ended}");
    }
}

#[test]
fn the_fault_link_delays_throttles_and_cuts_after_a_byte_count() {
    let host = crate::online::Net::bind(crate::online::Reach::Loopback).unwrap();
    let incoming = host.listen(2);
    let guest = crate::online::Net::bind(crate::online::Reach::Loopback).unwrap();
    let faults = Arc::new(Faults::new(Duration::from_millis(100), Some(10_000)));
    let mut out = Faulty::new(guest.connect(host.address()).unwrap(), Arc::clone(&faults));
    let start = Instant::now();
    out.write_all(&[7; 1000]).unwrap();
    // One turn of latency plus 1000 bytes at 10 kB/s.
    assert!(start.elapsed() >= Duration::from_millis(190), "{:?}", start.elapsed());
    let mut served = incoming.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut bytes = [0; 1000];
    served.read_exact(&mut bytes).unwrap();
    faults.cut_after(Some(1500));
    out.write_all(&[1; 400]).unwrap();
    assert!(out.write_all(&[1; 400]).is_err(), "the link fails at the byte limit");
    assert!(out.write_all(&[1]).is_err(), "and stays cut");
    assert_eq!(faults.cuts(), 1);
    // A cut on demand interrupts a read that is waiting for data.
    let faults = Arc::new(Faults::default());
    let mut waiting = Faulty::new(guest.connect(host.address()).unwrap(), Arc::clone(&faults));
    waiting.set_deadline(Instant::now() + Duration::from_secs(10));
    let cutter = Arc::clone(&faults);
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        cutter.cut();
    });
    let start = Instant::now();
    assert!(waiting.read(&mut bytes).is_err());
    assert!(start.elapsed() < Duration::from_secs(5));
    thread.join().unwrap();
}

#[test]
fn a_guest_away_longer_than_the_history_resumes_with_a_fresh_copy() {
    for place in PLACES {
        let faults = Arc::new(Faults::default());
        let mut owner = plain();
        let invitation = host(&mut owner, place);
        let mut guest = tuned(Some(Arc::clone(&faults)), Duration::from_secs(120), Duration::from_secs(30));
        join(&mut guest, &invitation);
        guest.take_shared().unwrap().keep();
        faults.cut();
        wait(&mut guest, "reconnecting", |v| v["mode"] == "reconnecting");
        // More edits than the host's history keeps: the guest cannot catch up operation by operation.
        for revision in 0..40u64 {
            let pan = (revision % 10) as f64 / 10.0;
            edit(&mut owner, revision, json!({"command":"track.setPan","track":2,"pan":pan}));
            wait(&mut owner, "host edit", |v| v["controls"]["revision"] == revision + 1);
        }
        faults.heal();
        let back = wait(&mut guest, "resumed", |v| v["mode"] == "connected" && v["controls"]["revision"] == 40);
        let copy = guest.take_shared().expect("a fresh copy replaces the old one");
        assert_eq!(copy.revision, 40, "{place}");
        assert_eq!(back["guests_connected"], 1);
        same(&back["controls"], &state(&mut owner)["controls"], place);
        // The same place, not a second guest.
        wait(&mut owner, "counted back", |v| v["guests_connected"] == 1 && v["guests_reconnecting"] == 0);
    }
}

#[test]
fn a_background_sound_cut_midway_completes_after_the_resume_from_its_partial_copy() {
    for place in PLACES {
        let (host_dir, guest_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        // About 384 KiB as a float WAV: past the (test-sized) early-opening threshold, so it
        // arrives in the background, in slices of a few chunks, slowly enough to cut midway.
        let sound = Arc::new(suhina_engine::AudioData {
            sample_rate: 8000,
            channels: 1,
            samples: (0..96_000).map(|i| ((i % 997) as f32 / 997.0 - 0.5) * 0.8).collect(),
        });
        let mut project = audio_project();
        project.contents[0].lanes[0].data =
            suhina_model::LaneData::Audio(suhina_model::AudioRegion::new("/only-on-the-host/background.wav"));
        let mut pool = suhina_engine::AudioPool::new();
        pool.files.insert("/only-on-the-host/background.wav".into(), Arc::clone(&sound));
        let mut owner = plain();
        owner.set_storage_root(Some(host_dir.path().into())).unwrap();
        owner.select_audio_project(&project, &pool).unwrap();
        owner.execute("multiplayer.hostProject", &json!({"address":place})).unwrap();
        let invitation =
            wait(&mut owner, "hosting", |v| v["mode"] == "hosting")["invitation"].as_str().unwrap().to_owned();
        owner.take_shared().unwrap().keep();
        let faults = Arc::new(Faults::new(Duration::ZERO, Some(128 * 1024)));
        let mut guest = tuned(Some(Arc::clone(&faults)), Duration::from_secs(120), Duration::from_secs(30));
        guest.set_storage_root(Some(guest_dir.path().into())).unwrap();
        join(&mut guest, &invitation);
        let first = guest.take_shared().unwrap();
        assert!(first.session().unwrap().audio().files.is_empty(), "{place}: the project opens first");
        first.keep();
        // Cut once a good part of the sound has arrived in the background.
        let total: u64 = 96_000 * 4;
        let midway = wait(&mut guest, "background progress", |v| {
            v["receiving"][0].as_u64().is_some_and(|done| done > total / 4 && done < total * 3 / 4)
        });
        assert_eq!(midway["mode"], "connected");
        faults.cut();
        wait(&mut guest, "reconnecting", |v| v["mode"] == "reconnecting");
        let cache = guest_dir.path().join("Sound cache");
        let partial: u64 = std::fs::read_dir(&cache)
            .unwrap()
            // Windows reports a stale size for a file still open in a directory listing.
            .map(|e| std::fs::metadata(e.unwrap().path()).unwrap().len())
            .sum();
        assert!(partial >= total / 4, "{place}: the partial sound stays in the cache ({partial})");
        assert!(guest.take_audio().is_empty(), "{place}: the sound is not complete yet");
        let before = faults.carried();
        faults.heal();
        // The guest asks for a fresh copy (background slices belong to the first connection) and
        // fetches only the rest of the sound.
        wait(&mut guest, "resumed", |v| v["mode"] == "connected");
        let deadline = Instant::now() + Duration::from_secs(60);
        let addition = loop {
            let value = state(&mut guest);
            assert!(value["mode"] != "error", "{place}: {}", value["error"]);
            if let Some(addition) = guest.take_audio().into_iter().next() {
                break addition;
            }
            assert!(Instant::now() < deadline, "{place}: the background sound did not arrive");
            std::thread::sleep(Duration::from_millis(10));
        };
        let (_, data) = addition.audio.files.iter().next().unwrap();
        assert_eq!(data.as_ref(), sound.as_ref(), "{place}: the sound arrived intact");
        addition.keep();
        let after = faults.carried() - before;
        // Everything after the resume: a fresh portable copy and the rest of the sound only.
        let rest = total - partial;
        assert!(after < rest + 32 * 1024, "{place}: {after} bytes after the resume for {rest} missing");
        assert!(faults.cuts() >= 1);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1, "{place}: one cached sound, no second partial");
        assert_eq!(state(&mut guest)["mode"], "connected");
        owner.execute("multiplayer.stop", &json!({})).unwrap();
    }
}
