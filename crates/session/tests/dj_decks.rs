// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::{dj::idle, dj_audition::seeded, temp_dir::TempDir};
use serde_json::json;
#[test]
fn dj_decks_load_without_output_play_together_and_never_write_catalog_or_project() {
    let dir = TempDir::new("dj-decks");
    let (mut p, out) = seeded(&dir);
    let before = std::fs::read(dir.join("dj-library.json")).unwrap();
    for deck in ["A", "B"] {
        p.execute("dj.deck.load", &json!({"deck":deck,"track":1})).unwrap();
        let s = idle(&mut p);
        assert!(s["performance"]["decks"][if deck == "A" { 0 } else { 1 }]["error"].is_null(), "{s}");
    }
    assert!(out.renderers.lock().unwrap().is_empty());
    for deck in ["A", "B"] {
        p.execute("dj.deck.play", &json!({"deck":deck})).unwrap();
    }
    assert_eq!(out.renderers.lock().unwrap().len(), 1);
    let mut audio = vec![0.0; 2048];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert!((audio[1600] - 0.2 * 0.501_187_2).abs() < 1e-4);
    let s = idle(&mut p);
    assert_eq!(s["performance"]["decks"][0]["playing"], true);
    assert_eq!(s["performance"]["decks"][1]["playing"], true);
    p.execute("dj.deck.pause", &json!({"deck":"A"})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert_eq!(idle(&mut p)["performance"]["decks"][0]["playing"], false);
    p.execute("dj.deck.stopAll", &json!({})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert!(audio.iter().all(|s| *s == 0.0));
    assert_eq!(std::fs::read(dir.join("dj-library.json")).unwrap(), before);
    assert!(p.is_empty());
    assert!(suhina_media::decode_file_bounded(&dir.join("song.wav"), 10).is_err());
    for id in [
        "dj.deck.load",
        "dj.deck.play",
        "dj.deck.pause",
        "dj.deck.stop",
        "dj.deck.clear",
        "dj.deck.seek",
        "dj.deck.stopAll",
        "dj.mixer.set",
    ] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn stopped_loading_and_folder_changes_cannot_start_or_keep_a_deck() {
    let dir = TempDir::new("dj-decks-cancel");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    p.execute("dj.deck.stopAll", &json!({})).unwrap();
    assert!(idle(&mut p)["performance"]["decks"][0]["track"].is_null());
    assert!(out.renderers.lock().unwrap().is_empty());
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 2048];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let other = TempDir::new("dj-decks-other");
    p.set_file_library(suhina_library::Library::new(Some(other.to_path_buf())));
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert!(audio.iter().all(|s| *s == 0.0));
    assert!(idle(&mut p)["performance"]["decks"][0]["track"].is_null());
}
#[test]
fn missing_files_positions_and_missing_headphone_output_are_visible_errors() {
    let dir = TempDir::new("dj-decks-errors");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    assert!(p.execute("dj.deck.seek", &json!({"deck":"A","from":2.0})).is_err());
    assert!(
        p.execute("dj.mixer.set", &json!({"crossfade":0.0,"levelA":1.0,"levelB":1.0,"cueA":true,"cueB":false}))
            .is_err()
    );
    std::fs::remove_file(dir.join("song.wav")).unwrap();
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    let s = idle(&mut p);
    assert!(s["performance"]["decks"][0]["error"].is_string());
    assert_eq!(s["performance"]["decks"][0]["track"], 1);
    p.execute("dj.deck.clear", &json!({"deck":"A"})).unwrap();
    assert!(idle(&mut p)["performance"]["decks"][0]["track"].is_null());
    assert!(out.renderers.lock().unwrap().is_empty());
}

#[test]
fn controls_remain_available_while_loading_and_switching_projects_cancels_pending_decks() {
    let dir = TempDir::new("dj-decks-context");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 2048];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    p.execute("dj.deck.load", &json!({"deck":"B","track":1})).unwrap();
    p.execute("dj.deck.pause", &json!({"deck":"A"})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert!(audio.iter().all(|s| *s == 0.0));
    let project = suhina_session::blank_project();
    let id = p.add(suhina_session::Session::new(project.clone()), None).unwrap();
    let state = idle(&mut p);
    assert!(state["performance"]["decks"][1]["track"].is_null());
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert!(audio.iter().all(|s| *s == 0.0));
    assert_eq!(p.get(id).unwrap().project(), &project);
    assert_eq!(out.renderers.lock().unwrap().len(), 1);
}
