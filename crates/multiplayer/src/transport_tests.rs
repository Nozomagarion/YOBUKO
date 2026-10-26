// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn expiration_wrong_secrets_and_reuse_do_not_admit_a_peer() {
    let now = Instant::now();
    let secret = "a".repeat(64);
    let mut host = Canonical {
        journal: VecDeque::new(),
        host: Host::new(crate::starter_project()).unwrap(),
        secret: secret_hash(&secret),
        expires: now + Duration::from_secs(1),
        redeemed: false,
        guest: false,
    };
    assert!(host.admit(&"b".repeat(64), now).is_err());
    assert!(!host.redeemed);
    assert!(host.admit(&secret, now + Duration::from_secs(2)).is_err());
    assert!(!host.redeemed);
    assert!(host.admit(&secret, now).is_ok());
    assert!(host.admit(&secret, now).is_err());
}

#[test]
fn frame_bounds_versions_and_typed_fields_are_checked_before_commands() {
    use std::io::Cursor;
    for size in [0u32, protocol::MAX_FRAME as u32 + 1, u32::MAX] {
        let mut input = Cursor::new(size.to_be_bytes());
        assert!(protocol::read::<ClientMessage>(&mut input).is_err());
        assert_eq!(input.position(), 4);
    }
    for value in [
        r#"{"kind":"Edit","protocol":1,"sequence":1,"base_revision":0,"edit":{"command":"project.open","path":"/private"}}"#,
        r#"{"kind":"Poll","protocol":1,"extra":true}"#,
        r#"{"kind":"Poll","protocol":1,"protocol":1}"#,
        r#"{"kind":"Edit","protocol":1,"sequence":1,"base_revision":0,"edit":{"command":"track.rename","track":1,"name":"x","name":"y"}}"#,
    ] {
        let mut bytes = (value.len() as u32).to_be_bytes().to_vec();
        bytes.extend(value.as_bytes());
        assert!(protocol::read::<ClientMessage>(&mut Cursor::new(bytes)).is_err());
    }
    assert!(WireEdit::Pan { track: 1, pan: f64::NAN }.edit().is_err());
    assert!(WireEdit::Rename { track: 0, name: "not shared".into() }.edit().is_err());
}

#[test]
fn a_certificate_mismatch_and_wrong_token_leave_the_real_invitation_usable() {
    let mut host = Service::default();
    host.execute("multiplayer.host", &json!({"address":"127.0.0.1:0"})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.invitation.is_none() && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let original = host.invitation.as_ref().unwrap().clone();
    for change_certificate in [true, false] {
        let mut invitation = Invitation::decode(&original).unwrap();
        if change_certificate {
            invitation.certificate = URL_SAFE_NO_PAD
                .encode(rcgen::generate_simple_self_signed(vec!["suhina.local".into()]).unwrap().cert.der());
        } else {
            invitation.secret = "b".repeat(64);
        }
        let mut guest = Service::default();
        guest.execute("multiplayer.join", &json!({"invitation":invitation.encode().unwrap()})).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while guest.active() && Instant::now() < deadline {
            guest.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!guest.active());
        assert!(guest.error.is_some());
    }
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":original})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while guest.mode != "connected" && Instant::now() < deadline {
        guest.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(guest.mode, "connected", "failure: {:?}", guest.error);
}

#[test]
fn shared_project_gate_blocks_edits_and_history_but_accepts_only_host_owned_controls() {
    let mut projects = suhina_session::Projects::new();
    let id = projects.add(suhina_session::Session::new(crate::starter_project()), None).unwrap();
    projects.restrict_shared_project(id, true).unwrap();
    let before = projects.get(id).unwrap().project().clone();
    assert!(projects.execute("track.rename", &json!({"track":1,"name":"bypass"})).is_err());
    assert!(projects.execute("edit.undo", &json!({})).is_err());
    assert!(projects.execute("track.setArm", &json!({"track":1,"arm":true})).is_err());
    assert_eq!(projects.get(id).unwrap().project(), &before);
    projects.apply_shared_control(id, "track.rename", &json!({"track":1,"name":"Confirmed"})).unwrap();
    assert!(projects.apply_shared_control(id, "track.delete", &json!({"track":1})).is_err());
    projects.restrict_shared_project(id, false).unwrap();
    assert!(projects.execute("track.rename", &json!({"track":1,"name":"Local again"})).is_ok());
}

#[test]
fn deadline_socket_exchanges_bytes_with_a_local_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    socket(&stream).unwrap();
    let mut stream = DeadlineSocket::new(stream);
    stream.write_all(b"hello").unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    let mut bytes = [0; 5];
    peer.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"hello");
}

