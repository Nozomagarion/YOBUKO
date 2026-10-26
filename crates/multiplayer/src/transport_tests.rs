// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn expiration_wrong_secrets_and_reuse_do_not_admit_a_peer() {
    let now = Instant::now();
    let secret = "a".repeat(64);
    let mut host = Canonical {
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
    for size in [0u32, 8193, u32::MAX] {
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
    assert!(WireEdit::Rename { track: 5, name: "not shared".into() }.edit().is_err());
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
        &ClientMessage::Hello { protocol: 2, starter: 1, secret: invitation.secret.clone() },
    )
    .unwrap();
    assert!(matches!(protocol::read::<ServerMessage>(&mut incompatible).unwrap(), ServerMessage::Denied));
    let mut io = connect(&invitation);
    protocol::write(
        &mut io,
        &ClientMessage::Hello { protocol: PROTOCOL, starter: 1, secret: invitation.secret.clone() },
    )
    .unwrap();
    assert!(matches!(protocol::read::<ServerMessage>(&mut io).unwrap(), ServerMessage::State { .. }));
    let edit = ClientMessage::Edit {
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
    protocol::write(&mut io, &ClientMessage::Poll { protocol: 2 }).unwrap();
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
