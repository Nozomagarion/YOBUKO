// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers
use crate::temp_dir::TempDir;
use serde_json::{Value, json};
use suhina_session::{Agents, Projects};

fn projects(dir: &std::path::Path) -> Projects {
    let mut p = Projects::new();
    p.set_file_library(suhina_library::Library::new(Some(dir.to_owned())));
    p
}
pub(super) fn idle(p: &mut Projects) -> Value {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let result = p.execute("dj.library", &json!({})).unwrap();
        if result["busy"] != true {
            return result;
        }
        assert!(std::time::Instant::now() < until, "DJ worker timed out");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn edit(p: &mut Projects, command: &str, mut params: Value) -> Value {
    let state = idle(p);
    params["revision"] = state["revision"].clone();
    p.execute(command, &params).unwrap();
    let result = idle(p);
    assert!(result["error"].is_null(), "{result}");
    result
}
fn seed(dir: &std::path::Path) {
    let tracks: Vec<_> = (1..=3)
        .map(|id| suhina_dj::Track {
            id,
            path: dir.join(format!("{id}.wav")),
            title: format!("Track {id}"),
            seconds: 2.0,
            bpm: Some(120.0),
            key: None,
            source_project: Some(dir.join("original.suhina")),
        })
        .collect();
    suhina_dj::Catalog { tracks, ..Default::default() }.save(&dir.join("dj-library.json")).unwrap();
}

#[test]
fn dj_playlist_commands_persist_order_names_and_source_links_without_a_project() {
    let dir = TempDir::new("dj-playlist-commands");
    seed(&dir);
    let mut p = projects(&dir);
    let result = edit(&mut p, "dj.playlist.create", json!({"name":"Live"}));
    assert_eq!(result["created_playlist"], 1);
    assert!(idle(&mut p)["created_playlist"].is_null());
    for track in 1..=3 {
        edit(&mut p, "dj.playlist.addTrack", json!({"playlist":1,"track":track}));
    }
    edit(&mut p, "dj.playlist.moveTrack", json!({"playlist":1,"track":3,"position":1}));
    edit(&mut p, "dj.playlist.removeTrack", json!({"playlist":1,"track":2}));
    edit(&mut p, "dj.playlist.rename", json!({"playlist":1,"name":"Final"}));
    drop(p);
    let mut p = projects(&dir);
    let result = idle(&mut p);
    assert_eq!(result["playlists"][0]["tracks"], json!([3, 1]));
    assert_eq!(result["playlists"][0]["name"], "Final");
    assert_eq!(result["tracks"][0]["source_project"], json!(dir.join("original.suhina")));
    let result = edit(&mut p, "dj.playlist.delete", json!({"playlist":1}));
    assert_eq!(result["playlists"], json!([]));
    assert_eq!(result["tracks"].as_array().unwrap().len(), 3);
}

#[test]
fn stale_instances_need_a_reload_before_they_can_write() {
    let dir = TempDir::new("dj-stale-instance");
    seed(&dir);
    let mut a = projects(&dir);
    let mut b = projects(&dir);
    idle(&mut a);
    idle(&mut b);
    edit(&mut a, "dj.playlist.create", json!({"name":"A"}));
    b.execute("dj.playlist.create", &json!({"name":"B","revision":0})).unwrap();
    let stale = idle(&mut b);
    assert!(stale["error"].as_str().unwrap().contains("another app"));
    assert_eq!(stale["revision"], 0);
    assert_eq!(stale["playlists"], json!([]));
    b.execute("dj.reload", &json!({})).unwrap();
    assert_eq!(idle(&mut b)["playlists"][0]["name"], "A");
    let result = edit(&mut b, "dj.playlist.create", json!({"name":"B"}));
    assert_eq!(result["playlists"].as_array().unwrap().len(), 2);
    b.execute("dj.playlist.delete", &json!({"playlist":1,"revision":0})).unwrap();
    assert!(idle(&mut b)["error"].as_str().unwrap().contains("refresh"));
}

#[test]
fn malformed_catalog_can_be_repaired_and_refreshed_without_being_overwritten() {
    let dir = TempDir::new("dj-broken-catalog");
    let file = dir.join("dj-library.json");
    std::fs::write(&file, b"broken").unwrap();
    let mut p = projects(&dir);
    assert!(idle(&mut p)["error"].is_string());
    assert!(p.execute("dj.playlist.create", &json!({"name":"A","revision":0})).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"broken");
    seed(&dir);
    p.execute("dj.reload", &json!({})).unwrap();
    assert!(idle(&mut p)["error"].is_null());
    edit(&mut p, "dj.playlist.create", json!({"name":"Recovered"}));
}

#[test]
fn invalid_parameters_are_refused_and_all_dj_commands_are_person_only() {
    let dir = TempDir::new("dj-command-parameters");
    let mut p = projects(&dir);
    idle(&mut p);
    for params in [
        json!({"name":" ","revision":0}),
        json!({"name":"a\nb","revision":0}),
        json!({"name":"x".repeat(129),"revision":0}),
        json!({"name":"x","revision":-1}),
    ] {
        assert!(p.execute("dj.playlist.create", &params).is_err());
    }
    for command in [
        "dj.library",
        "dj.reload",
        "dj.import",
        "dj.playlist.create",
        "dj.playlist.rename",
        "dj.playlist.delete",
        "dj.playlist.addTrack",
        "dj.playlist.removeTrack",
        "dj.playlist.moveTrack",
    ] {
        assert!(matches!(suhina_session::policy(command).unwrap().agents, Agents::Denied(_)), "{command}");
    }
    assert!(p.execute("dj.playlist.addTrack", &json!({"playlist":4294967296u64,"track":1,"revision":0})).is_err());
    assert_eq!(idle(&mut p)["revision"], 0);
}

#[test]
fn failed_catalog_publication_keeps_the_saved_and_worker_copies() {
    let dir = TempDir::new("dj-failed-publication");
    seed(&dir);
    let path = dir.join("dj-library.json");
    let bytes = std::fs::read(&path).unwrap();
    let mut p = projects(&dir);
    idle(&mut p);
    std::fs::create_dir(dir.join("dj-library.lock")).unwrap();
    p.execute("dj.playlist.create", &json!({"name":"Retry me","revision":0})).unwrap();
    let failed = idle(&mut p);
    assert!(failed["error"].is_string());
    assert_eq!(failed["revision"], 0);
    assert_eq!(failed["playlists"], json!([]));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_dir(dir.join("dj-library.lock")).unwrap();
    let saved = edit(&mut p, "dj.playlist.create", json!({"name":"Retry me"}));
    assert_eq!(saved["playlists"][0]["id"], 1);
}

#[test]
fn dj_usb_export_writes_a_rekordbox_stick_and_refuses_an_empty_library() {
    let dir = TempDir::new("dj-usb-export");
    let mut p = projects(&dir);
    idle(&mut p);
    // Nothing in the library yet: refused before the worker is asked.
    let stick = dir.join("stick");
    std::fs::create_dir(&stick).unwrap();
    assert!(p.execute("dj.usb.export", &json!({"destination": stick})).is_err());
    assert!(p.execute("dj.usb.export", &json!({"destination": dir.join("missing")})).is_err());

    let audio = dir.join("tone.wav");
    let rate = 44_100u32;
    let stereo: Vec<f32> = (0..rate * 2)
        .flat_map(|i| {
            let s = (i as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin() * 0.3;
            [s, s]
        })
        .collect();
    suhina_media::write::write_audio_file(
        &audio,
        &stereo,
        &suhina_media::write::AudioFileSpec::float_wav(rate),
        suhina_media::write::WriteMode::Export { overwrite: false },
    )
    .unwrap();
    p.execute("dj.import", &json!({"path": audio})).unwrap();
    assert!(idle(&mut p)["error"].is_null());

    p.execute("dj.usb.export", &json!({"destination": stick})).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let result = loop {
        let result = p.execute("dj.library", &json!({})).unwrap();
        if result["busy"] != true {
            break result;
        }
        assert!(std::time::Instant::now() < until, "USB export timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert!(result["usb"]["error"].is_null(), "{result}");
    assert_eq!(result["usb"]["summary"]["tracks"], 1, "{result}");
    assert!(result["usb"]["progress"].is_null());
    assert!(stick.join("PIONEER/rekordbox/export.pdb").is_file());
}
