// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn pump(h: &mut egui_kittest::Harness<'_, App>, guests: &mut [App], count: u64) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        h.run_steps(2);
        for guest in guests.iter_mut() {
            guest.poll_multiplayer(&h.ctx);
        }
        if h.state().multiplayer.state["guests_connected"] == count
            && guests
                .iter()
                .filter(|g| g.multiplayer.service.active())
                .all(|g| g.multiplayer.bound.is_some() && g.multiplayer.state["guests_connected"] == count)
        {
            return;
        }
        assert!(Instant::now() < until, "participant counters timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn visible_invitation_button_keeps_three_guests_and_reopens_one_slot_after_departure() {
    buttons(false);
}
#[test]
#[ignore = "manual GPU capture of a host with three guests"]
fn render_three_guests_for_review() {
    buttons(true);
}
fn buttons(render: bool) {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut host = App::new();
    host.set_default_data_folder(root.path().join("Host"));
    host.set_session(Session::new(suhina_multiplayer::starter_project()), &[]);
    let original = host.projects.active().unwrap();
    host.multiplayer.open = true;
    host.multiplayer.online = false;
    host.multiplayer.address = "127.0.0.1:0".into();
    host.host_current_midi();
    let until = Instant::now() + Duration::from_secs(10);
    while host.multiplayer.bound.is_none() && Instant::now() < until {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let bound = host.multiplayer.bound.unwrap();
    let builder = egui_kittest::Harness::builder().with_size(Vec2::new(1400., 1000.));
    let builder = if render { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    theme::install(&h.ctx);
    h.run_steps(3);
    let before = h.state().projects.get(original).unwrap().project().clone();
    let mut guests = Vec::new();
    for n in 0..3 {
        if n > 0 {
            let old = h.state().multiplayer.invitation.clone();
            h.get_by_label("New invitation").click();
            let until = Instant::now() + Duration::from_secs(10);
            while h.state().multiplayer.invitation == old && Instant::now() < until {
                h.run_steps(2);
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(h.state().multiplayer.invitation != old);
        }
        let mut guest = App::new();
        guest.set_default_data_folder(root.path().join(format!("Guest{n}")));
        guest.multiplayer_command("multiplayer.join", json!({"invitation":h.state().multiplayer.invitation}));
        guests.push(guest);
        pump(&mut h, &mut guests, n + 1);
        assert!(h.query_by_label(&format!("Guests: {} / 3", n + 1)).is_some());
        assert_eq!(h.get_by_label("New invitation").accesskit_node().is_disabled(), n == 2);
        assert_eq!(h.state().multiplayer.applied_revision, 0);
    }
    let old_guest = guests[1].multiplayer.bound.unwrap();
    let other_guest = guests[2].multiplayer.bound.unwrap();
    guests[1].leave_multiplayer();
    guests[1].projects.execute_in(Some(old_guest), "track.rename", &json!({"track":1,"name":"Solo retained"})).unwrap();
    let solo = guests[1].projects.get(old_guest).unwrap().project().clone();
    pump(&mut h, &mut guests, 2);
    assert!(!h.get_by_label("New invitation").accesskit_node().is_disabled());
    let old = h.state().multiplayer.invitation.clone();
    h.get_by_label("New invitation").click();
    let until = Instant::now() + Duration::from_secs(10);
    while h.state().multiplayer.invitation == old && Instant::now() < until {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.state().multiplayer.invitation != old);
    guests[1].multiplayer_command("multiplayer.join", json!({"invitation":h.state().multiplayer.invitation}));
    pump(&mut h, &mut guests, 3);
    assert_ne!(guests[1].multiplayer.bound.unwrap(), old_guest);
    assert_eq!(guests[2].multiplayer.bound, Some(other_guest));
    assert_eq!(guests[1].projects.get(old_guest).unwrap().project(), &solo);
    assert_eq!(h.state().multiplayer.bound, Some(bound));
    guests[2].multiplayer_command("multiplayer.edit",json!({"base_revision":0,"edit":json!({"command":"scene.rename","scene":1,"name":"Four musicians"}).to_string()}));
    let until = Instant::now() + Duration::from_secs(10);
    while (h.state().multiplayer.applied_revision < 1 || guests.iter().any(|g| g.multiplayer.applied_revision < 1))
        && Instant::now() < until
    {
        h.run_steps(2);
        for guest in &mut guests {
            guest.poll_multiplayer(&h.ctx);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().multiplayer.applied_revision, 1);
    for g in &guests {
        assert_eq!(g.multiplayer.applied_revision, 1);
        assert_eq!(
            h.state().projects.get(bound).unwrap().project(),
            g.projects.get(g.multiplayer.bound.unwrap()).unwrap().project()
        );
    }
    h.get_by_label("Launcher").click();
    h.run_steps(3);
    assert!(h.query_by_label("Scene 1 · Four musicians").is_some());
    assert!(h.query_by_label("Guests: 3 / 3").is_some());
    assert_eq!(h.state().projects.get(original).unwrap().project(), &before);
    let copies: Vec<_> = guests.iter().map(|g| g.multiplayer.bound.unwrap()).collect();
    if render {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        std::fs::create_dir_all(&folder).unwrap();
        h.render().unwrap().save(folder.join("multiplayer-three-guests.png")).unwrap();
    }
    h.get_by_label("Leave session").click();
    let until = Instant::now() + Duration::from_secs(10);
    while guests.iter().any(|g| g.multiplayer.bound.is_some()) && Instant::now() < until {
        h.run_steps(2);
        for g in &mut guests {
            g.poll_multiplayer(&h.ctx);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(guests.iter().all(|g| g.multiplayer.bound.is_none()));
    assert!(h.state().projects.get(bound).is_some());
    for (g, id) in guests.iter().zip(copies) {
        assert!(g.projects.get(id).is_some());
    }
}
