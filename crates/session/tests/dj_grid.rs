// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::{dj::idle, dj_audition::seeded, temp_dir::TempDir};
use serde_json::json;

#[test]
fn grid_commands_persist_without_output_and_preserve_analysis_cues_and_projects() {
    let dir = TempDir::new("dj-grid-commands");
    let (mut p, out) = seeded(&dir);
    let before = idle(&mut p);
    let bytes = std::fs::read(dir.join("song.wav")).unwrap();
    p.execute("dj.grid.set", &json!({"track":1,"revision":before["revision"],"bpm":120.0,"firstBeat":0.25})).unwrap();
    let state = idle(&mut p);
    assert!(state["error"].is_null(), "{state}");
    assert_eq!(state["grids"][0]["bpm"], 120.0);
    assert_eq!(state["grids"][0]["first_beat"], 0.25);
    assert_eq!(state["tracks"], before["tracks"]);
    assert_eq!(state["cues"], before["cues"]);
    assert!(out.renderers.lock().unwrap().is_empty());
    assert!(p.is_empty());
    assert_eq!(std::fs::read(dir.join("song.wav")).unwrap(), bytes);
    p.execute("dj.reload", &json!({})).unwrap();
    let state = idle(&mut p);
    assert_eq!(state["grids"][0]["first_beat"], 0.25);
    p.execute("dj.grid.clear", &json!({"track":1,"revision":state["revision"]})).unwrap();
    assert_eq!(idle(&mut p)["grids"], json!([]));
    for command in ["dj.grid.set", "dj.grid.clear"] {
        assert!(matches!(suhina_session::policy(command).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}
#[test]
fn grid_command_failures_keep_saved_values_and_invalid_inputs_are_refused() {
    let dir = TempDir::new("dj-grid-errors");
    let (mut p, _) = seeded(&dir);
    let state = idle(&mut p);
    for params in [
        json!({"track":1,"revision":0,"bpm":0.0,"firstBeat":0.0}),
        json!({"track":1,"revision":0,"bpm":120.0,"firstBeat":-1.0}),
    ] {
        assert!(p.execute("dj.grid.set", &params).is_err());
    }
    p.execute("dj.grid.set", &json!({"track":1,"revision":state["revision"],"bpm":120.0,"firstBeat":0.0})).unwrap();
    let saved = idle(&mut p);
    assert!(saved["error"].is_null());
    for params in [
        json!({"track":1,"revision":0,"bpm":100.0,"firstBeat":0.0}),
        json!({"track":1,"revision":saved["revision"],"bpm":120.0,"firstBeat":2.0}),
        json!({"track":99,"revision":saved["revision"],"bpm":120.0,"firstBeat":0.0}),
    ] {
        p.execute("dj.grid.set", &params).unwrap();
        let state = idle(&mut p);
        assert!(state["error"].is_string());
        assert_eq!(state["grids"], saved["grids"]);
    }
}
