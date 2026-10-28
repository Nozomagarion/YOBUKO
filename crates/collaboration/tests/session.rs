// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.

use serde_json::{Value, json};
use suhina_collaboration::{Edit, Error, Host, MAX_PEERS, Replica, Request, Role};
use suhina_engine::{AudioPool, Renderer};
use suhina_model::{Clip, ContentId, Note, Project};
use suhina_session::Player;
use suhina_test_support::ProjectBuilder;

fn project() -> Project {
    ProjectBuilder::new()
        .midi_track(1, "Bass")
        .midi_track(2, "Lead")
        .notes(1, "Riff", 80.0, vec![Note::new(0.0, 1.0, 36, 0.8)])
        .clip(1, Clip::new(ContentId(1), 0.0, 80.0))
        .build()
}

fn request(sequence: u64, base_revision: u64, command: &str, params: Value) -> Request {
    Request { sequence, base_revision, edit: Edit::new(command, params).unwrap() }
}

fn rename(sequence: u64, base_revision: u64, name: &str) -> Request {
    request(sequence, base_revision, "track.rename", json!({"track": 1, "name": name}))
}

#[test]
fn both_authors_edit_one_project_and_existing_object_ids_survive() {
    let mut host = Host::new(project()).unwrap();
    let editor = host.add_peer(Role::Editor).unwrap();
    let mut a = Replica::new(host.snapshot());
    let mut b = Replica::new(host.snapshot());
    let ids = host.project().next_id;
    let first = host.submit(Host::owner(), rename(1, 0, "Shared bass")).unwrap();
    let second = host.submit(editor, request(1, 1, "track.setPan", json!({"tracks": [2], "pan": 0.5}))).unwrap();
    assert_eq!(first.author(), Host::owner());
    assert_eq!(second.author(), editor);
    for accepted in [&first, &second] {
        assert!(a.apply(accepted).unwrap());
        assert!(b.apply(accepted).unwrap());
    }
    assert_eq!(a.project(), host.project());
    assert_eq!(b.project(), host.project());
    assert_eq!(host.project().next_id, ids);
    assert_eq!(host.project().tracks[0].name, "Shared bass");
    assert_eq!(host.project().channels[1].pan, 0.5);
    assert_eq!(host.revision(), 2);
    assert_eq!(a.revision(), 2);
    assert_eq!(b.revision(), 2);
}

#[test]
fn readers_cannot_edit_and_demoted_or_removed_peers_cannot_use_the_retry_cache() {
    let mut host = Host::new(project()).unwrap();
    let peer = host.add_peer(Role::Reader).unwrap();
    let before = host.project().clone();
    let req = rename(1, 0, "New name");
    assert_eq!(host.submit(peer, req.clone()), Err(Error::ReadOnly));
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 0);
    host.set_role(peer, Role::Editor).unwrap();
    host.submit(peer, req.clone()).unwrap();
    host.set_role(peer, Role::Reader).unwrap();
    assert_eq!(host.submit(peer, req.clone()), Err(Error::ReadOnly));
    host.remove_peer(peer).unwrap();
    assert_eq!(host.submit(peer, req), Err(Error::UnknownPeer));
    assert_eq!(host.revision(), 1);
}

#[test]
fn stale_edits_are_refused_without_consuming_the_authors_operation_number() {
    let mut host = Host::new(project()).unwrap();
    let peer = host.add_peer(Role::Editor).unwrap();
    host.submit(Host::owner(), rename(1, 0, "Owner's edit")).unwrap();
    let before = host.project().clone();
    assert_eq!(
        host.submit(peer, rename(1, 0, "Stale overwrite")),
        Err(Error::StaleRevision { expected: 1, received: 0 })
    );
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 1);
    host.submit(peer, rename(1, 1, "Explicitly retried")).unwrap();
    assert_eq!(host.project().tracks[0].name, "Explicitly retried");
}

#[test]
fn a_lost_ack_can_be_retried_after_another_author_edits_without_double_applying() {
    let mut host = Host::new(project()).unwrap();
    let peer = host.add_peer(Role::Editor).unwrap();
    let req = rename(1, 0, "Once");
    let ack = host.submit(peer, req.clone()).unwrap();
    host.submit(Host::owner(), rename(1, 1, "Later edit")).unwrap();
    assert_eq!(host.submit(peer, req.clone()).unwrap(), ack);
    assert_eq!(host.project().tracks[0].name, "Later edit");
    assert_eq!(host.revision(), 2);
    assert_eq!(host.submit(peer, rename(1, 0, "Reused number")), Err(Error::SequenceReused));
    host.submit(peer, rename(2, 2, "Second operation")).unwrap();
    assert_eq!(host.submit(peer, req), Err(Error::ReplayTooOld));
    assert_eq!(host.submit(peer, rename(4, 3, "Gap")), Err(Error::SequenceGap));
    assert_eq!(host.revision(), 3);
}

