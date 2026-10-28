// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn expiration_wrong_secrets_and_reuse_do_not_admit_a_peer() {
    let now = Instant::now();
    let secret = "a".repeat(64);
    let mut host = Canonical {
        journal: VecDeque::new(),
        assets: Vec::new(),
        pending_audio: Vec::new(),
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
        &ClientMessage::Hello { protocol: PROTOCOL + 1, format: 2, secret: invitation.secret.clone() },
    )
    .unwrap();
    assert!(matches!(protocol::read::<ServerMessage>(&mut incompatible).unwrap(), ServerMessage::Denied));
    let mut io = connect(&invitation);
    protocol::write(
        &mut io,
        &ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret: invitation.secret.clone() },
    )
    .unwrap();
    let offer = protocol::read::<ServerMessage>(&mut io).unwrap();
    let document = crate::transfer::receive(&mut io, offer, &AtomicBool::new(false), |_, _| {}).unwrap();
    let project = crate::midi::decode_portable(&document.0).unwrap();
    let _snapshot = assets::receive(&mut io, project, document.1, None, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(matches!(read_state(&mut io), ServerMessage::State { .. }));
    let edit = ClientMessage::Edit {
        seen_revision: 0,
        protocol: PROTOCOL,
        sequence: 1,
        base_revision: 0,
        edit: WireEdit::Rename { track: 1, name: "One revision only".into() },
    };
    for _ in 0..2 {
        protocol::write(&mut io, &edit).unwrap();
        let ServerMessage::State { controls, accepted, .. } = read_state(&mut io) else { panic!("expected controls") };
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
    let ServerMessage::State { controls, accepted, .. } = read_state(&mut io) else { panic!("expected controls") };
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
        assets: Vec::new(),
        pending_audio: Vec::new(),
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
fn notes_and_tracks_changed_during_download_are_applied_before_joining() {
    let mut host = Service::default();
    let invitation = invitation_from(&mut host);
    let clip = host.project.as_ref().unwrap().project.arrangement.lanes[0].clips[0].id.0;
    let mut io = connect(&invitation);
    protocol::write(&mut io, &ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret: invitation.secret })
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
    host.execute(
        "multiplayer.edit",
        &json!({"base_revision":1,"edit":serde_json::to_string(
        &WireEdit::TrackCreate { kind:"audio".into(),name:"Created during download".into() }).unwrap()}),
    )
    .unwrap();
    while host.controls.as_ref().unwrap().revision < 2 && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let (document, mut revision) =
        crate::transfer::receive(&mut io, offer, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(revision, 0);
    let project = crate::midi::decode_portable(&document).unwrap();
    let snapshot = assets::receive(&mut io, project, revision, None, &AtomicBool::new(false), |_, _| {}).unwrap();
    let mut mirror = snapshot.session().unwrap();
    let ServerMessage::State { controls, operations, .. } = read_state(&mut io) else { panic!("state") };
    assert_eq!(operations.len(), 2);
    protocol::apply_operations(&mut mirror, &mut revision, &controls, &operations).unwrap();
    assert_eq!(revision, 2);
    assert!(mirror.project().tracks.iter().any(|t| t.name == "Created during download"));
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
    assert!(send.try_send(Event::State(controls, vec![operation.clone()], true, vec![])).is_ok());
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

fn read_state(io: &mut (impl Read + Write)) -> ServerMessage {
    let value = protocol::read::<ServerMessage>(io).unwrap();
    let assets = protocol::read::<ServerMessage>(io).unwrap();
    assert!(matches!(assets, ServerMessage::Assets { manifest, .. } if manifest.is_empty()));
    value
}

#[test]
fn a_guest_cannot_forge_a_publication_even_with_a_valid_looking_audio_reference() {
    let mut state = canonical(crate::starter_project());
    let peer = state.host.add_peer(Role::Editor).unwrap();
    let wire = WireEdit::AudioInsert {
        track: 1,
        start: 0.0,
        file: format!("Audio/{}.wav", "a".repeat(64)),
        rate: 8000,
        channels: 1,
        frames: 8000,
    };
    let before = state.host.project().clone();
    assert!(state.submit(peer, 1, 0, wire).is_err());
    assert_eq!(state.host.revision(), 0);
    assert_eq!(state.host.project(), &before);
    assert!(state.assets.is_empty());
}

#[test]
fn a_take_published_during_initial_download_is_verified_before_the_join_finishes() {
    let root = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let mut project = crate::starter_project();
    project.tracks[0].content = suhina_model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = suhina_model::LaneData::Audio(suhina_model::AudioRegion::new(""));
    let mut host = Service::default();
    host.set_storage_root(Some(root.path().into())).unwrap();
    host.select_audio_project(&project, &AudioPool::new()).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.invitation.is_none() && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let invitation = Invitation::decode(host.invitation.as_ref().unwrap()).unwrap();
    let mut io = connect(&invitation);
    protocol::write(&mut io, &ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret: invitation.secret })
        .unwrap();
    let offer = protocol::read::<ServerMessage>(&mut io).unwrap();
    assert!(matches!(offer, ServerMessage::Project { revision: 0, .. }));
    host.select_take(Arc::new(suhina_engine::AudioData { sample_rate: 8000, channels: 1, samples: vec![0.4; 8000] }))
        .unwrap();
    host.execute("multiplayer.publishTake", &json!({"base_revision":0,"track":1,"start":4})).unwrap();
    while host.controls.as_ref().unwrap().revision < 1 && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(host.controls.as_ref().unwrap().revision, 1);
    let (document, mut revision) =
        crate::transfer::receive(&mut io, offer, &AtomicBool::new(false), |_, _| {}).unwrap();
    let mut snapshot = assets::receive(
        &mut io,
        crate::midi::decode_portable(&document).unwrap(),
        revision,
        None,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert!(snapshot.audio.files.is_empty());
    let mut mirror = snapshot.session().unwrap();
    let ServerMessage::State { controls, operations, .. } = protocol::read(&mut io).unwrap() else { panic!("state") };
    protocol::apply_operations(&mut mirror, &mut revision, &controls, &operations).unwrap();
    let audio = assets::receive_known(
        &mut io,
        mirror.project().clone(),
        revision,
        Some(destination.path()),
        &AtomicBool::new(false),
        &assets::Known::new(),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(audio.audio.files.len(), 1);
    snapshot.project = mirror.project().clone();
    snapshot.add_audio(audio.into_audio());
    let ready = snapshot.session().unwrap();
    assert_eq!(ready.audio().files.len(), 1, "the first player gets the in-flight sound before attachment");
    assert!(ready.loaded_audio_paths().iter().all(|p| p.is_file()));
    let folder = snapshot.folder().unwrap().to_path_buf();
    ready.project().validate().unwrap();
    let mut ready = ready;
    ready.save(&folder.join("Joined.suhina"), false).unwrap();
    assert_eq!(revision, 1);
    assert_eq!(mirror.project().contents.last().unwrap().name, "Shared take");
    assert_eq!(ready.audio().files.values().next().unwrap().samples, vec![0.4; 8000]);
}

#[test]
fn shared_track_wire_is_exact_and_cannot_select_files_devices_inputs_or_ids() {
    for text in [
        r#"{"command":"track.createShared","kind":"audio","name":"Voice","device":"sampler"}"#,
        r#"{"command":"track.createShared","kind":"midi","name":"Keys","preset":"/do-not-open"}"#,
        r#"{"command":"track.createShared","kind":"audio","name":"Voice","track":99}"#,
        r#"{"command":"track.createShared","kind":"audio","name":"Voice","input":1}"#,
    ] {
        assert!(serde_json::from_str::<WireEdit>(text).is_err());
    }
    for (kind, name) in
        [("group", "Group".into()), ("audio", " ".into()), ("midi", "\n".into()), ("audio", "é".repeat(129))]
    {
        assert!(WireEdit::TrackCreate { kind: kind.into(), name }.edit().is_err());
    }
}

#[test]
fn shared_track_quotas_refuse_without_mutation_ids_or_consuming_an_author_sequence() {
    use suhina_model::{ArrangementLane, Channel, ChannelId, Device, DeviceKind};
    let mut tracks = suhina_session::Session::new(crate::starter_project());
    while tracks.project().tracks.len() < 64 {
        tracks.execute("track.createShared", &json!({"kind":"audio","name":"Bounded audio"})).unwrap();
    }
    let mut channels = crate::starter_project();
    while channels.channels.len() < 128 {
        let id = 200 + channels.channels.len() as u32;
        channels.channels.push(Channel::new(ChannelId(id), "Unused"));
    }
    let mut lanes = crate::starter_project();
    while lanes.arrangement.lanes.len() < 512 {
        lanes.arrangement.lanes.push(ArrangementLane {
            id: Default::default(),
            name: "Empty".into(),
            track: None,
            clips: vec![],
        });
    }
    lanes.assign_ids().unwrap();
    let mut devices = crate::starter_project();
    devices.channels.push(Channel::new(ChannelId(200), "Unused"));
    for c in &mut devices.channels {
        c.devices.clear();
    }
    for c in &mut devices.channels {
        c.devices = vec![Device::new("Utility", DeviceKind::Builtin("utility".into())); 16];
    }
    assert_eq!(devices.channels.iter().map(|c| c.devices.len()).sum::<usize>(), 128);
    for project in [tracks.project().clone(), channels, lanes, devices] {
        let mut state = canonical(project);
        let before = state.host.project().clone();
        assert!(
            state
                .submit(Host::owner(), 1, 0, WireEdit::TrackCreate { kind: "midi".into(), name: "Over budget".into() })
                .is_err()
        );
        assert_eq!(state.host.project(), &before);
        assert_eq!(state.host.revision(), 0);
        assert!(state.journal.is_empty());
        state
            .submit(Host::owner(), 1, 0, WireEdit::Rename { track: 1, name: "Counter still available".into() })
            .unwrap();
        assert_eq!(state.host.revision(), 1);
    }
}

fn guest_upload_host() -> (Service, Invitation, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let mut project = crate::starter_project();
    project.tracks[0].content = suhina_model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = suhina_model::LaneData::Audio(suhina_model::AudioRegion::new(""));
    let mut host = Service::default();
    host.set_storage_root(Some(root.path().into())).unwrap();
    host.select_audio_project(&project, &AudioPool::new()).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.invitation.is_none() && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let invite = Invitation::decode(host.invitation.as_ref().unwrap()).unwrap();
    host.take_shared().unwrap().keep();
    (host, invite, root)
}
fn admitted_upload(invite: &Invitation) -> StreamOwned<ClientConnection, DeadlineSocket> {
    let mut io = connect(invite);
    protocol::write(&mut io, &ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret: invite.secret.clone() })
        .unwrap();
    let offer = protocol::read(&mut io).unwrap();
    let (doc, revision) = crate::transfer::receive(&mut io, offer, &AtomicBool::new(false), |_, _| {}).unwrap();
    assets::receive(
        &mut io,
        crate::midi::decode_portable(&doc).unwrap(),
        revision,
        None,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    read_state(&mut io);
    io
}
fn offer_take(io: &mut impl Write, meta: &assets::Asset, sequence: u64, base: u64, seen: u64, track: u32) {
    protocol::write(
        io,
        &ClientMessage::Publish {
            protocol: PROTOCOL,
            sequence,
            base_revision: base,
            seen_revision: seen,
            track,
            start: 8.0,
            asset: meta.clone(),
        },
    )
    .unwrap();
}
fn upload_asset() -> Arc<assets::Encoded> {
    assets::encode_take(&suhina_engine::AudioData { sample_rate: 8000, channels: 1, samples: vec![0.3; 8000] }).unwrap()
}
fn empty_assets(io: &mut impl Read) {
    assert!(
        matches!(protocol::read::<ServerMessage>(io).unwrap(),ServerMessage::Assets { manifest,.. } if manifest.is_empty())
    );
}
#[test]
fn upload_preflight_refuses_bad_offers_before_bytes_and_accepted_retries_add_exactly_one_clip() {
    let (mut host, invite, root) = guest_upload_host();
    let mut io = admitted_upload(&invite);
    let asset = upload_asset();
    for (seq, base, track) in [(0, 0, 1), (2, 0, 1), (1, 9, 1), (1, 0, 2)] {
        offer_take(&mut io, &asset.meta, seq, base, 0, track);
        assert!(matches!(read_state(&mut io), ServerMessage::State { accepted: Some(false), .. }));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    let mut bad = asset.meta.clone();
    bad.hash = "../outside".into();
    offer_take(&mut io, &bad, 1, 0, 0, 1);
    assert!(matches!(read_state(&mut io), ServerMessage::State { accepted: Some(false), .. }));
    offer_take(&mut io, &asset.meta, 1, 0, 0, 1);
    let ack = assets::send_take(&mut io, &asset, &AtomicBool::new(false), |_, _| {}).unwrap();
    empty_assets(&mut io);
    assert!(matches!(ack,ServerMessage::State { accepted:Some(true),controls,.. } if controls.revision==1));
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.controls.as_ref().unwrap().revision < 1 && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let audio = host.take_audio();
    assert_eq!(audio.len(), 1);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    offer_take(&mut io, &asset.meta, 1, 0, 1, 1);
    assert!(
        matches!(read_state(&mut io),ServerMessage::State { accepted:Some(true),controls,operations,.. } if controls.revision==1 && operations.is_empty())
    );
    host.poll();
    assert!(host.take_audio().is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    drop(audio);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0, "an unclaimed UI lease cleans up");
}

#[test]
fn editing_during_upload_never_holds_the_host_mutex_and_refuses_the_finished_stale_take() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (mut host, invite, root) = guest_upload_host();
    let mut io = admitted_upload(&invite);
    let asset = upload_asset();
    offer_take(&mut io, &asset.meta, 1, 0, 0, 1);
    let first = protocol::read::<ServerMessage>(&mut io).unwrap();
    assert!(matches!(first, ServerMessage::UploadRequest { offset: 0, .. }));
    host.execute("multiplayer.edit",&json!({"base_revision":0,"edit":serde_json::to_string(&WireEdit::Rename { track:1,name:"While receiving".into() }).unwrap()})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.controls.as_ref().unwrap().revision < 1 && Instant::now() < deadline {
        host.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(host.controls.as_ref().unwrap().revision, 1, "host edits proceed while peer waits for chunk zero");
    for (i, chunk) in asset.bytes.chunks(16384).enumerate() {
        if i > 0 {
            assert!(
                matches!(protocol::read::<ServerMessage>(&mut io).unwrap(),ServerMessage::UploadRequest { offset,.. } if offset==i*16384)
            );
        }
        protocol::write(
            &mut io,
            &ClientMessage::UploadChunk { protocol: PROTOCOL, offset: i * 16384, data: STANDARD.encode(chunk) },
        )
        .unwrap();
    }
    assert!(
        matches!(read_state(&mut io),ServerMessage::State { accepted:Some(false),controls,.. } if controls.revision==1)
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    assert!(host.take_audio().is_empty());
    offer_take(&mut io, &asset.meta, 1, 1, 1, 1);
    assert!(
        matches!(assets::send_take(&mut io,&asset,&AtomicBool::new(false),|_,_|{}).unwrap(),ServerMessage::State { accepted:Some(true),controls,.. } if controls.revision==2)
    );
    empty_assets(&mut io);
}

#[test]
fn a_bad_hash_upload_disconnects_without_publishing_or_leaving_files() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (mut host, invite, root) = guest_upload_host();
    let mut io = admitted_upload(&invite);
    let asset = upload_asset();
    offer_take(&mut io, &asset.meta, 1, 0, 0, 1);
    let mut bytes = asset.bytes.clone();
    bytes[100] ^= 1;
    for (i, chunk) in bytes.chunks(16384).enumerate() {
        assert!(
            matches!(protocol::read::<ServerMessage>(&mut io).unwrap(),ServerMessage::UploadRequest { offset,.. } if offset==i*16384)
        );
        protocol::write(
            &mut io,
            &ClientMessage::UploadChunk { protocol: PROTOCOL, offset: i * 16384, data: STANDARD.encode(chunk) },
        )
        .unwrap();
    }
    assert!(protocol::read::<ServerMessage>(&mut io).is_err());
    host.poll();
    assert_eq!(host.controls.as_ref().unwrap().revision, 0);
    assert!(host.take_audio().is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn a_full_ui_queue_keeps_the_sound_lease_until_its_operation_can_be_delivered_with_it() {
    let root = tempfile::tempdir().unwrap();
    let mut project = crate::starter_project();
    project.tracks[0].content = suhina_model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = suhina_model::LaneData::Audio(suhina_model::AudioRegion::new(""));
    let mut state = canonical(project);
    let asset = upload_asset();
    let collected = assets::collect_take(&asset, Some(root.path()), &AtomicBool::new(false)).unwrap();
    let path = collected.folder().unwrap().to_path_buf();
    state.commit_take(Host::owner(), 1, 0, (1, 0.0), asset, Some(collected)).unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    assert!(send.try_send(Event::Upload(None)).is_ok());
    let mut last = (0, false);
    state.emit(&send, &mut last).unwrap();
    assert_eq!(last, (0, false));
    assert_eq!(state.pending_audio.len(), 1);
    assert!(path.exists());
    receive.recv().unwrap();
    state.emit(&send, &mut last).unwrap();
    assert_eq!(last, (1, false));
    assert!(state.pending_audio.is_empty());
    let Event::State(controls, operations, _, audio) = receive.recv().unwrap() else { panic!("state") };
    assert_eq!(controls.revision, 1);
    assert_eq!(operations.len(), 1);
    assert_eq!(audio.len(), 1);
    assert!(path.exists());
    drop(audio);
    assert!(!path.exists());
}

#[test]
fn renewal_revokes_expired_and_spent_secrets_without_resetting_music_or_owner_sequences() {
    let now = Instant::now();
    let mut state = canonical(crate::starter_project());
    let original = "a".repeat(64);
    state.secret = secret_hash(&original);
    state.expires = now;
    state.redeemed = true;
    state.submit(Host::owner(), 1, 0, WireEdit::Rename { track: 1, name: "Keep this".into() }).unwrap();
    let music = state.host.project().clone();
    let (send, receive) = mpsc::sync_channel(1);
    let invite = Invitation {
        protocol: PROTOCOL,
        endpoint: "127.0.0.1:1".parse().unwrap(),
        certificate: "YQ".into(),
        secret: "b".repeat(64),
    };
    state.renew(&invite, now, &send).unwrap();
    assert!(matches!(receive.try_recv().unwrap(), Event::Invitation(_)));
    assert_eq!(state.host.project(), &music);
    assert_eq!(state.host.revision(), 1);
    assert_eq!(state.journal.len(), 1);
    assert!(state.admit(&original, now).is_err());
    assert!(!state.redeemed);
    let peer = state.admit(&invite.secret, now + Duration::from_secs(299)).unwrap();
    // Admission winning the lock prevents renewal, and the live editor keeps its identity.
    let next = Invitation { secret: "c".repeat(64), ..invite };
    assert!(state.renew(&next, now, &send).is_err());
    state.apply(peer, 1, 1, WireEdit::Pan { track: 1, pan: 0.25 }).unwrap();
    state.host.remove_peer(peer).unwrap();
    state.guest = false;
    state.renew(&next, now, &send).unwrap();
    assert!(state.admit(&next.secret, now + Duration::from_secs(300)).is_err());
    assert!(!state.redeemed);
    state.submit(Host::owner(), 2, 2, WireEdit::Pan { track: 1, pan: -0.25 }).unwrap();
    assert_eq!(state.host.revision(), 3);
}

#[test]
fn renewal_backpressure_and_closed_views_preserve_the_previous_invitation() {
    let now = Instant::now();
    let original = "a".repeat(64);
    for closed in [false, true] {
        let mut state = canonical(crate::starter_project());
        state.secret = secret_hash(&original);
        let expires = state.expires;
        let (send, receive) = mpsc::sync_channel(1);
        assert!(send.try_send(Event::Closed).is_ok());
        if closed {
            drop(receive);
        }
        let invite = Invitation {
            protocol: PROTOCOL,
            endpoint: "127.0.0.1:1".parse().unwrap(),
            certificate: "YQ".into(),
            secret: "b".repeat(64),
        };
        assert!(state.renew(&invite, now, &send).is_err());
        assert!(state.secret == secret_hash(&original));
        assert_eq!(state.expires, expires);
        assert!(!state.redeemed && !state.guest);
        assert!(state.admit(&original, now).is_ok());
    }
}

#[test]
fn renewal_refuses_a_repeated_secret_without_consuming_the_old_invitation() {
    let now = Instant::now();
    let secret = "a".repeat(64);
    let mut state = canonical(crate::starter_project());
    state.secret = secret_hash(&secret);
    let (send, receive) = mpsc::sync_channel(1);
    let invite =
        Invitation { protocol: PROTOCOL, endpoint: "127.0.0.1:1".parse().unwrap(), certificate: "YQ".into(), secret };
    assert!(state.renew(&invite, now, &send).is_err());
    assert!(receive.try_recv().is_err());
    assert!(state.admit(&invite.secret, now).is_ok());
}

#[test]
fn a_hello_started_before_renewal_cannot_consume_the_new_invitation() {
    let mut host = Service::default();
    let original = invitation_from(&mut host);
    let mut old_io = connect(&original);
    host.execute("multiplayer.renewInvitation", &json!({})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        host.poll();
        let current = Invitation::decode(host.invitation.as_ref().unwrap()).unwrap();
        if current.secret != original.secret {
            assert_eq!(current.endpoint, original.endpoint);
            assert!(current.certificate == original.certificate);
            break;
        }
        assert!(Instant::now() < deadline, "invitation renewal timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    protocol::write(&mut old_io, &ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret: original.secret })
        .unwrap();
    assert!(matches!(protocol::read::<ServerMessage>(&mut old_io).unwrap(), ServerMessage::Denied));
    let mut guest = Service::default();
    guest.execute("multiplayer.join", &json!({"invitation":host.invitation})).unwrap();
    while guest.mode != "connected" && Instant::now() < deadline {
        guest.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(guest.mode, "connected", "{:?}", guest.error);
}

#[test]
fn shared_device_wire_is_exact_and_refuses_paths_gestures_and_nonfinite_values() {
    let valid = json!({"command":"device.setSharedParam","track":1,"device":0,"param":"op1.on","value":0});
    let edit: WireEdit = serde_json::from_value(valid.clone()).unwrap();
    assert!(edit.edit().is_ok());
    assert_eq!(
        serde_json::to_value(edit).unwrap(),
        json!({"command":"device.setSharedParam","track":1,"device":0,"param":"op1.on","value":0.0})
    );
    for (key, value) in [
        ("path", json!("/must-not-read")),
        ("gesture", json!("drag")),
        ("device", json!("0/0/0")),
        ("track", json!(-1)),
        ("value", json!(null)),
    ] {
        let mut data = valid.clone();
        data[key] = value;
        assert!(serde_json::from_value::<WireEdit>(data).is_err());
    }
    for edit in [
        WireEdit::DeviceParam { track: 0, device: 0, param: "gain".into(), value: 0. },
        WireEdit::DeviceParam { track: 1, device: 16, param: "gain".into(), value: 0. },
        WireEdit::DeviceParam { track: 1, device: 0, param: "x".repeat(65), value: 0. },
        WireEdit::DeviceParam { track: 1, device: 0, param: "gain".into(), value: f64::NAN },
    ] {
        assert!(edit.edit().is_err());
    }
}

#[test]
fn shared_chain_wire_shapes_refuse_presets_paths_positions_and_invalid_indices() {
    for data in [
        json!({"command":"device.addShared","track":1,"kind":"utility"}),
        json!({"command":"device.removeShared","track":1,"device":0}),
    ] {
        let edit: WireEdit = serde_json::from_value(data.clone()).unwrap();
        assert!(edit.edit().is_ok());
        assert_eq!(serde_json::to_value(edit).unwrap(), data);
        for (key, value) in
            [("preset", json!("/file.syx")), ("position", json!(0)), ("state", json!([1])), ("path", json!("/file"))]
        {
            let mut bad = data.clone();
            bad[key] = value;
            assert!(serde_json::from_value::<WireEdit>(bad).is_err());
        }
    }
    for edit in [
        WireEdit::DeviceAdd { track: 0, kind: "utility".into() },
        WireEdit::DeviceAdd { track: 1, kind: "sampler".into() },
        WireEdit::DeviceRemove { track: 1, device: 16 },
    ] {
        assert!(edit.edit().is_err());
    }
}

#[test]
fn full_shared_chains_refuse_addition_without_consuming_revision_or_owner_sequence() {
    let mut project = crate::starter_project();
    project.channels[0].devices = (0..16)
        .map(|_| suhina_model::Device::new("Utility", suhina_model::DeviceKind::Builtin("utility".into())))
        .collect();
    let mut state = canonical(project.clone());
    assert!(state.submit(Host::owner(), 1, 0, WireEdit::DeviceAdd { track: 1, kind: "utility".into() }).is_err());
    assert_eq!(state.host.project(), &project);
    assert_eq!(state.host.revision(), 0);
    assert!(state.journal.is_empty());
    state.submit(Host::owner(), 1, 0, WireEdit::DeviceRemove { track: 1, device: 15 }).unwrap();
    state.submit(Host::owner(), 2, 1, WireEdit::DeviceAdd { track: 1, kind: "utility".into() }).unwrap();
    assert_eq!(state.host.revision(), 2);
    assert_eq!(state.host.project().channels[0].devices.len(), 16);
}

#[test]
fn launcher_wire_is_exact_and_scene_names_and_indices_are_bounded() {
    let good = [
        WireEdit::SceneInsert { scene: 1, name: "Intro".into() },
        WireEdit::SceneRename { scene: 1, name: String::new() },
        WireEdit::SceneDuplicate { scene: 1 },
        WireEdit::SceneDelete { scene: 1 },
        WireEdit::SlotCreate { track: 1, scene: 1, length: 4. },
        WireEdit::SlotCopy { source: 1, track: 1, scene: 1 },
        WireEdit::SlotMove { track: 1, scene: 1, to_track: 2, to_scene: 2 },
    ];
    for edit in good {
        assert!(edit.edit().is_ok());
        let mut value = serde_json::to_value(&edit).unwrap();
        let decoded: WireEdit = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decoded, edit);
        value["file"] = json!("/must-not-read");
        assert!(serde_json::from_value::<WireEdit>(value).is_err());
    }
    for edit in [
        WireEdit::SceneRename { scene: 0, name: "Intro".into() },
        WireEdit::SceneInsert { scene: 130, name: "Intro".into() },
        WireEdit::SceneRename { scene: 1, name: "x".repeat(257) },
        WireEdit::SceneRename { scene: 1, name: "bad\nname".into() },
        WireEdit::SlotCreate { track: 1, scene: 129, length: 4. },
        WireEdit::SlotCreate { track: 1, scene: 1, length: f64::NAN },
        WireEdit::SlotCopy { source: 0, track: 1, scene: 1 },
        WireEdit::SlotMove { track: 1, scene: 1, to_track: 2, to_scene: 0 },
    ] {
        assert!(edit.edit().is_err());
    }
}

#[test]
fn launcher_quota_refusal_preserves_revision_sequence_and_model() {
    let mut p = crate::starter_project();
    let template = p.launcher.scenes[0].clone();
    while p.launcher.scenes.len() < 128 {
        let mut scene = template.clone();
        scene.id = p.new_scene_id().unwrap();
        p.launcher.scenes.push(scene);
    }
    for c in &mut p.launcher.columns {
        c.slots.resize_with(128, || suhina_model::Slot { clip: None, has_stop: true });
    }
    let mut state = canonical(p.clone());
    for edit in [WireEdit::SceneInsert { scene: 129, name: "overflow".into() }, WireEdit::SceneDuplicate { scene: 1 }] {
        assert!(state.submit(Host::owner(), 1, 0, edit).is_err());
        assert_eq!(state.host.project(), &p);
        assert_eq!(state.host.revision(), 0);
        assert!(state.journal.is_empty());
    }
    state.submit(Host::owner(), 1, 0, WireEdit::SceneRename { scene: 128, name: "Still works".into() }).unwrap();
    assert_eq!(state.host.revision(), 1);
}

#[test]
fn slot_content_quota_and_occupied_target_refuse_without_consuming_sequence() {
    let mut p = crate::starter_project();
    let source = p.arrangement.lanes[0].clips[0].id.0;
    let template = p.contents[0].clone();
    while p.contents.len() < 512 {
        let mut c = template.clone();
        c.id = suhina_model::ContentId(2000 + p.contents.len() as u32);
        p.contents.push(c);
    }
    let mut state = canonical(p.clone());
    assert!(state.submit(Host::owner(), 1, 0, WireEdit::SlotCopy { source, track: 1, scene: 1 }).is_err());
    assert_eq!(state.host.project(), &p);
    assert_eq!(state.host.revision(), 0);
    state.submit(Host::owner(), 1, 0, WireEdit::SceneRename { scene: 1, name: "Retry".into() }).unwrap();
    let mut state = canonical(crate::starter_project());
    state.submit(Host::owner(), 1, 0, WireEdit::SlotCreate { track: 1, scene: 1, length: 4. }).unwrap();
    let before = state.host.project().clone();
    assert!(state.submit(Host::owner(), 2, 1, WireEdit::SlotCopy { source, track: 1, scene: 1 }).is_err());
    assert_eq!(state.host.project(), &before);
    assert_eq!(state.host.revision(), 1);
    state.submit(Host::owner(), 2, 1, WireEdit::SceneRename { scene: 1, name: "Retry after occupied".into() }).unwrap();
}
