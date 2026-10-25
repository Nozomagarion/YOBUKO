// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Comping from take lanes (#989): parts named and coloured after their takes, swapping a part's
//! take, moving the cut between parts, recolouring a take, and splits reaching the takes.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

use serde_json::json;
use suhina_model::*;
use suhina_session::Session;
use suhina_test_support::ProjectBuilder;

use crate::support::run;

/// An audio track "Vox" with three takes of eight beats, passes of one 24-beat file.
fn session() -> Session {
    let mut project =
        ProjectBuilder::new().audio_track(1, "Vox").audio(1, "Recording", 24.0, AudioRegion::new("take.wav")).build();
    let pass = |n: f64| Clip { offset: n * 8.0, ..Clip::new(ContentId(1), 0.0, 8.0) };
    project.arrangement.takes = (0..3)
        .map(|n| TakeLane {
            track: TrackId(1),
            name: format!("Take {}", n + 1),
            clips: vec![pass(f64::from(n))],
            color: None,
        })
        .collect();
    Session::new(project)
}

fn lane(session: &Session) -> Vec<Clip> {
    session.project().arrangement.lanes.iter().find(|l| l.track == Some(TrackId(1))).unwrap().clips.clone()
}

fn source(session: &Session, clip: &Clip) -> Option<usize> {
    session.project().comp_source(TrackId(1), clip)
}

#[test]
fn a_parts_take_is_swapped_in_turn_and_the_cut_between_parts_moves() {
    let mut s = session();
    run(&mut s, "take.comp", json!({ "track": 1, "take": 1, "start": 0.0, "end": 4.0 })).unwrap();
    run(&mut s, "take.comp", json!({ "track": 1, "take": 2, "start": 4.0, "end": 8.0 })).unwrap();
    let parts = lane(&s);
    assert_eq!(parts.iter().map(|c| source(&s, c)).collect::<Vec<_>>(), [Some(0), Some(1)]);
    assert_eq!(s.project().comp_used(1), [(4.0, 8.0)]);
    // Down from Take 1 is Take 2, then Take 3, then round to Take 1.
    let first = parts[0].id.0;
    run(&mut s, "take.swap", json!({ "clip": first, "direction": "down" })).unwrap();
    let swapped = lane(&s).into_iter().find(|c| c.start == 0.0).unwrap();
    assert_eq!(source(&s, &swapped), Some(1));
    assert_eq!((swapped.length, swapped.offset), (4.0, 8.0), "the same span of Take 2");
    run(&mut s, "take.swap", json!({ "clip": swapped.id.0, "direction": "up" })).unwrap();
    let back = lane(&s).into_iter().find(|c| c.start == 0.0).unwrap();
    assert_eq!(source(&s, &back), Some(0));
    // The cut at beat 4 moves to beat 6: Take 1 plays to 6, Take 2 from 6.
    run(&mut s, "take.moveCut", json!({ "clip": back.id.0, "to": 6.0 })).unwrap();
    let parts = lane(&s);
    assert_eq!(parts.len(), 2);
    assert_eq!((parts[0].start, parts[0].length, source(&s, &parts[0])), (0.0, 6.0, Some(0)));
    assert_eq!((parts[1].start, parts[1].length, parts[1].offset, source(&s, &parts[1])), (6.0, 2.0, 14.0, Some(1)));
    // And back to 3.
    run(&mut s, "take.moveCut", json!({ "clip": parts[0].id.0, "to": 3.0 })).unwrap();
    let parts = lane(&s);
    assert_eq!((parts[0].length, parts[1].start, parts[1].offset), (3.0, 3.0, 11.0));
    // Each step is one undo step.
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert_eq!(lane(&s)[0].length, 6.0);
}

#[test]
fn recolouring_a_take_colours_its_parts_and_a_split_cuts_the_takes() {
    let mut s = session();
    run(&mut s, "take.comp", json!({ "track": 1, "take": 2, "start": 0.0, "end": 8.0 })).unwrap();
    run(&mut s, "take.setColor", json!({ "track": 1, "take": 2, "color": "#ff00aa" })).unwrap();
    assert_eq!(s.project().arrangement.takes[1].color, Some(Rgb(0xff, 0x00, 0xaa)));
    assert_eq!(lane(&s)[0].color, Some(Rgb(0xff, 0x00, 0xaa)));
    assert_eq!(s.project().arrangement.takes[0].color, None, "other takes keep theirs");
    // One piece of a split take, and its part on the track, alone (Recolor).
    run(&mut s, "clip.split", json!({ "track": 1, "index": 0, "at": 4.0 })).unwrap();
    run(&mut s, "take.setColor", json!({ "track": 1, "take": 2, "color": "#00ff00", "piece": 2 })).unwrap();
    let take = &s.project().arrangement.takes[1];
    assert_eq!((take.clips[0].color, take.clips[1].color), (None, Some(Rgb(0, 0xff, 0))));
    let colours: Vec<Option<Rgb>> = lane(&s).iter().map(|c| c.color).collect();
    assert_eq!(colours, [Some(Rgb(0xff, 0x00, 0xaa)), Some(Rgb(0, 0xff, 0))]);
    // Recolor lane gives every piece the lane's colour again.
    run(&mut s, "take.setColor", json!({ "track": 1, "take": 2, "color": "#123456" })).unwrap();
    let take = &s.project().arrangement.takes[1];
    assert!(take.clips.iter().all(|c| c.color.is_none()));
    assert!(lane(&s).iter().all(|c| c.color == Some(Rgb(0x12, 0x34, 0x56))));
    run(&mut s, "edit.undo", json!({})).unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    // A split of the track's clip at beat 2 cuts every take there too.
    run(&mut s, "clip.split", json!({ "track": 1, "index": 0, "at": 2.0 })).unwrap();
    for take in &s.project().arrangement.takes {
        let cuts: Vec<(f64, f64)> = take.clips.iter().map(|c| (c.start, c.length)).collect();
        assert_eq!(cuts, [(0.0, 2.0), (2.0, 6.0)], "{}", take.name);
    }
    // The parts still know their take.
    assert!(lane(&s).iter().all(|c| source(&s, c) == Some(1)));
}
