// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions and setup in test-only helpers.

use serde_json::json;
use std::time::{Duration, Instant};
use suhina_engine::{AudioPool, Renderer};
use suhina_multiplayer::{Operation, Service, WireEdit};
use suhina_session::{Player, Session};

struct Member {
    service: Service,
    session: Session,
    renderer: Renderer,
    revision: u64,
    guests: u64,
    invitation: String,
    error: Option<String>,
}
impl Member {
    fn open(mut service: Service) -> Self {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let state = service.execute("multiplayer.state", &json!({})).unwrap();
            assert_ne!(state["mode"], "error", "{}", state["error"]);
            if matches!(state["mode"].as_str(), Some("hosting" | "connected")) {
                break;
            }
            assert!(Instant::now() < until, "member startup timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        let snapshot = service.take_shared().unwrap();
        let revision = snapshot.revision;
        let mut session = snapshot.session().unwrap();
        snapshot.keep();
        let mut renderer = Renderer::stopped(session.project(), session.audio(), 8000).unwrap();
        session.attach_player(Player::new(renderer.controller(), None));
        Self { service, session, renderer, revision, guests: 0, invitation: String::new(), error: None }
    }
    fn poll(&mut self) {
        let value = self.service.execute("multiplayer.state", &json!({})).unwrap();
        assert_ne!(value["mode"], "error", "{}", value["error"]);
        let operations: Vec<Operation> = serde_json::from_value(value["operations"].clone()).unwrap();
        for audio in self.service.take_audio() {
            let folder = audio.folder().unwrap();
            self.session.add_collected_audio(&audio.audio, folder).unwrap();
            audio.keep();
        }
        for op in operations {
            assert_eq!(op.revision, self.revision + 1);
            let (command, params) = op.edit.command();
            self.session.execute(command, &params).unwrap();
            self.revision = op.revision;
        }
        assert_eq!(value["controls"]["revision"], self.revision);
        self.guests = value["guests_connected"].as_u64().unwrap();
        self.invitation = value["invitation"].as_str().unwrap_or_default().into();
        self.error = value["error"].as_str().map(str::to_owned);
    }
    fn edit(&mut self, base: u64, edit: WireEdit) {
        self.service
            .execute("multiplayer.edit", &json!({"base_revision":base,"edit":serde_json::to_string(&edit).unwrap()}))
            .unwrap();
    }
}
struct Band {
    members: Vec<Member>,
    root: tempfile::TempDir,
}
impl Band {
    fn new(audio: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::default();
        service.set_storage_root(Some(root.path().join("Host"))).unwrap();
        let mut project = suhina_multiplayer::starter_project();
        let mut point = suhina_model::AutomationPoint::new(0., 0.1);
        point.curve = suhina_model::automation::Curve::new(0.1, 0.8, 0.7, 0.4).unwrap();
        project.channels[0].automation.push(suhina_model::AutomationLane {
            param: "volume".into(),
            enabled: true,
            points: vec![point, suhina_model::AutomationPoint::new(4., 0.2)],
        });
        if audio {
            project.tracks[3].content = suhina_model::ContentKinds { notes: false, audio: true };
            project.contents[3].lanes[0].data = suhina_model::LaneData::Audio(suhina_model::AudioRegion::new(""));
            service.select_audio_project(&project, &AudioPool::default()).unwrap();
        } else {
            service.select_project(&project).unwrap();
        }
        service.execute("multiplayer.hostProject", &json!({"address":"127.0.0.1:0"})).unwrap();
        let mut band = Self { members: vec![Member::open(service)], root };
        band.members[0].poll();
        band
    }
    fn wait(&mut self, ready: impl Fn(&Self) -> bool) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            for member in &mut self.members {
                member.poll();
            }
            if ready(self) {
                return;
            }
            assert!(Instant::now() < until, "band convergence timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn counts(&mut self, guests: u64) {
        self.wait(|b| b.members.iter().filter(|m| m.service.active()).all(|m| m.guests == guests));
    }
    fn invite(&mut self) -> String {
        let old = self.members[0].invitation.clone();
        self.members[0].service.execute("multiplayer.renewInvitation", &json!({})).unwrap();
        self.wait(|b| b.members[0].invitation != old);
        self.members[0].invitation.clone()
    }
    fn join(&mut self, invitation: String) {
        let mut service = Service::default();
        service.set_storage_root(Some(self.root.path().join(format!("Guest{}", self.members.len())))).unwrap();
        service.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
        self.members.push(Member::open(service));
        let guests = self.members.iter().skip(1).filter(|m| m.service.active()).count() as u64;
        self.counts(guests);
        self.equal();
    }
    fn revision(&mut self, revision: u64) {
        self.wait(|b| b.members.iter().filter(|m| m.service.active()).all(|m| m.revision == revision));
        self.equal();
    }
    fn equal(&self) {
        for member in self.members.iter().filter(|m| m.service.active()) {
            assert_eq!(self.members[0].session.project(), member.session.project());
        }
    }
}

fn refused(invitation: String) {
    let mut service = Service::default();
    service.execute("multiplayer.join", &json!({"invitation":invitation})).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    while service.active() && Instant::now() < until {
        service.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!service.active());
    assert!(service.take_shared().is_none());
}

#[test]
fn four_musicians_keep_independent_sequences_players_and_copies_when_a_guest_leaves_and_returns() {
    let mut b = Band::new(false);
    let original = b.members[0].session.project().clone();
    let first = b.members[0].invitation.clone();
    b.join(first.clone());
    let replaced = b.invite();
    let second = b.invite();
    assert!(replaced != second);
    refused(replaced);
    b.counts(1);
    b.join(second);
    let third = b.invite();
    assert!(third != first);
    b.join(third);
    refused(first);
    b.counts(3);
    assert_eq!(b.members.len(), 4);
    assert!(b.members[0].service.execute("multiplayer.renewInvitation", &json!({})).is_err());
    assert_eq!(b.members[0].session.project(), &original);
    for i in 0..4 {
        b.members[i].session.execute("transport.setPosition", &json!({"beats":i * 8})).unwrap();
        b.members[i].session.execute("transport.play", &json!({})).unwrap();
        b.members[i].edit(i as u64, WireEdit::Rename { track: i as u32 + 1, name: format!("Musician {i}") });
        b.revision(i as u64 + 1);
    }
    let mut out = [0.; 512];
    for m in &mut b.members {
        m.renderer.process(&mut out);
        assert!(m.renderer.monitor().running());
    }
    for pair in b.members.windows(2) {
        assert!(pair[1].renderer.monitor().beat() - pair[0].renderer.monitor().beat() > 7.);
    }
    b.members[1].session.execute("transport.stop", &json!({})).unwrap();
    for m in &mut b.members {
        m.renderer.process(&mut out);
    }
    assert!(!b.members[1].renderer.monitor().running());
    assert!(b.members[2].renderer.monitor().running());
    let departed = b.members[2].session.project().clone();
    b.members[2].service.execute("multiplayer.stop", &json!({})).unwrap();
    b.counts(2);
    b.members[3].edit(4, WireEdit::SceneRename { scene: 1, name: "Still together".into() });
    b.revision(5);
    assert_eq!(b.members[2].session.project(), &departed);
    let invite = b.invite();
    b.join(invite);
    assert_eq!(b.members[4].revision, 5);
    b.members[1].edit(5, WireEdit::SceneRename { scene: 2, name: "Old sequence kept".into() });
    b.revision(6);
    b.members[4].edit(6, WireEdit::SceneRename { scene: 3, name: "New peer sequence".into() });
    b.revision(7);
    b.members[0].service.execute("multiplayer.stop", &json!({})).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while b.members.iter().any(|m| m.service.active()) && Instant::now() < until {
        for m in &mut b.members {
            m.service.poll();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(b.members.iter().all(|m| !m.service.active()));
    for m in &b.members {
        assert!(m.session.project().validate().is_ok());
    }
}

#[test]
fn competing_guest_edits_reject_one_draft_then_accept_its_refreshed_sequence() {
    let mut b = Band::new(false);
    let first = b.members[0].invitation.clone();
    b.join(first);
    let invite = b.invite();
    b.join(invite);
    b.members[1].edit(0, WireEdit::Rename { track: 1, name: "First draft".into() });
    b.members[2].edit(0, WireEdit::Rename { track: 2, name: "Other draft".into() });
    b.revision(1);
    b.wait(|b| b.members.iter().skip(1).filter(|m| m.error.is_some()).count() == 1);
    let loser = if b.members[1].error.is_some() { 1 } else { 2 };
    b.members[loser].edit(1, WireEdit::SceneRename { scene: 1, name: "Refreshed".into() });
    b.revision(2);
    b.members[0].edit(2, WireEdit::SceneRename { scene: 2, name: "Host follows".into() });
    b.revision(3);
    assert_eq!(b.members[loser].error, None);
}

#[test]
fn several_guests_publish_verified_takes_and_retry_competing_uploads_without_losing_sources() {
    use std::sync::Arc;
    use suhina_engine::{AudioData, RenderInputs};
    let mut b = Band::new(true);
    let first = b.members[0].invitation.clone();
    b.join(first);
    for _ in 0..2 {
        let invite = b.invite();
        b.join(invite);
    }
    let takes: Vec<Arc<AudioData>> = (1..=3)
        .map(|n| {
            Arc::new(AudioData {
                sample_rate: 8000,
                channels: 1,
                samples: (0..8000).map(|i| ((i * n) as f32 * 0.12).sin() * 0.2).collect(),
            })
        })
        .collect();
    for i in 1..=2 {
        b.members[i].service.select_take(Arc::clone(&takes[i - 1])).unwrap();
        b.members[i]
            .service
            .execute("multiplayer.publishTake", &json!({"base_revision":0,"track":4,"start":i * 8}))
            .unwrap();
    }
    b.revision(1);
    b.wait(|b| b.members.iter().skip(1).filter(|m| m.error.is_some()).count() == 1);
    let loser = if b.members[1].error.is_some() { 1 } else { 2 };
    assert_eq!(takes[loser - 1].samples.len(), 8000);
    b.members[loser].service.select_take(Arc::clone(&takes[loser - 1])).unwrap();
    b.members[loser]
        .service
        .execute("multiplayer.publishTake", &json!({"base_revision":1,"track":4,"start":loser * 8}))
        .unwrap();
    b.revision(2);
    b.members[3].service.select_take(Arc::clone(&takes[2])).unwrap();
    b.members[3].service.execute("multiplayer.publishTake", &json!({"base_revision":2,"track":4,"start":24})).unwrap();
    b.revision(3);
    for m in &b.members {
        assert_eq!(m.session.audio().files.len(), 3);
    }
    b.members[3].service.select_take(Arc::clone(&takes[0])).unwrap();
    b.members[3].service.execute("multiplayer.publishTake", &json!({"base_revision":3,"track":4,"start":32})).unwrap();
    b.revision(4);
    let render = |m: &Member| {
        suhina_engine::render_offline_from(
            m.session.project(),
            RenderInputs::from_audio(m.session.audio()),
            8000,
            512,
            None,
        )
        .unwrap()
    };
    let expected = render(&b.members[0]);
    assert!(expected.iter().any(|s| s.abs() > 0.001));
    for m in &b.members {
        assert_eq!(m.session.audio().files.len(), 3);
        assert_eq!(render(m), expected);
        let saved = suhina_project_file::from_json(&suhina_project_file::to_json(m.session.project())).unwrap();
        assert_eq!(&saved, m.session.project());
    }
    b.members[2].service.execute("multiplayer.stop", &json!({})).unwrap();
    b.counts(2);
    b.members[1].edit(4, WireEdit::SceneRename { scene: 1, name: "Takes retained".into() });
    b.revision(5);
    for m in &b.members {
        assert_eq!(m.session.audio().files.len(), 3);
    }
}
