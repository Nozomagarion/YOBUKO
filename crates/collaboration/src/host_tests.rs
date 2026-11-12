// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.

use serde_json::json;
use suhina_session::blank_project;

use super::*;
use crate::Edit;

fn request(sequence: u64, base_revision: u64) -> Request {
    Request {
        sequence,
        base_revision,
        edit: Edit::new("track.rename", json!({"track": 1, "name": "Changed"})).unwrap(),
    }
}

#[test]
fn revision_overflow_does_not_execute_the_edit() {
    let mut host = Host::new(blank_project()).unwrap();
    host.revision = u64::MAX;
    let before = host.project().clone();
    assert_eq!(host.submit(Host::owner(), request(1, u64::MAX)), Err(Error::CounterExhausted));
    assert_eq!(host.project(), &before);
}

#[test]
fn operation_overflow_does_not_execute_the_edit() {
    let mut host = Host::new(blank_project()).unwrap();
    host.peers.get_mut(&Host::owner()).unwrap().next_sequence = u64::MAX;
    let before = host.project().clone();
    assert_eq!(host.submit(Host::owner(), request(u64::MAX, 0)), Err(Error::CounterExhausted));
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 0);
}

#[test]
fn peer_counter_overflow_does_not_add_a_member() {
    let mut host = Host::new(blank_project()).unwrap();
    host.next_peer = u64::MAX;
    assert_eq!(host.add_peer(Role::Editor), Err(Error::CounterExhausted));
    assert_eq!(host.peers.len(), 1);
}

#[test]
fn preflight_has_the_same_replay_and_membership_rules_without_consuming_anything() {
    let mut host = Host::new(blank_project()).unwrap();
    let peer = host.add_peer(Role::Editor).unwrap();
    let request = request(1, 0);
    let before = host.project().clone();
    assert!(host.preflight(peer, &request).unwrap().is_none());
    assert!(host.preflight(peer, &request).unwrap().is_none());
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 0);
    let accepted = host.submit(peer, request.clone()).unwrap();
    assert_eq!(host.preflight(peer, &request).unwrap(), Some(accepted));
    host.set_role(peer, Role::Reader).unwrap();
    assert_eq!(host.preflight(peer, &request), Err(Error::ReadOnly));
}

#[test]
fn a_studio_edit_from_an_older_revision_runs_on_the_latest_and_a_typed_one_does_not() {
    let mut host = Host::new(blank_project()).unwrap();
    let peer = host.add_peer(Role::Editor).unwrap();
    host.submit(Host::owner(), request(1, 0)).unwrap();
    // The peer had not seen revision 1 yet.
    let stale = Request { sequence: 1, base_revision: 0, ..request(1, 0) };
    assert!(matches!(host.submit(peer, stale), Err(Error::StaleRevision { .. })));
    let studio = Edit::studio("track.rename", json!({"track": 1, "name": "Last wins"})).unwrap();
    assert!(studio.rebases());
    let accepted = host.submit(peer, Request { sequence: 1, base_revision: 0, edit: studio }).unwrap();
    assert_eq!(accepted.revision, 2);
    assert_eq!(host.project().tracks[0].name, "Last wins");
    // A revision from the future is still refused.
    let future = Edit::studio("track.rename", json!({"track": 1, "name": "Future"})).unwrap();
    assert!(host.submit(peer, Request { sequence: 2, base_revision: 9, edit: future }).is_err());
}

#[test]
fn a_studio_edit_outside_the_project_is_refused() {
    assert_eq!(Edit::studio("clip.paste", json!({})), Err(Error::UnsupportedCommand));
    assert_eq!(Edit::studio("track.add", json!({"kind": "midi", "sample": "a.wav"})), Err(Error::InvalidParameters));
    assert!(Edit::studio("track.add", json!({"kind": "midi"})).is_ok());
}
