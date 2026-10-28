// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;
use suhina_session::{Session, blank_project};

#[test]
fn slot_copy_keeps_source_content_independent_and_supports_native_undo_and_roundtrip() {
    let mut s = Session::new(blank_project());
    let source = s.execute("clip.create", &json!({"track":1,"start":2,"length":8})).unwrap()["clip"].as_u64().unwrap();
    s.execute("note.add", &json!({"clip":source,"notes":[{"time":0,"length":1,"key":60,"velocity":100}]})).unwrap();
    let before = s.project().clone();
    let copy = s.execute("clip.copyToSharedSlot", &json!({"source":source,"track":2,"scene":2})).unwrap()["clip"]
        .as_u64()
        .unwrap();
    assert_ne!(source, copy);
    let source_content = s.project().arrangement.lanes[0].clips[0].content;
    let copied = s.project().launcher.columns[1].slots[1].clip.as_ref().unwrap();
    assert_ne!(source_content, copied.content);
    assert_eq!(copied.start, 0.0);
    assert_eq!(s.project().content(source_content).unwrap().lanes, s.project().content(copied.content).unwrap().lanes);
    s.execute("note.delete", &json!({"clip":copy,"notes":[0]})).unwrap();
    assert_eq!(s.project().content(source_content).unwrap(), before.content(source_content).unwrap());
    assert_eq!(s.project(), &suhina_project_file::from_json(&suhina_project_file::to_json(s.project())).unwrap());
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    let mut restored = s.project().clone();
    assert!(restored.next_id >= before.next_id);
    restored.next_id = before.next_id;
    assert_eq!(restored, before);
}

#[test]
fn slot_commands_refuse_missing_occupied_incompatible_and_invalid_targets_without_mutation() {
    let mut s = Session::new(blank_project());
    let source =
        s.execute("clip.createSharedSlot", &json!({"track":1,"scene":1,"length":4})).unwrap()["clip"].as_u64().unwrap();
    let before = s.project().clone();
    for params in [
        json!({"source":source,"track":1,"scene":1}),
        json!({"source":source,"track":3,"scene":2}),
        json!({"source":source,"track":2,"scene":0}),
        json!({"source":source,"track":2,"scene":129}),
        json!({"source":source,"track":2,"scene":9}),
        json!({"source":u64::MAX,"track":2,"scene":2}),
        json!({"source":source,"track":"2 MIDI","scene":2}),
    ] {
        assert!(s.execute("clip.copyToSharedSlot", &params).is_err());
        assert_eq!(s.project(), &before);
    }
    for length in [json!(0), json!(-1), json!(120001), json!(null)] {
        assert!(s.execute("clip.createSharedSlot", &json!({"track":2,"scene":2,"length":length})).is_err());
        assert_eq!(s.project(), &before);
    }
}

#[test]
fn audio_slot_copy_preserves_region_and_does_not_load_or_change_local_audio() {
    let mut s = Session::new(blank_project());
    let source =
        s.execute("clip.createSharedSlot", &json!({"track":3,"scene":1,"length":4})).unwrap()["clip"].as_u64().unwrap();
    let result = s.execute("clip.copyToSharedSlot", &json!({"source":source,"track":4,"scene":3})).unwrap();
    assert!(result["clip"].as_u64().is_some());
    assert!(s.audio().files.is_empty());
    let a = s.project().launcher.columns[2].slots[0].clip.as_ref().unwrap();
    let b = s.project().launcher.columns[3].slots[2].clip.as_ref().unwrap();
    assert_eq!(s.project().content(a.content).unwrap().lanes, s.project().content(b.content).unwrap().lanes);
}

#[test]
fn copy_refuses_targeted_patterns_without_redirecting_notes_or_consuming_ids() {
    let mut s = Session::new(blank_project());
    let source = s.execute("clip.create", &json!({"track":1,"start":0,"length":4})).unwrap()["clip"].as_u64().unwrap();
    let mut project = s.project().clone();
    project.contents[0].lanes[0].target = Some(suhina_model::TrackId(1));
    let mut s = Session::new(project);
    let before = s.project().clone();
    assert!(s.execute("clip.copyToSharedSlot", &json!({"source":source,"track":2,"scene":1})).is_err());
    assert_eq!(s.project(), &before);
}