#[test]
fn a_replica_rejects_gaps_old_messages_and_other_sessions_without_changing_its_project() {
    let mut host = Host::new(project()).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let a = host.submit(Host::owner(), rename(1, 0, "A")).unwrap();
    let b = host.submit(Host::owner(), rename(2, 1, "B")).unwrap();
    let before = replica.project().clone();
    assert!(matches!(replica.apply(&b), Err(Error::StaleRevision { .. })));
    assert_eq!(replica.project(), &before);
    assert!(replica.apply(&a).unwrap());
    assert!(!replica.apply(&a).unwrap());
    assert!(replica.apply(&b).unwrap());
    assert!(matches!(replica.apply(&a), Err(Error::StaleRevision { .. })));
    let mut other = Host::new(project()).unwrap();
    let foreign = other.submit(Host::owner(), rename(1, 0, "Foreign")).unwrap();
    assert_eq!(replica.apply(&foreign), Err(Error::WrongSession));
    assert_eq!(replica.project(), host.project());
}

#[test]
fn owner_and_peer_bounds_are_enforced_and_removed_handles_are_not_reused() {
    let mut host = Host::new(project()).unwrap();
    assert_eq!(host.remove_peer(Host::owner()), Err(Error::OwnerProtected));
    assert_eq!(host.set_role(Host::owner(), Role::Reader), Err(Error::OwnerProtected));
    let first = host.add_peer(Role::Reader).unwrap();
    for _ in 2..MAX_PEERS {
        host.add_peer(Role::Reader).unwrap();
    }
    assert_eq!(host.add_peer(Role::Reader), Err(Error::PeerLimit));
    host.remove_peer(first).unwrap();
    let replacement = host.add_peer(Role::Reader).unwrap();
    assert_ne!(replacement, first);
    assert_eq!(host.set_role(first, Role::Editor), Err(Error::UnknownPeer));
}

#[test]
fn unknown_objects_do_not_advance_revision_or_sequence() {
    let mut host = Host::new(project()).unwrap();
    let before = host.project().clone();
    let missing = request(1, 0, "track.rename", json!({"track": 999, "name": "Missing"}));
    assert!(matches!(host.submit(Host::owner(), missing), Err(Error::Command(_))));
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 0);
    host.submit(Host::owner(), rename(1, 0, "Valid retry")).unwrap();
}

#[test]
fn no_op_edits_still_acknowledge_and_advance_protocol_order() {
    let mut host = Host::new(project()).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let before = host.project().clone();
    let ack = host.submit(Host::owner(), request(1, 0, "track.setPan", json!({"tracks": [1], "pan": 0}))).unwrap();
    assert!(replica.apply(&ack).unwrap());
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 1);
    assert_eq!(replica.revision(), 1);
}

#[test]
fn arbitrary_commands_files_batches_history_transport_and_extra_parameters_are_refused() {
    for command in [
        "project.save",
        "project.collect",
        "device.addPlugin",
        "edit.undo",
        "transport.play",
        "transport.setTempo",
        "batch",
        "track.add",
        "track.delete",
    ] {
        assert_eq!(Edit::new(command, json!({})), Err(Error::UnsupportedCommand));
    }
    for params in [
        json!(null),
        json!({"track": 1, "name": ""}),
        json!({"track": "Bass", "name": "X"}),
        json!({"track": 0, "name": "X"}),
        json!({"track": u64::MAX, "name": "X"}),
        json!({"track": 1, "name": "X", "path": "/private"}),
        json!({"track": 1, "name": "X", "continue": true}),
        json!({"track": 1, "name": "x".repeat(257)}),
    ] {
        assert_eq!(Edit::new("track.rename", params), Err(Error::InvalidParameters));
    }
    for tracks in [json!([]), json!([0]), json!(["Bass"]), json!([u64::MAX]), json!(vec![1; 65])] {
        assert_eq!(Edit::new("track.setVolume", json!({"tracks": tracks, "db": -6})), Err(Error::InvalidParameters));
    }
    for db in [json!(7), json!(-151), json!("-inf"), json!(null)] {
        assert_eq!(Edit::new("track.setVolume", json!({"tracks": [1], "db": db})), Err(Error::InvalidParameters));
    }
    assert_eq!(Edit::new("track.setPan", json!({"tracks": [1], "pan": 1.1})), Err(Error::InvalidParameters));
}

