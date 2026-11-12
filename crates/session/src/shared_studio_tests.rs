// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;

use super::*;

#[test]
fn studio_edits_are_shared() {
    for id in [
        "clip.create",
        "clip.moveMany",
        "note.move",
        "note.add",
        "track.add",
        "track.delete",
        "device.add",
        "device.setParam",
        "scene.insert",
        "transport.setTempo",
        "automation.addPoint",
    ] {
        assert!(shared_studio_command(id), "{id}");
        assert_eq!(shared_studio_id(id), Some(id));
    }
}

#[test]
fn reads_transport_history_and_local_edits_are_not_shared() {
    for id in [
        "project.tracks",
        "transport.play",
        "clip.launch",
        "edit.undo",
        "clip.paste",
        "track.setAudioInput",
        "device.addPlugin",
        "clip.consolidate",
        "project.collect",
        "no.such",
    ] {
        assert!(!shared_studio_command(id), "{id}");
        assert!(shared_studio_params(id, &json!({})).is_err(), "{id}");
    }
}

#[test]
fn parameters_naming_a_file_are_refused() {
    assert!(shared_studio_params("track.add", &json!({"kind": "midi"})).is_ok());
    assert!(shared_studio_params("track.add", &json!({"kind": "midi", "preset": "/etc/passwd"})).is_err());
    assert!(shared_studio_params("track.add", &json!({"kind": "audio", "sample": "a.wav"})).is_err());
    assert!(shared_studio_params("device.add", &json!({"track": 1, "id": "reverb", "preset": "x.fxp"})).is_err());
}

#[test]
fn unknown_large_and_deep_parameters_are_refused() {
    assert!(shared_studio_params("track.rename", &json!({"track": 1, "nme": "x"})).is_err());
    assert!(shared_studio_params("track.rename", &json!(["track"])).is_err());
    let long = "x".repeat(MAX_SHARED_PARAMS);
    assert!(shared_studio_params("track.rename", &json!({"track": 1, "name": long})).is_err());
    assert!(shared_studio_params("note.delete", &json!({"clip": 1, "notes": [[[[[1]]]]]})).is_err());
}

#[test]
fn random_edits_need_a_seed() {
    let params = json!({"track": 1, "device": 0, "amount": 0.5});
    assert!(shared_studio_params("device.randomize", &params).is_err());
    assert!(
        shared_studio_params("device.randomize", &json!({"track": 1, "device": 0, "amount": 0.5, "seed": 7})).is_ok()
    );
}
