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
    let error = OnlineInvitation::decode(&old.encode().unwrap()).unwrap_err();
    assert!(error.starts_with("This invitation comes from a different Suhina version (protocol 17"), "{error}");
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

/// A loopback host endpoint with an open lobby, and the short code once the lobby is reachable.
fn lobby(invitation: &str) -> (Net, Lobby, String) {
    let host = Net::bind(Reach::Loopback).unwrap();
    let (send, ready) = mpsc::channel();
    let lobby = host.open_lobby(invitation.to_owned(), move |code| send.send(code).unwrap());
    let code = ready.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
    (host, lobby, code)
}
fn invitation_text(net: &Net) -> String {
    OnlineInvitation { protocol: PROTOCOL, endpoint: net.address(), secret: "b".repeat(64) }.encode().unwrap()
}

#[test]
fn a_lobby_hands_the_invitation_to_every_guest_with_the_code() {
    let main = Net::bind(Reach::Loopback).unwrap();
    let text = invitation_text(&main);
    let (_host, _lobby, code) = lobby(&text);
    let parsed = ShortCode::parse(&code).unwrap();
    assert!(parsed.is_local(), "a loopback lobby gives a code for this computer: {code}");
    let (_guest, invitation) = redeem(&parsed).unwrap();
    assert_eq!(invitation.encode().unwrap(), text);
    // The whole group uses the same code.
    let (_friend, again) = redeem(&parsed).unwrap();
    assert_eq!(again.encode().unwrap(), text);
}

#[test]
fn a_wrong_proof_is_refused() {
    let main = Net::bind(Reach::Loopback).unwrap();
    let (_host, _lobby, code) = lobby(&invitation_text(&main));
    let parsed = ShortCode::parse(&code).unwrap();
    let keys = parsed.keys().unwrap();
    let guest = Net::bind(Reach::Loopback).unwrap();
    let mut forged = keys.proof;
    forged[0] ^= 1;
    let refused = guest.runtime.block_on(ask_lobby(&guest.endpoint, keys.key.public(), &forged, false));
    assert!(refused.unwrap_err().contains("refused"));
    assert!(redeem(&parsed).is_ok(), "the right code still works");
}

#[test]
fn an_unknown_or_withdrawn_code_finds_no_session() {
    let main = Net::bind(Reach::Loopback).unwrap();
    let (host, lobby, code) = lobby(&invitation_text(&main));
    let mut other = ShortCode::local().unwrap();
    while other.text().unwrap() == code {
        other = ShortCode::local().unwrap();
    }
    let started = Instant::now();
    assert!(redeem(&other).unwrap_err().contains("no session"));
    assert!(started.elapsed() < Duration::from_secs(10), "a local miss fails fast");
    drop(lobby);
    assert!(redeem(&ShortCode::parse(&code).unwrap()).unwrap_err().contains("no session"));
    // A lobby dropped before it is reachable never reports a code.
    let (send, ready) = mpsc::channel();
    drop(host.open_lobby(invitation_text(&main), move |code| {
        let _ = send.send(code);
    }));
    assert!(ready.recv_timeout(Duration::from_secs(3)).is_err());
}
