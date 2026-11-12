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
#[test]
fn an_effect_unit_is_set_kept_and_handed_to_the_audio_when_a_deck_plays() {
    let dir = TempDir::new("dj-decks-fx");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    p.execute("dj.fx.set", &json!({"unit":1,"slot":1,"effect":"echo","on":true,"meta":0.7})).unwrap();
    p.execute("dj.fx.set", &json!({"unit":1,"mix":0.8,"beats":0.5,"bpm":124.0,"deckA":true})).unwrap();
    p.execute("dj.fx.set", &json!({"unit":2,"slot":3,"effect":"filter"})).unwrap();
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 4096];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let s = idle(&mut p);
    let fx = &s["performance"]["fx"];
    assert_eq!(fx[0]["slots"][0], json!({"effect":"echo","on":true,"meta":0.7f32 as f64}), "{fx}");
    assert_eq!((&fx[0]["deckA"], &fx[0]["deckB"], &fx[0]["beats"]), (&json!(true), &json!(false), &json!(0.5)));
    assert_eq!(fx[1]["slots"][2]["effect"], "filter");
    assert_eq!(fx[1]["slots"][2]["on"], false, "a new effect starts off");
    assert!(s["performance"]["effects"].as_array().unwrap().iter().any(|e| e["id"] == "reverb"));
    // Changes while it plays go to the audio at once.
    p.execute("dj.fx.set", &json!({"unit":1,"slot":1,"effect":"none"})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert_eq!(idle(&mut p)["performance"]["fx"][0]["slots"][0]["effect"], serde_json::Value::Null);
    for wrong in [
        json!({"unit":3}),
        json!({"unit":1,"slot":4,"on":true}),
        json!({"unit":1,"effect":"echo"}),
        json!({"unit":1,"slot":1,"effect":"wah"}),
        json!({"unit":1,"mix":2.0}),
        json!({"unit":1,"beats":64.0}),
        json!({"unit":1,"deckA":"yes"}),
    ] {
        assert!(p.execute("dj.fx.set", &wrong).is_err(), "{wrong}");
    }
    assert!(matches!(suhina_session::policy("dj.fx.set").unwrap().agents, suhina_session::Agents::Denied(_)));
}
#[test]
fn a_sampler_slot_loads_a_track_and_plays_it_from_its_start_at_its_level() {
    let dir = TempDir::new("dj-sampler");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.sampler.load", &json!({"slot":2,"track":1})).unwrap();
    let s = idle(&mut p);
    let slot = &s["performance"]["samplers"][1];
    assert_eq!((&slot["track"], &slot["loading"]), (&json!(1), &json!(false)), "{slot}");
    assert!(s["performance"]["decks"][0]["track"].is_null(), "a sampler load is not a deck load");
    p.execute("dj.sampler.gain", &json!({"slot":2,"gain":0.5})).unwrap();
    p.execute("dj.sampler.play", &json!({"slot":2})).unwrap();
    let mut audio = vec![0.0; 4096];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let s = idle(&mut p);
    let slot = &s["performance"]["samplers"][1];
    assert_eq!((&slot["playing"], &slot["gain"]), (&json!(true), &json!(0.5)), "{slot}");
    assert!(audio.iter().any(|x| x.abs() > 0.0), "it sounds");
    p.execute("dj.sampler.stop", &json!({"slot":2})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut audio);
    assert_eq!(idle(&mut p)["performance"]["samplers"][1]["playing"], false);
    p.execute("dj.sampler.clear", &json!({"slot":2})).unwrap();
    assert!(idle(&mut p)["performance"]["samplers"][1]["track"].is_null());
    assert!(p.execute("dj.sampler.play", &json!({"slot":2})).is_err(), "nothing loaded");
    for wrong in [json!({"slot":9,"track":1}), json!({"slot":0,"track":1}), json!({"slot":1,"track":99})] {
        assert!(p.execute("dj.sampler.load", &wrong).is_err(), "{wrong}");
    }
    assert!(p.execute("dj.sampler.gain", &json!({"slot":1,"gain":1.5})).is_err());
    // A deck still loads after the sampler did.
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    assert_eq!(idle(&mut p)["performance"]["decks"][0]["track"], 1);
    for id in ["dj.sampler.load", "dj.sampler.play", "dj.sampler.gain"] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn the_mix_is_recorded_to_a_wav_file_until_stopped() {
    let dir = TempDir::new("dj-record");
    let (mut p, out) = seeded(&dir);
    let folder = dir.join("Recordings");
    assert!(p.execute("dj.record.start", &json!({"folder": ""})).is_err(), "a folder is needed");
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    let started = p.execute("dj.record.start", &json!({"folder": folder})).unwrap();
    assert!(p.execute("dj.record.start", &json!({"folder": folder})).is_err(), "one recording at a time");
    let s = idle(&mut p);
    assert_eq!(s["performance"]["recording"]["on"], true, "{s}");
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let mut audio = vec![0.0; 2048];
    out.renderers.lock().unwrap()[0].process(&mut audio);
    let mut quiet = vec![0.0; 2048];
    p.execute("dj.deck.pause", &json!({"deck":"A"})).unwrap();
    out.renderers.lock().unwrap()[0].process(&mut quiet);
    let stopped = p.execute("dj.record.stop", &json!({})).unwrap();
    let path = std::path::PathBuf::from(stopped["path"].as_str().unwrap());
    assert_eq!(stopped["path"], started["path"]);
    assert!(path.starts_with(&folder) && path.extension().is_some_and(|e| e == "wav"), "{path:?}");
    let s = idle(&mut p);
    let rec = &s["performance"]["recording"];
    assert_eq!((&rec["on"], &rec["last"], &rec["error"]), (&json!(false), &stopped["path"], &json!(null)), "{rec}");
    let wav = hound::WavReader::open(&path).unwrap();
    assert_eq!((wav.spec().channels, wav.spec().bits_per_sample), (2, 16));
    let samples: Vec<i16> = wav.into_samples().map(Result::unwrap).collect();
    assert_eq!(samples.len(), 4096, "both blocks, the quiet one too");
    let heard = (audio[1600] * f32::from(i16::MAX)).round() as i16;
    assert!((samples[1600] - heard).abs() <= 1, "{} vs {heard}", samples[1600]);
    // Stopped twice is fine; a new recording starts on the same output and gets its own file.
    assert_eq!(p.execute("dj.record.stop", &json!({})).unwrap()["recording"], false);
    let again = p.execute("dj.record.start", &json!({"folder": folder})).unwrap();
    assert_ne!(again["path"], stopped["path"]);
    p.execute("dj.record.stop", &json!({})).unwrap();
    for id in ["dj.record.start", "dj.record.stop"] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn each_track_played_goes_in_the_days_history_once_a_load_and_crates_are_made() {
    let dir = TempDir::new("dj-history");
    let (mut p, out) = seeded(&dir);
    let render = || {
        if let Some(r) = out.renderers.lock().unwrap().first_mut() {
            r.process(&mut [0.0; 512]);
        }
    };
    assert_eq!(idle(&mut p)["history"]["days"], json!([]));
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    // Play, pause and play again: one play. Loaded again and played: another.
    for command in ["dj.deck.play", "dj.deck.pause", "dj.deck.play", "dj.deck.stop"] {
        p.execute(command, &json!({"deck":"A"})).unwrap();
        render();
    }
    p.execute("dj.deck.load", &json!({"deck":"A","track":1})).unwrap();
    idle(&mut p);
    p.execute("dj.deck.play", &json!({"deck":"A"})).unwrap();
    let s = idle(&mut p);
    let days = s["history"]["days"].as_array().unwrap();
    assert_eq!(days.len(), 1, "{s}");
    let plays = days[0]["plays"].as_array().unwrap();
    assert_eq!(plays.len(), 2);
    assert_eq!((&plays[0]["track"], &plays[0]["title"]), (&json!(1), &json!("Song")));
    assert_eq!(plays[0]["at"].as_str().map(str::len), Some(5), "hh:mm");
    // It is kept in its own file, read again by a new session.
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("dj-history.json")).unwrap()).unwrap();
    assert_eq!(file["days"][0]["plays"].as_array().map(Vec::len), Some(2));
    let day = days[0]["day"].as_str().unwrap().to_owned();
    assert!(p.execute("dj.history.delete", &json!({"day":"1999-01-01"})).is_err());
    p.execute("dj.history.delete", &json!({"day": day})).unwrap();
    assert_eq!(idle(&mut p)["history"]["days"], json!([]));
    // A damaged history file is reported and never written over.
    std::fs::write(dir.join("dj-history.json"), b"{oops").unwrap();
    let (mut q, _out) = seeded(&dir);
    assert!(idle(&mut q)["history"]["error"].is_string());
    q.execute("dj.deck.load", &json!({"deck":"B","track":1})).unwrap();
    idle(&mut q);
    q.execute("dj.deck.play", &json!({"deck":"B"})).unwrap();
    assert_eq!(std::fs::read(dir.join("dj-history.json")).unwrap(), b"{oops");
    // Crates.
    let revision = idle(&mut p)["revision"].clone();
    p.execute("dj.crate.create", &json!({"name":"House","revision":revision})).unwrap();
    let s = idle(&mut p);
    assert_eq!((&s["playlists"][0]["name"], &s["playlists"][0]["kind"]), (&json!("House"), &json!("crate")), "{s}");
    for id in ["dj.crate.create", "dj.history.delete"] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn auto_dj_plays_its_queue_on_both_decks_fading_from_outro_to_intro() {
    let dir = TempDir::new("dj-autodj");
    let (mut p, out) = seeded(&dir);
    let revision = idle(&mut p)["revision"].clone();
    // The song (2 s) has its intro at 0.5 s and its outro at 1.2 s.
    p.execute("dj.cue.add", &json!({"track":1,"revision":revision,"name":"Intro","seconds":0.5})).unwrap();
    let revision = idle(&mut p)["revision"].clone();
    p.execute("dj.cue.add", &json!({"track":1,"revision":revision,"name":"Outro","seconds":1.2})).unwrap();
    idle(&mut p);
    assert!(p.execute("dj.autodj.set", &json!({"on":true})).is_err(), "an empty queue");
    for _ in 0..2 {
        p.execute("dj.autodj.add", &json!({"track":1})).unwrap();
    }
    assert!(p.execute("dj.autodj.add", &json!({"track":99})).is_err());
    assert!(p.execute("dj.autodj.set", &json!({"fade":31})).is_err());
    p.execute("dj.autodj.set", &json!({"on":true,"fade":0.5})).unwrap();
    // The first track loads on A and starts at its intro, the crossfader on A.
    let mut s = idle(&mut p);
    for _ in 0..10 {
        if let Some(r) = out.renderers.lock().unwrap().first_mut() {
            r.process(&mut [0.0; 2]);
        }
        s = idle(&mut p);
        if s["performance"]["decks"][0]["playing"] == true {
            break;
        }
    }
    let a = &s["performance"]["decks"][0];
    assert_eq!((&a["playing"], &s["performance"]["crossfade"]), (&json!(true), &json!(0.0)), "{s}");
    assert!((a["position"].as_f64().unwrap() - 0.5).abs() < 0.01, "{a}");
    // Played on: B loads the next, and at A's outro B starts at its intro and the fader moves.
    let mut faded = false;
    let mut moved = false;
    for _ in 0..200 {
        out.renderers.lock().unwrap()[0].process(&mut [0.0; 512]);
        s = idle(&mut p);
        let x = s["performance"]["crossfade"].as_f64().unwrap();
        moved |= x > 0.0 && x < 1.0;
        if s["performance"]["decks"][1]["playing"] == true && s["performance"]["decks"][0]["playing"] == false {
            faded = true;
            break;
        }
    }
    assert!(faded && moved, "{s}");
    assert_eq!(s["performance"]["crossfade"], 1.0);
    let auto = &s["performance"]["autoDj"];
    assert_eq!((&auto["on"], &auto["queue"]), (&json!(true), &json!([])), "{auto}");
    // The queue is empty: B plays out and Auto DJ switches off.
    for _ in 0..200 {
        out.renderers.lock().unwrap()[0].process(&mut [0.0; 512]);
        s = idle(&mut p);
        if s["performance"]["autoDj"]["on"] == false {
            break;
        }
    }
    assert_eq!(s["performance"]["autoDj"]["on"], false, "{s}");
    p.execute("dj.autodj.add", &json!({"track":1})).unwrap();
    assert!(p.execute("dj.autodj.remove", &json!({"position":2})).is_err());
    p.execute("dj.autodj.remove", &json!({"position":1})).unwrap();
    for id in ["dj.autodj.set", "dj.autodj.add", "dj.autodj.skip"] {
        assert!(matches!(suhina_session::policy(id).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
