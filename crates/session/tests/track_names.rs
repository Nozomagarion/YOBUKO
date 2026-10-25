// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Track names with the track's number in them and names that follow the instrument (#946).
#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

use serde_json::json;
use suhina_session::{Session, blank_project};

use crate::support::run;

fn shown(session: &Session) -> Vec<String> {
    let p = session.project();
    p.tracks.iter().map(|t| p.shown_track_name(t.id)).collect()
}

#[test]
fn new_tracks_read_their_number_and_follow_the_tracks_order() {
    let mut session = Session::new(blank_project());
    assert_eq!(shown(&session)[..4], ["1 MIDI", "2 MIDI", "3 Audio", "4 Audio"]);
    run(&mut session, "track.rename", json!({ "track": "2 MIDI", "name": "# Bass" })).unwrap();
    run(&mut session, "track.move", json!({ "track": "2 Bass", "to": 0 })).unwrap();
    assert_eq!(shown(&session)[..2], ["1 Bass", "2 MIDI"]);
    run(&mut session, "track.rename", json!({ "track": "1 Bass", "name": "5 Bass" })).unwrap();
    run(&mut session, "track.move", json!({ "track": "5 Bass", "to": 2 })).unwrap();
    assert!(shown(&session).contains(&"5 Bass".to_string()), "{:?}", shown(&session));
}

#[test]
fn a_track_never_renamed_takes_its_instruments_name_until_the_user_names_it() {
    let mut session = Session::new(blank_project());
    run(&mut session, "device.add", json!({ "track": "1 MIDI", "id": "fm-synth" })).unwrap();
    let name = |s: &Session| s.project().tracks.first().map(|t| t.name.clone()).unwrap_or_default();
    assert_eq!(name(&session), "FM Synth");
    // Undo takes the instrument and the name back in one step.
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert_eq!(name(&session), "# MIDI");
    run(&mut session, "edit.redo", json!({})).unwrap();
    // Removing the instrument brings the new tracks' name back.
    run(&mut session, "device.remove", json!({ "track": "FM Synth", "device": 0 })).unwrap();
    assert_eq!(name(&session), "# MIDI");
    // A name the user chose stays.
    run(&mut session, "track.rename", json!({ "track": "1 MIDI", "name": "Lead" })).unwrap();
    run(&mut session, "device.add", json!({ "track": "Lead", "id": "fm-synth" })).unwrap();
    assert_eq!(name(&session), "Lead");
}
