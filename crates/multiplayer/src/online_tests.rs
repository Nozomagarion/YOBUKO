// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn invitations_round_trip_and_reject_tampering() {
    let net = Net::bind(Reach::Loopback).unwrap();
    let invitation = OnlineInvitation { protocol: PROTOCOL, endpoint: net.address(), secret: "a".repeat(64) };
    let text = invitation.encode().unwrap();
    assert!(OnlineInvitation::is_online(&text));
    let decoded = OnlineInvitation::decode(&text).unwrap();
    assert_eq!(decoded.endpoint, invitation.endpoint);
    assert!(OnlineInvitation::decode(&text.replace("suhina-net", "suhina-lan")).is_err());
    assert!(OnlineInvitation::decode(&format!("{text}x")).is_err());
    let short = OnlineInvitation { secret: "a".repeat(63), ..decoded };
    assert!(OnlineInvitation::decode(&short.encode().unwrap()).is_err());
    let old = OnlineInvitation { protocol: PROTOCOL - 1, secret: "a".repeat(64), ..short };
    assert!(OnlineInvitation::decode(&old.encode().unwrap()).is_err());
}

#[test]
fn streams_exchange_bytes_and_time_out_without_losing_data() {
    let host = Net::bind(Reach::Loopback).unwrap();
    let incoming = host.listen(1);
    let guest = Net::bind(Reach::Loopback).unwrap();
    let mut out = guest.connect(host.address()).unwrap();
    out.write_all(b"hello").unwrap();
    let mut served = incoming.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut word = [0u8; 5];
    served.read_exact(&mut word).unwrap();
    assert_eq!(&word, b"hello");
    served.set_deadline(Instant::now() + Duration::from_millis(50));
    let error = served.read(&mut word).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    served.renew();
    out.write_all(b"again").unwrap();
    served.read_exact(&mut word).unwrap();
    assert_eq!(&word, b"again");
}

#[test]
fn a_forged_host_key_is_refused() {
    let host = Net::bind(Reach::Loopback).unwrap();
    let _incoming = host.listen(1);
    let other = SecretKey::generate().public();
    let forged = EndpointAddr::from_parts(other, host.address().addrs);
    let guest = Net::bind(Reach::Loopback).unwrap();
    assert!(guest.connect(forged).is_err());
}
