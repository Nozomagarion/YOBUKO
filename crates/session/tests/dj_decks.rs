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
#[test]
fn a_deck_keeps_its_speed_eq_and_filter_and_hands_them_to_the_audio_when_it_plays() {
    let dir = TempDir::new("dj-decks-tone");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    assert!(p.dj_deck_peaks(0).is_some_and(|w| w.frames > 0));
    assert!(p.dj_deck_peaks(1).is_none());
    p.execute("dj.deck.rate", &json!({"deck":"A","rate":1.25})).unwrap();
    p.execute("dj.deck.tone", &json!({"deck":"A","low":-40.0,"mid":0.0,"high":6.0,"filter":-0.5})).unwrap();
    let s = idle(&mut p);
    let a = &s["performance"]["decks"][0];
    assert_eq!(a["rate"], 1.25);
    assert_eq!(a["low"], -40.0);
    assert_eq!(a["high"], 6.0);
    assert_eq!(a["filter"], -0.5);
    assert_eq!(s["performance"]["decks"][1]["rate"], 1.0);
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 2048];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let seconds = idle(&mut p)["performance"]["decks"][0]["position"].as_f64().unwrap();
    assert!(seconds > 1024.0 / 44_100.0 * 1.2, "the deck plays at its speed: {seconds}");
    p.execute("dj.deck.rate", &json!({"deck":"A","rate":1.0})).unwrap();
    p.execute("dj.deck.tone", &json!({"deck":"A","low":0.0,"mid":0.0,"high":0.0,"filter":0.0})).unwrap();
    for wrong in [
        json!({"deck":"A","rate":0.4}),
        json!({"deck":"A","rate":2.5}),
        json!({"deck":"A"}),
        json!({"deck":"C","rate":1.0}),
        json!({"deck":"A","rate":"fast"}),
    ] {
        assert!(p.execute("dj.deck.rate", &wrong).is_err(), "{wrong}");
    }
    for wrong in [
        json!({"deck":"A","low":-41.0,"mid":0.0,"high":0.0,"filter":0.0}),
        json!({"deck":"A","low":0.0,"mid":7.0,"high":0.0,"filter":0.0}),
        json!({"deck":"A","low":0.0,"mid":0.0,"high":0.0,"filter":1.5}),
        json!({"deck":"A","low":0.0,"mid":0.0,"high":0.0}),
        json!({"deck":"B","low":0.0,"mid":0.0,"high":"x","filter":0.0}),
    ] {
        assert!(p.execute("dj.deck.tone", &wrong).is_err(), "{wrong}");
    }
    for id in ["dj.deck.rate", "dj.deck.tone"] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn a_deck_loops_nudges_and_trims_and_the_mixer_meters_master_and_headphones() {
    let dir = TempDir::new("dj-decks-loop");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    // A loop set before playing is kept, and handed to the audio when it plays.
    p.execute("dj.deck.loop", &json!({"deck":"A","from":0.0,"to":0.02})).unwrap();
    let mix = json!({"crossfade":0.0,"levelA":1.0,"levelB":1.0,"cueA":false,"cueB":false,"master":0.5});
    p.execute("dj.mixer.set", &mix).unwrap();
    p.execute("dj.deck.tone", &json!({"deck":"A","low":0.0,"mid":0.0,"high":0.0,"filter":0.0,"trim":-6.0})).unwrap();
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 4096];
    for _ in 0..4 {
        out.renderers.lock().unwrap()[0].process(&mut audio);
    }
    let s = idle(&mut p);
    let a = &s["performance"]["decks"][0];
    assert!(a["position"].as_f64().unwrap() <= 0.02, "{a}");
    assert_eq!((a["loopFrom"].as_f64(), a["loopTo"].as_f64(), &a["loopOn"]), (Some(0.0), Some(0.02), &json!(true)));
    assert_eq!(a["trim"], -6.0);
    assert_eq!(s["performance"]["master"], 0.5);
    assert_eq!(s["performance"]["phones"], 1.0, "left out, the headphones keep their level");
    let channel = a["meter"][0].as_f64().unwrap();
    assert!(channel > 0.0 && channel < 0.2, "the channel meter is after the trim: {channel}");
    assert!(s["performance"]["masterMeter"][0].as_f64().unwrap() > 0.0);
    // Let go, the loop is kept for reloop; reloop plays it again.
    p.execute("dj.deck.loop", &json!({"deck":"A","on":false})).unwrap();
    assert_eq!(idle(&mut p)["performance"]["decks"][0]["loopOn"], false);
    p.execute("dj.deck.loop", &json!({"deck":"A","on":true})).unwrap();
    p.execute("dj.deck.bend", &json!({"deck":"A","amount":0.04})).unwrap();
    p.execute("dj.deck.bend", &json!({"deck":"A","amount":0.0})).unwrap();
    for wrong in [
        json!({"deck":"A","from":0.5,"to":0.5}),
        json!({"deck":"A","from":0.1}),
        json!({"deck":"A","from":-1.0,"to":0.5}),
        json!({"deck":"A","on":"yes"}),
    ] {
        assert!(p.execute("dj.deck.loop", &wrong).is_err(), "{wrong}");
    }
    assert!(p.execute("dj.deck.loop", &json!({"deck":"B","on":true})).is_err(), "nothing loaded on B");
    assert!(p.execute("dj.deck.bend", &json!({"deck":"A","amount":0.5})).is_err());
    let trim = json!({"deck":"A","low":0.0,"mid":0.0,"high":0.0,"filter":0.0,"trim":13.0});
    assert!(p.execute("dj.deck.tone", &trim).is_err());
    let loud = json!({"crossfade":0.0,"levelA":1.0,"levelB":1.0,"cueA":false,"cueB":false,"phones":2.0});
    assert!(p.execute("dj.mixer.set", &loud).is_err());
    for id in ["dj.deck.loop", "dj.deck.bend"] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn a_deck_key_is_locked_or_moved_and_kept_for_the_next_play() {
    let dir = TempDir::new("dj-decks-key");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    p.execute("dj.deck.key", &json!({"deck":"A","lock":true})).unwrap();
    p.execute("dj.deck.rate", &json!({"deck":"A","rate":1.08})).unwrap();
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 4096];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    p.execute("dj.deck.key", &json!({"deck":"A","shift":-2.0})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let s = idle(&mut p);
    let a = &s["performance"]["decks"][0];
    assert_eq!((&a["keyLock"], &a["keyShift"]), (&json!(true), &json!(-2.0)), "{a}");
    assert!(a["position"].as_f64().unwrap() > 0.0);
    // Leaving one out keeps it.
    p.execute("dj.deck.key", &json!({"deck":"A","lock":false})).unwrap();
    assert_eq!(idle(&mut p)["performance"]["decks"][0]["keyShift"], -2.0);
    for wrong in [json!({"deck":"A","shift":13.0}), json!({"deck":"A","lock":"on"}), json!({"deck":"C","lock":true})] {
        assert!(p.execute("dj.deck.key", &wrong).is_err(), "{wrong}");
    }
    assert!(matches!(suhina_session::policy("dj.deck.key").unwrap().agents, suhina_session::Agents::Denied(_)));
    // A seek by an amount moves on from where the deck is; one way or the other, not both.
    p.execute("dj.deck.pause", &json!({"deck":"A"})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let before = idle(&mut p)["performance"]["decks"][0]["position"].as_f64().unwrap();
    p.execute("dj.deck.seek", &json!({"deck":"A","by":0.01})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let after = idle(&mut p)["performance"]["decks"][0]["position"].as_f64().unwrap();
    assert!((after - before - 0.01).abs() < 1e-6, "{before} {after}");
    assert!(p.execute("dj.deck.seek", &json!({"deck":"A","by":0.01,"from":0.0})).is_err());
    assert!(p.execute("dj.deck.seek", &json!({"deck":"A"})).is_err());
}
