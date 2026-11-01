// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn pump_until(h: &mut egui_kittest::Harness<'_, App>, guest: &mut App, what: &str, done: impl Fn(&App, &App) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done(h.state(), guest) {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        h.run_steps(2);
        guest.poll_multiplayer(&h.ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_link_toggle_sends_play_and_stop_to_every_participant() {
    let mut host = App::new();
    host.multiplayer.open = true;
    host.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.multiplayer.invitation.is_empty() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut guest = App::new();
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    let mut h = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    pump_until(&mut h, &mut guest, "the guest", |host, guest| {
        host.multiplayer.bound.is_some() && guest.multiplayer.bound.is_some()
    });
    assert!(h.query_by_label("Play locally").is_some(), "unlinked playback stays local");
    assert!(h.query_by_label("Play together").is_none());

    h.get_by_label("Link playback").click();
    pump_until(&mut h, &mut guest, "linking", |host, guest| {
        host.multiplayer.linked_applied == 1 && guest.multiplayer.linked_applied == 1
    });
    assert_eq!(guest.multiplayer.state["transport"]["playing"], false);
    h.run_steps(2);
    assert!(h.query_by_label("Play locally").is_none());

    h.get_by_label("Play together").click();
    pump_until(&mut h, &mut guest, "a shared start", |host, guest| {
        host.multiplayer.linked_applied == 2 && guest.multiplayer.linked_applied == 2
    });
    let host_state = &h.state().multiplayer.state;
    assert_eq!(host_state["transport"]["playing"], true, "Play went through the shared transport");
    assert_eq!(host_state["transport"], guest.multiplayer.state["transport"]);

    h.get_by_label("Everyone from beat 16").click();
    pump_until(&mut h, &mut guest, "a shared move", |host, guest| {
        host.multiplayer.linked_applied == 3 && guest.multiplayer.linked_applied == 3
    });
    assert_eq!(guest.multiplayer.state["transport"]["beat"], 16.0);

    h.get_by_label("Stop together").click();
    pump_until(&mut h, &mut guest, "a shared stop", |host, guest| {
        host.multiplayer.linked_applied == 4 && guest.multiplayer.linked_applied == 4
    });
    assert_eq!(guest.multiplayer.state["transport"]["playing"], false);

    h.get_by_label("Link playback").click();
    pump_until(&mut h, &mut guest, "unlinking", |host, guest| {
        host.multiplayer.state["linked"] == false && guest.multiplayer.state["linked"] == false
    });
    h.run_steps(2);
    assert!(h.query_by_label("Play locally").is_some(), "unlinked again, playback is local as before");
    h.state_mut().leave_multiplayer();
    guest.leave_multiplayer();
}