fn invitation_from(service: &mut Service) -> Invitation {
    service.execute("multiplayer.host", &json!({"address":"127.0.0.1:0"})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.invitation.is_none() && Instant::now() < deadline {
        service.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    Invitation::decode(service.invitation.as_ref().unwrap()).unwrap()
}

fn connect(invitation: &Invitation) -> StreamOwned<ClientConnection, DeadlineSocket> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(invitation.der().unwrap())).unwrap();
    let tls = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = TcpStream::connect_timeout(&invitation.endpoint, TIMEOUT).unwrap();
    socket(&stream).unwrap();
    StreamOwned::new(
        ClientConnection::new(Arc::new(tls), ServerName::try_from("suhina.local").unwrap()).unwrap(),
        DeadlineSocket::new(stream),
    )
}

#[test]
fn incompatible_hello_does_not_consume_the_invitation_and_wire_retries_are_idempotent() {
    let mut host = Service::default();
    let invitation = invitation_from(&mut host);
    let mut incompatible = connect(&invitation);
    protocol::write(
        &mut incompatible,
        &ClientMessage::Hello { protocol: PROTOCOL + 1, format: 1, secret: invitation.secret.clone() },
    )
    .unwrap();
    assert!(matches!(protocol::read::<ServerMessage>(&mut incompatible).unwrap(), ServerMessage::Denied));
    let mut io = connect(&invitation);
    protocol::write(
        &mut io,
        &ClientMessage::Hello { protocol: PROTOCOL, format: 1, secret: invitation.secret.clone() },
    )
    .unwrap();
    let offer = protocol::read::<ServerMessage>(&mut io).unwrap();
    let document = crate::transfer::receive(&mut io, offer, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(crate::midi::decode(&document.0).is_ok());
    assert!(matches!(protocol::read::<ServerMessage>(&mut io).unwrap(), ServerMessage::State { .. }));
    let edit = ClientMessage::Edit {
        seen_revision: 0,
        protocol: PROTOCOL,
        sequence: 1,
        base_revision: 0,
        edit: WireEdit::Rename { track: 1, name: "One revision only".into() },
    };
    for _ in 0..2 {
        protocol::write(&mut io, &edit).unwrap();
        let ServerMessage::State { controls, accepted, .. } = protocol::read::<ServerMessage>(&mut io).unwrap() else {
            panic!("expected controls")
        };
        assert_eq!(controls.revision, 1);
        assert_eq!(accepted, Some(true));
    }
    protocol::write(
        &mut io,
        &ClientMessage::Edit {
            seen_revision: 0,
            protocol: PROTOCOL,
            sequence: 1,
            base_revision: 0,
            edit: WireEdit::Rename { track: 1, name: "Forged retry".into() },
        },
    )
    .unwrap();
    let ServerMessage::State { controls, accepted, .. } = protocol::read::<ServerMessage>(&mut io).unwrap() else {
        panic!("expected controls")
    };
    assert_eq!(controls.revision, 1);
    assert_eq!(controls.tracks[0].name, "One revision only");
    assert_eq!(accepted, Some(false));
    protocol::write(&mut io, &ClientMessage::Poll { protocol: PROTOCOL + 1, seen_revision: 0 }).unwrap();
    assert!(protocol::read::<ServerMessage>(&mut io).is_err());
}

#[test]
fn a_partial_unauthenticated_handshake_times_out_without_consuming_the_invitation() {
    let mut host = Service::default();
    let invitation = invitation_from(&mut host);
    let mut stalled = TcpStream::connect(invitation.endpoint).unwrap();
    stalled.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stalled.write_all(&[0x16]).unwrap();
    let start = Instant::now();
    assert_eq!(stalled.read(&mut [0; 1]).unwrap(), 0);
    assert!(start.elapsed() < Duration::from_secs(5));
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":invitation.encode().unwrap()})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while guest.mode != "connected" && Instant::now() < deadline {
        guest.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(guest.mode, "connected", "{:?}", guest.error);
}

#[test]
fn sixty_four_tracks_with_json_escaped_names_fit_a_control_frame() {
    let controls = Controls {
        revision: 0,
        tracks: (1..=64)
            .map(|track| crate::TrackControl { track, name: "\\".repeat(256), db: 0.0, pan: 0.0 })
            .collect(),
    };
    controls.validate().unwrap();
    let mut bytes = vec![];
    protocol::write(
        &mut bytes,
        &ServerMessage::State {
            protocol: PROTOCOL,
            controls: Box::new(controls.clone()),
            operations: (1..=32)
                .map(|revision| Operation { revision, edit: WireEdit::Rename { track: 1, name: "\\".repeat(256) } })
                .collect(),
            accepted: None,
            error: None,
        },
    )
    .unwrap();
    assert!(bytes.len() <= protocol::MAX_FRAME + 4);
    let ServerMessage::State { controls: back, .. } =
        protocol::read::<ServerMessage>(&mut std::io::Cursor::new(bytes)).unwrap()
    else {
        panic!("state")
    };
    assert_eq!(*back, controls);
    assert!(WireEdit::Rename { track: 1, name: "a\nb".into() }.edit().is_err());
}

#[test]
fn an_invalid_new_selection_cannot_leave_an_old_project_staged_for_sharing() {
    let mut service = Service::default();
    service.select_project(&crate::starter_project()).unwrap();
    let mut invalid = crate::starter_project();
    invalid.tracks[0].content.audio = true;
    assert!(service.select_project(&invalid).is_err());
    assert!(service.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).is_err());
    assert!(!service.active());
}

