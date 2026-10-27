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