#[test]
fn two_real_renderers_keep_different_positions_and_stop_independently_during_edits() {
    const SR: u32 = 8000;
    let mut host = Host::new(project()).unwrap();
    let editor = host.add_peer(Role::Editor).unwrap();
    let mut a = Replica::new(host.snapshot());
    let mut b = Replica::new(host.snapshot());
    let mut render_a = Renderer::stopped(a.project(), &AudioPool::default(), SR).unwrap();
    let mut render_b = Renderer::stopped(b.project(), &AudioPool::default(), SR).unwrap();
    let monitor_a = render_a.monitor();
    let monitor_b = render_b.monitor();
    a.attach_player(Player::new(render_a.controller(), None));
    b.attach_player(Player::new(render_b.controller(), None));
    a.audition("transport.setPosition", &json!({"beats": 4})).unwrap();
    b.audition("transport.setPosition", &json!({"beats": 68})).unwrap();
    a.audition("transport.play", &json!({})).unwrap();
    b.audition("transport.play", &json!({})).unwrap();
    let mut buffer = [0.0; 128];
    render_a.process(&mut buffer);
    render_b.process(&mut buffer);
    assert!((monitor_b.beat() - monitor_a.beat() - 64.0).abs() < 1e-5);
    for (sequence, db) in [(1, -6.0), (2, -12.0), (3, -3.0)] {
        let ack = host
            .submit(editor, request(sequence, sequence - 1, "track.setVolume", json!({"tracks": [1], "db": db})))
            .unwrap();
        a.apply(&ack).unwrap();
        b.apply(&ack).unwrap();
        render_a.process(&mut buffer);
        render_b.process(&mut buffer);
        assert!((monitor_b.beat() - monitor_a.beat() - 64.0).abs() < 1e-5);
    }
    assert_eq!(a.project(), b.project());
    let revision = host.revision();
    a.audition("transport.stop", &json!({})).unwrap();
    render_a.process(&mut buffer);
    let previous_b = monitor_b.beat();
    render_b.process(&mut buffer);
    assert!(!monitor_a.running());
    assert!(monitor_b.running());
    assert!(monitor_b.beat() > previous_b);
    assert_eq!(host.revision(), revision);
    assert_eq!(a.audition("transport.setTempo", &json!({"bpm": 100})), Err(Error::UnsupportedCommand));
    assert_eq!(a.audition("transport.play", &json!({"from": "clip", "index": 0})), Err(Error::InvalidParameters));
}

#[test]
fn a_failed_player_update_requires_explicit_recovery_instead_of_silent_retry() {
    let mut host = Host::new(project()).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let mut render = Renderer::stopped(replica.project(), &AudioPool::default(), 8000).unwrap();
    replica.attach_player(Player::new(render.controller(), None));
    drop(render);
    let mut failed = None;
    // The abandoned consumer's queues still have capacity. Exhaust them with actual edits;
    // eventually both the parameter queue and graph fallback refuse a notification.
    for sequence in 1..=1100 {
        let db = if sequence % 2 == 0 { -12 } else { -6 };
        let ack = host
            .submit(Host::owner(), request(sequence, sequence - 1, "track.setVolume", json!({"tracks": [1], "db": db})))
            .unwrap();
        match replica.apply(&ack) {
            Ok(true) => {}
            Err(Error::RecoveryRequired) => {
                failed = Some(ack);
                break;
            }
            result => panic!("unexpected replica result: {result:?}"),
        }
    }
    let ack = failed.expect("a disconnected renderer's bounded queues must eventually fill");
    assert!(replica.recovery_required());
    assert_eq!(replica.apply(&ack), Err(Error::RecoveryRequired));
    let recovered = Replica::new(host.snapshot());
    assert!(!recovered.recovery_required());
    assert_eq!(recovered.project(), host.project());
    assert_eq!(recovered.revision(), host.revision());
}

