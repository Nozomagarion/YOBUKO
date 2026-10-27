// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;
use suhina_model::{LaneData, Project};
use suhina_session::Session;

#[test]
fn portable_audio_placement_is_pure_stacked_deterministic_and_one_undo_step() {
    let mut a = Session::new(Project::default());
    a.execute("track.add", &json!({"kind":"audio","name":"Voice"})).unwrap();
    a.execute("clip.create", &json!({"track":1,"start":0,"length":8})).unwrap();
    let before = a.project().clone();
    let mut b = Session::new(before.clone());
    let params = json!({"track":1,"start":2,"file":format!("Audio/{}.wav","a".repeat(64)),"rate":8000,"channels":1,"frames":8000});
    let result = a.execute("clip.insertAudio", &params).unwrap();
    assert_eq!(result, b.execute("clip.insertAudio", &params).unwrap());
    assert_eq!(a.project(), b.project());
    assert_eq!(a.project().arrangement.lanes[0].clips.len(), 2);
    assert_eq!(a.project().arrangement.lanes[0].clips[0], before.arrangement.lanes[0].clips[0]);
    let content = a.project().contents.last().unwrap();
    assert_eq!(content.name, "Shared take");
    let LaneData::Audio(region) = &content.lanes[0].data else { panic!("audio") };
    assert!(!region.warped);
    assert!(region.absolute_file.is_none());
    assert_eq!(region.duration_seconds, Some(1.0));
    assert!(a.audio().files.is_empty(), "pure placement does not follow paths or load files");
    a.execute("edit.undo", &json!({})).unwrap();
    // Native undo preserves the monotonic object counter so removed IDs are not reused.
    let mut expected = before;
    expected.next_id = a.project().next_id;
    assert_eq!(a.project(), &expected);
}

#[test]
fn malformed_collected_audio_never_mutates_the_project() {
    let mut session = Session::new(Project::default());
    session.execute("track.add", &json!({"kind":"audio"})).unwrap();
    let before = session.project().clone();
    let params = json!({"track":1,"start":0,"file":format!("Audio/{}.wav","b".repeat(64)),"rate":8000,"channels":1,"frames":8000});
    for (key, value) in [
        ("file", json!("../outside.wav")),
        ("file", json!("Audio/no.wav")),
        ("rate", json!(0)),
        ("channels", json!(3)),
        ("frames", json!(u64::MAX)),
        ("start", json!(-1)),
    ] {
        let mut bad = params.clone();
        bad[key] = value;
        assert!(session.execute("clip.insertAudio", &bad).is_err());
        assert_eq!(session.project(), &before);
    }
}