fn canonical(project: Project) -> Canonical {
    Canonical {
        host: Host::new(project).unwrap(),
        journal: VecDeque::new(),
        secret: [0; 32],
        expires: Instant::now() + Duration::from_secs(300),
        redeemed: false,
        guest: false,
    }
}

#[test]
fn journal_is_bounded_retries_are_not_duplicated_and_bad_cursors_fail() {
    let mut state = canonical(crate::starter_project());
    let edit = WireEdit::Rename { track: 1, name: "once".into() };
    state.submit(Host::owner(), 1, 0, edit.clone()).unwrap();
    state.submit(Host::owner(), 1, 0, edit).unwrap();
    assert_eq!(state.journal.len(), 1);
    assert!(state.since(2).is_err());
    for sequence in 2..=34 {
        state.submit(Host::owner(), sequence, sequence - 1, WireEdit::Pan { track: 1, pan: 0.1 }).unwrap();
    }
    assert_eq!(state.journal.len(), 32);
    assert!(state.since(1).is_err());
    let ops = state.since(2).unwrap();
    assert_eq!(ops.first().unwrap().revision, 3);
    assert_eq!(ops.last().unwrap().revision, 34);
    assert!(state.since(34).unwrap().is_empty());
}

#[test]
fn quota_preflight_refuses_copy_on_write_growth_before_the_host_or_sequence_changes() {
    use suhina_model::{Clip, ClipContent, ContentId};
    let mut p = crate::starter_project();
    let duplicate = Clip { id: Default::default(), start: 70.0, ..p.arrangement.lanes[0].clips[0].clone() };
    p.arrangement.lanes[0].clips.push(duplicate);
    for i in p.contents.len()..512 {
        p.contents.push(ClipContent {
            id: ContentId(2000 + i as u32),
            name: "Empty".into(),
            length: 4.0,
            lanes: vec![],
            extensions: Default::default(),
            scale: None,
        });
    }
    p.assign_ids().unwrap();
    crate::midi::validate(&p).unwrap();
    let mut state = canonical(p);
    let before = state.host.project().clone();
    let clip = before.arrangement.lanes[0].clips[0].id.0;
    assert!(
        state
            .submit(Host::owner(), 1, 0, WireEdit::NoteAdd { clip, time: 0., length: 1., key: 60, velocity: 90. })
            .is_err()
    );
    assert!(
        state
            .submit(
                Host::owner(),
                1,
                0,
                WireEdit::NoteUpdate { clip, note: 0, time: 1., length: 1., key: 60, velocity: 90., muted: false }
            )
            .is_err(),
        "COW also counts toward quotas"
    );
    assert_eq!(state.host.project(), &before);
    assert_eq!(state.host.revision(), 0);
    assert!(state.journal.is_empty());
    state.submit(Host::owner(), 1, 0, WireEdit::Rename { track: 1, name: "still usable".into() }).unwrap();
    assert_eq!(state.host.revision(), 1);
}