#[test]
fn single_note_commands_converge_and_stale_indices_never_edit_another_note() {
    let mut host = Host::new(project()).unwrap();
    let peer = host.add_peer(Role::Editor).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let clip = host.project().arrangement.lanes[0].clips[0].id.0;
    let add = host
        .submit(
            peer,
            request(
                1,
                0,
                "note.add",
                json!({"clip":clip,
        "notes":[{"time":2,"length":1,"key":70,"velocity":95}]}),
            ),
        )
        .unwrap();
    replica.apply(&add).unwrap();
    let delete = host.submit(Host::owner(), request(1, 1, "note.delete", json!({"clip":clip,"notes":[0]}))).unwrap();
    replica.apply(&delete).unwrap();
    let before = host.project().clone();
    let stale = request(
        2,
        1,
        "note.update",
        json!({"clip":clip,"note":0,
        "time":3,"length":2,"key":40,"velocity":80,"muted":false}),
    );
    assert!(matches!(host.submit(peer, stale), Err(Error::StaleRevision { .. })));
    assert_eq!(host.project(), &before);
    let update = host
        .submit(
            peer,
            request(
                2,
                2,
                "note.update",
                json!({"clip":clip,"note":0,
        "time":3,"length":2,"key":40,"velocity":80,"muted":false}),
            ),
        )
        .unwrap();
    replica.apply(&update).unwrap();
    assert_eq!(replica.project(), host.project());
    for params in [
        json!({"clip":clip,"notes":[0,1]}),
        json!({"track":1,"index":0,"notes":[0]}),
        json!({"clip":clip,"notes":[0],"continue":true}),
    ] {
        assert!(Edit::new("note.delete", params).is_err());
    }
    assert!(Edit::new("note.move", json!({"clip":clip,"notes":[0],"beats":1})).is_err());
}

#[test]
fn clip_edits_allocate_identical_ids_and_keep_neighbours_on_each_replica() {
    let mut host = Host::new(project()).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let original = host.project().arrangement.lanes[0].clips[0].id.0;
    let operations = [
        ("clip.create", json!({"track":1,"start":2,"length":4,"stack":true})),
        ("clip.move", json!({"clip":original,"start":4,"toTrack":2,"stack":true})),
        ("clip.trim", json!({"clip":original,"start":4,"end":12,"stack":true})),
        ("clip.duplicate", json!({"clips":[{"clip":original}],"stack":true})),
        ("clip.setEnabled", json!({"clips":[{"clip":original}],"enabled":false})),
        ("clip.delete", json!({"clip":original})),
    ];
    for (i, (command, params)) in operations.into_iter().enumerate() {
        let ack = host.submit(Host::owner(), request(i as u64 + 1, i as u64, command, params)).unwrap();
        replica.apply(&ack).unwrap();
        assert_eq!(replica.project(), host.project());
        assert_eq!(replica.project().next_id, host.project().next_id);
    }
    assert_eq!(host.project().arrangement.lanes[0].clips.len(), 1, "the new clip survives other edits");
    assert_eq!(host.project().arrangement.lanes[1].clips.len(), 1, "the duplicate survives deleting its original");
}

#[test]
fn clip_admission_refuses_overwrite_batches_paths_and_legacy_index_addressing() {
    for (command, params) in [
        ("clip.move", json!({"clip":1,"start":0,"toTrack":2,"stack":false})),
        ("clip.move", json!({"track":1,"index":0,"start":0,"toTrack":2,"stack":true})),
        ("clip.trim", json!({"clip":1,"start":4,"end":4,"stack":true})),
        ("clip.trim", json!({"clip":1,"start":0,"end":4,"stack":true,"loop":true})),
        ("clip.create", json!({"track":1,"scene":1,"length":4,"stack":true})),
        ("clip.create", json!({"track":0,"start":0,"length":4,"stack":true})),
        ("clip.create", json!({"track":1,"start":0,"length":0,"stack":true})),
        ("clip.duplicate", json!({"clips":[{"clip":1},{"clip":2}],"stack":true})),
        ("clip.duplicate", json!({"clips":[{"clip":1}],"stack":false})),
        ("clip.delete", json!({"clip":1,"path":"/private"})),
        ("clip.setEnabled", json!({"clips":[{"clip":1}],"enabled":1})),
    ] {
        assert!(Edit::new(command, params).is_err(), "{command}");
    }
    for command in ["clip.paste", "clip.addFile", "clip.stretch", "clip.consolidate", "clip.moveMany"] {
        assert!(Edit::new(command, json!({})).is_err());
    }
}

