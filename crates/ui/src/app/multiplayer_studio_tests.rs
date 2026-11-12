// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shared session in the ordinary Studio: studio edits from either computer reach both.

use super::*;

/// Polls both computers until `done` holds, or fails after the slow runner's wait.
fn until(host: &mut App, guest: &mut App, ctx: &egui::Context, done: impl Fn(&App, &App) -> bool) {
    let deadline = Instant::now() + SLOW_RUNNER;
    while !done(host, guest) {
        assert!(
            Instant::now() < deadline,
            "the shared edit did not reach both computers: host {:?} {:?}, guest {:?} {:?}",
            host.status,
            host.multiplayer.error,
            guest.status,
            guest.multiplayer.error
        );
        host.poll_multiplayer(ctx);
        guest.poll_multiplayer(ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn shared(app: &App) -> &suhina_engine::model::Project {
    app.projects.get(app.multiplayer.bound.unwrap()).unwrap().project()
}

#[test]
fn studio_edits_from_either_computer_reach_both_shared_tabs() {
    let ctx = egui::Context::default();
    let mut host = App::new();
    let mut guest = App::new();
    host.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + SLOW_RUNNER;
    while (host.multiplayer.invitation.is_empty() || host.multiplayer.bound.is_none()) && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    until(&mut host, &mut guest, &ctx, |_, g| g.multiplayer.bound.is_some());
    assert_eq!(shared(&host), shared(&guest));
    // The shared tab is the active one on both: the Studio shows it once the page is closed.
    assert!(host.shared_studio_shown() && guest.shared_studio_shown());
    let tracks = shared(&host).tracks.len();
    let ids: Vec<u32> = shared(&host).tracks.iter().map(|t| t.id.0).collect();

    // The guest adds a MIDI track as the Studio's Add track does: an edit no typed control has.
    guest.run(&Action::new("track.add", json!({"kind":"midi"})));
    until(&mut host, &mut guest, &ctx, |h, g| {
        shared(h).tracks.len() == tracks + 1 && shared(g).tracks.len() == tracks + 1
    });
    let added = shared(&host).tracks.iter().map(|t| t.id.0).find(|id| !ids.contains(id)).unwrap();

    // The host draws a clip on it and the guest renames it, without waiting for each other: the
    // second edit was made against a revision its author had not seen yet, and still runs.
    host.run(&Action::new("clip.create", json!({"track": added, "start": 8.0, "length": 4.0})));
    guest.run(&Action::new("track.rename", json!({"track": added, "name": "Together"})));
    until(&mut host, &mut guest, &ctx, |h, g| {
        [h, g].iter().all(|app| {
            let project = shared(app);
            project.tracks.iter().any(|t| t.id.0 == added && t.name == "Together")
                && project
                    .arrangement
                    .lanes
                    .iter()
                    .any(|l| l.track.is_some_and(|t| t.0 == added) && !l.clips.is_empty())
        })
    });
    assert_eq!(shared(&host), shared(&guest));
    assert!(host.multiplayer.outbox.is_empty() && guest.multiplayer.outbox.is_empty());
}

#[test]
fn edits_the_session_does_not_share_are_refused_with_a_sentence() {
    let ctx = egui::Context::default();
    let mut host = App::new();
    host.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + SLOW_RUNNER;
    while host.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let before = shared(&host).clone();
    for (id, params) in [
        ("clip.paste", json!({"track": 1, "start": 0.0})),
        ("edit.undo", json!({})),
        ("track.add", json!({"kind": "midi", "preset": "/somewhere/else.fxp"})),
    ] {
        host.status = Status::Done(String::new());
        host.run(&Action::new(id, params));
        assert!(matches!(&host.status, Status::Hint(text) if !text.is_empty()), "{id}: {:?}", host.status);
        assert!(host.multiplayer.outbox.is_empty(), "{id}");
    }
    host.poll_multiplayer(&ctx);
    assert_eq!(shared(&host), &before);
    host.leave_multiplayer();
}

#[test]
fn the_session_page_opens_the_shared_session_in_the_studio() {
    let ctx = egui::Context::default();
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let original = app.projects.active().unwrap();
    app.multiplayer.open = true;
    app.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + SLOW_RUNNER;
    while app.multiplayer.bound.is_none() && Instant::now() < deadline {
        app.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    app.settings.view.menu_bar = false;
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run_steps(2);
    // Another tab in front: the button brings the shared one back.
    let switch = json!({"project": original.0});
    harness.state_mut().projects.execute("project.switch", &switch).unwrap();
    harness.run_steps(2);
    harness.get_by_label("Open the shared session in Studio").click();
    harness.run_steps(3);
    assert!(harness.state().shared_studio_shown());
    assert!(harness.query_by_label("Session page").is_some());
    harness.get_by_label("Session page").click();
    harness.run_steps(2);
    assert!(harness.state().multiplayer_visible());
    harness.state_mut().leave_multiplayer();
}