#[test]
fn malformed_operation_gaps_and_unconfirmed_controls_are_never_accepted() {
    use suhina_session::Session;
    let mut session = Session::new(crate::starter_project());
    let before = session.project().clone();
    let controls = Controls::from_project(&before, 1).unwrap();
    let mut revision = 0;
    let op = Operation { revision: 2, edit: WireEdit::Pan { track: 1, pan: 0.1 } };
    assert!(protocol::apply_operations(&mut session, &mut revision, &controls, &[op]).is_err());
    assert_eq!(session.project(), &before);
    assert!(protocol::apply_operations(&mut session, &mut revision, &controls, &[]).is_err());
}

#[test]
fn notes_changed_during_download_are_applied_after_the_snapshot_before_joining() {
    let mut host = Service::default();
    let invitation = invitation_from(&mut host);
    let clip = host.project.as_ref().unwrap().0.arrangement.lanes[0].clips[0].id.0;
    let mut io = connect(&invitation);
    protocol::write(&mut io, &ClientMessage::Hello { protocol: PROTOCOL, format: 1, secret: invitation.secret })
        .unwrap();
    let offer = protocol::read::<ServerMessage>(&mut io).unwrap();
    assert!(matches!(offer, ServerMessage::Project { revision: 0, .. }));
    host.execute(
        "multiplayer.edit",
        &json!({"base_revision":0,"edit":serde_json::to_string(
        &WireEdit::NoteAdd {clip,time:0.5,length:1.0,key:95,velocity:100.0}).unwrap()}),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.controls.as_ref().unwrap().revision < 1 && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let (document, mut revision) =
        crate::transfer::receive(&mut io, offer, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(revision, 0);
    let mut mirror = suhina_session::Session::new(crate::midi::decode(&document).unwrap());
    let ServerMessage::State { controls, operations, .. } = protocol::read::<ServerMessage>(&mut io).unwrap() else {
        panic!("state")
    };
    assert_eq!(operations.len(), 1);
    protocol::apply_operations(&mut mirror, &mut revision, &controls, &operations).unwrap();
    assert_eq!(revision, 1);
    let notes = mirror.execute("note.list", &json!({"clip":clip})).unwrap();
    assert_eq!(notes[4]["key"], 95);
}

#[test]
fn a_closed_worker_retains_its_last_verified_operations_until_the_ui_takes_them() {
    let (send, events) = mpsc::sync_channel(32);
    let (inputs, _) = mpsc::sync_channel(16);
    let stop = Arc::new(AtomicBool::new(false));
    let mut service = Service {
        running: Some(Running { stop, sockets: Arc::new(Mutex::new(vec![])), inputs, events }),
        ..Default::default()
    };
    let mut controls = Controls::from_project(&crate::starter_project(), 1).unwrap();
    controls.tracks[0].name = "Last confirmed change".into();
    let operation =
        Operation { revision: 1, edit: WireEdit::Rename { track: 1, name: controls.tracks[0].name.clone() } };
    assert!(send.try_send(Event::State(controls, vec![operation.clone()], true)).is_ok());
    assert!(send.try_send(Event::Failed("connection closed".into())).is_ok());
    service.poll();
    assert!(!service.active());
    let value = service.execute("multiplayer.state", &json!({})).unwrap();
    assert_eq!(value["controls"]["revision"], 1);
    let operations: Vec<Operation> = serde_json::from_value(value["operations"].clone()).unwrap();
    assert_eq!(operations, vec![operation]);
    assert!(service.execute("multiplayer.state", &json!({})).unwrap()["operations"].as_array().unwrap().is_empty());
}

#[test]
fn creating_a_clip_cannot_overrun_the_content_quota_or_consume_an_operation_number() {
    use suhina_model::{ClipContent, ContentId};
    let mut p = crate::starter_project();
    for i in p.contents.len()..512 {
        p.contents.push(ClipContent {
            id: ContentId(2000 + i as u32),
            name: "Empty".into(),
            length: 4.0,
            lanes: vec![],
            extensions: Default::default(),
            scale: None,
        });
    }
    let (p, _) = crate::midi::encode(&p).unwrap();
    let mut state = canonical(p);
    let before = state.host.project().clone();
    assert!(state.submit(Host::owner(), 1, 0, WireEdit::ClipCreate { track: 1, start: 0.0, length: 4.0 }).is_err());
    assert_eq!(state.host.project(), &before);
    assert_eq!(state.host.revision(), 0);
    state.submit(Host::owner(), 1, 0, WireEdit::Rename { track: 1, name: "Still usable".into() }).unwrap();
}

#[test]
fn duplicate_placement_quota_is_checked_before_allocating_canonical_ids() {
    use suhina_model::Clip;
    let mut p = crate::starter_project();
    let content = p.contents[0].id;
    for lane in &mut p.arrangement.lanes {
        lane.clips.clear();
    }
    p.arrangement.lanes[0].clips = (0..4096).map(|i| Clip::new(content, f64::from(i) * 4.0, 4.0)).collect();
    let (p, _) = crate::midi::encode(&p).unwrap();
    let clip = p.arrangement.lanes[0].clips[0].id.0;
    let mut state = canonical(p);
    let before = state.host.project().clone();
    assert!(state.submit(Host::owner(), 1, 0, WireEdit::ClipDuplicate { clip }).is_err());
    assert_eq!(state.host.project(), &before);
    assert_eq!(state.host.revision(), 0);
    let last = before.arrangement.lanes[0].clips.last().unwrap().id.0;
    state.submit(Host::owner(), 1, 0, WireEdit::ClipDelete { clip: last }).unwrap();
    state.submit(Host::owner(), 2, 1, WireEdit::ClipDuplicate { clip }).unwrap();
    assert_eq!(state.host.project().arrangement.lanes[0].clips.len(), 4096);
}

#[test]
fn clip_wire_shapes_are_exact_and_old_protocol_invitations_are_refused() {
    for text in [
        r#"{"command":"clip.move","clip":1,"start":0,"track":2,"stack":false}"#,
        r#"{"command":"clip.delete","clip":1,"path":"/private"}"#,
        r#"{"command":"clip.create","track":1,"start":0,"length":4,"scene":1}"#,
        r#"{"command":"clip.duplicate","clips":[1,2]}"#,
    ] {
        assert!(serde_json::from_str::<WireEdit>(text).is_err());
    }
    assert!(WireEdit::ClipMove { clip: 0, start: 0.0, track: 1 }.edit().is_err());
    assert!(WireEdit::ClipTrim { clip: 1, start: 4.0, end: 2.0 }.edit().is_err());
    assert!(WireEdit::ClipCreate { track: 1, start: f64::NAN, length: 4.0 }.edit().is_err());
    let mut service = Service::default();
    let mut invitation = invitation_from(&mut service);
    invitation.protocol = PROTOCOL - 1;
    assert!(Invitation::decode(&invitation.encode().unwrap()).is_err());
}