#[test]
fn shared_device_parameters_keep_roles_replays_and_stale_or_invalid_requests_atomic() {
    let mut native = suhina_session::Session::new(project());
    native.execute("device.add", &json!({"track":1,"id":"fm-synth"})).unwrap();
    let mut host = Host::new(native.project().clone()).unwrap();
    let peer = host.add_peer(Role::Reader).unwrap();
    let p = json!({"track":1,"device":0,"param":"algorithm","value":8});
    let req = request(1, 0, "device.setSharedParam", p);
    assert_eq!(host.submit(peer, req.clone()), Err(Error::ReadOnly));
    host.set_role(peer, Role::Editor).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let ack = host.submit(peer, req.clone()).unwrap();
    replica.apply(&ack).unwrap();
    assert_eq!(host.submit(peer, req).unwrap(), ack);
    assert_eq!(host.revision(), 1);
    let before = host.project().clone();
    assert!(
        host.submit(
            peer,
            request(2, 0, "device.setSharedParam", json!({"track":1,"device":0,"param":"algorithm","value":2}))
        )
        .is_err()
    );
    assert!(
        host.submit(
            peer,
            request(2, 1, "device.setSharedParam", json!({"track":1,"device":0,"param":"algorithm","value":40}))
        )
        .is_err()
    );
    assert_eq!(host.project(), &before);
    assert_eq!(host.revision(), 1);
    let ack = host
        .submit(
            peer,
            request(2, 1, "device.setSharedParam", json!({"track":1,"device":0,"param":"algorithm","value":2})),
        )
        .unwrap();
    replica.apply(&ack).unwrap();
    assert_eq!(replica.project(), host.project());
    assert_eq!(Edit::new("device.setParam", json!({})), Err(Error::UnsupportedCommand));
    for (key, value) in [
        ("device", json!("0/0/0")),
        ("device", json!(16)),
        ("param", json!("../file")),
        ("param", json!("a".repeat(65))),
        ("track", json!("Bass")),
        ("continuous", json!(true)),
        ("value", json!(1e300)),
    ] {
        let mut p = json!({"track":1,"device":0,"param":"algorithm","value":2});
        p[key] = value;
        assert_eq!(Edit::new("device.setSharedParam", p), Err(Error::InvalidParameters));
    }
}

#[test]
fn shared_device_chain_replays_and_stale_indices_do_not_remove_a_different_device() {
    let mut host = Host::new(project()).unwrap();
    let peer = host.add_peer(Role::Reader).unwrap();
    let add = request(1, 0, "device.addShared", json!({"track":1,"kind":"utility"}));
    assert_eq!(host.submit(peer, add.clone()), Err(Error::ReadOnly));
    host.set_role(peer, Role::Editor).unwrap();
    let mut replica = Replica::new(host.snapshot());
    let accepted = host.submit(peer, add.clone()).unwrap();
    replica.apply(&accepted).unwrap();
    assert_eq!(host.submit(peer, add).unwrap(), accepted);
    let add = request(2, 1, "device.addShared", json!({"track":1,"kind":"utility"}));
    let accepted = host.submit(peer, add).unwrap();
    replica.apply(&accepted).unwrap();
    let before = host.project().clone();
    assert!(host.submit(peer, request(3, 0, "device.removeShared", json!({"track":1,"device":0}))).is_err());
    assert_eq!(host.project(), &before);
    let remove = request(3, 2, "device.removeShared", json!({"track":1,"device":0}));
    let accepted = host.submit(peer, remove.clone()).unwrap();
    replica.apply(&accepted).unwrap();
    assert_eq!(host.submit(peer, remove).unwrap(), accepted);
    assert_eq!(host.project().channels[0].devices.len(), 1);
    assert_eq!(replica.project(), host.project());
    assert_eq!(host.revision(), 3);
    for (id, p) in [
        ("device.addShared", json!({"track":1,"kind":"sampler"})),
        ("device.addShared", json!({"track":1,"kind":"utility","preset":"/file"})),
        ("device.removeShared", json!({"track":1,"device":"0/0/0"})),
    ] {
        assert!(Edit::new(id, p).is_err());
    }
    for id in ["device.add", "device.remove", "device.addPlugin"] {
        assert_eq!(Edit::new(id, json!({})), Err(Error::UnsupportedCommand));
    }
}

#[test]
fn shared_launcher_policy_refuses_extra_keys_names_paths_and_remote_launches() {
    for (command, params) in [
        ("scene.rename", json!({"scene":1,"name":"Intro","file":"/private"})),
        ("scene.insert", json!({"scene":"1","name":"Intro"})),
        ("scene.duplicate", json!({"scene":0})),
        ("scene.delete", json!({"scene":129})),
        ("clip.createSharedSlot", json!({"track":1,"scene":1,"length":4,"start":0})),
        ("clip.copyToSharedSlot", json!({"source":1,"track":"Lead","scene":1})),
        ("clip.moveSlot", json!({"track":1,"scene":1,"toTrack":2,"toScene":1,"overwrite":true})),
        ("scene.launch", json!({"scene":1})),
        ("clip.launch", json!({"track":1,"scene":1})),
    ] {
        assert!(Edit::new(command, params).is_err(), "{command}");
    }
}
