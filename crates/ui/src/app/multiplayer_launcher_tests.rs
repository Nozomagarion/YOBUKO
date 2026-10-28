// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn sync(h: &mut egui_kittest::Harness<'_, App>, guest: &mut App, ctx: &egui::Context, revision: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while (h.state().multiplayer.applied_revision < revision || guest.multiplayer.applied_revision < revision)
        && Instant::now() < deadline
    {
        h.run_steps(2);
        guest.poll_multiplayer(ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().multiplayer.applied_revision, revision, "{:?}", h.state().multiplayer.error);
    assert_eq!(guest.multiplayer.applied_revision, revision, "{:?}", guest.multiplayer.error);
    let local = h.state().multiplayer.bound.unwrap();
    let remote = guest.multiplayer.bound.unwrap();
    assert_eq!(h.state().projects.get(local).unwrap().project(), guest.projects.get(remote).unwrap().project());
}
fn remote(guest: &mut App, base: u64, edit: serde_json::Value) {
    guest.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit.to_string()}));
}
#[test]
fn launcher_buttons_copy_create_scenes_and_discard_hidden_or_stale_indices() {
    buttons(false);
}
#[test]
#[ignore = "manual GPU capture of shared launcher editing"]
fn render_shared_launcher_for_review() {
    buttons(true);
}
fn buttons(render: bool) {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut host = App::new();
    host.set_default_data_folder(root.path().join("Host"));
    host.set_session(Session::new(suhina_multiplayer::starter_project()), &[]);
    let original = host.projects.active().unwrap();
    let source = host.projects.get(original).unwrap().project().arrangement.lanes[0].clips[0].id.0;
    host.multiplayer.open = true;
    host.multiplayer.address = "127.0.0.1:0".into();
    host.host_current_midi();
    let deadline = Instant::now() + Duration::from_secs(10);
    while host.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let local = host.multiplayer.bound.unwrap();
    let mut guest = App::new();
    guest.set_default_data_folder(root.path().join("Guest"));
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(guest.multiplayer.bound.is_some());
    let builder = egui_kittest::Harness::builder().with_size(Vec2::new(1400., 1150.));
    let builder = if render { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    theme::install(&h.ctx);
    h.run_steps(3);
    let original_music = h.state().projects.get(original).unwrap().project().clone();
    h.get_by_label("Launcher").click();
    h.run_steps(3);
    h.get_by_label("Slot 1:1 · Empty").click();
    h.run_steps(2);
    h.get_by_label("Source clip").click();
    h.run_steps(2);
    h.get_by_label(&format!("Clip #{source} · Pulse · arrangement")).click();
    h.run_steps(2);
    h.get_by_label("Copy clip into shared slot").click();
    sync(&mut h, &mut guest, &ctx, 1);
    let copied =
        h.state().projects.get(local).unwrap().project().launcher.columns[0].slots[0].clip.as_ref().unwrap().id.0;
    assert_eq!(h.state().projects.get(local).unwrap().project().arrangement, original_music.arrangement);
    h.get_by_label("Scene 1 · 1").click();
    h.run_steps(2);
    h.get_by_label("Scene name").focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Scene name").type_text("Intro live");
    h.run_steps(2);
    h.get_by_label("Rename shared scene").click();
    sync(&mut h, &mut guest, &ctx, 2);
    h.get_by_label("Scene 1 · Intro live").click();
    h.run_steps(2);
    h.get_by_label("Duplicate shared scene").click();
    sync(&mut h, &mut guest, &ctx, 3);
    assert!(h.state().multiplayer.launcher.selected.is_none());
    assert!(h.state().multiplayer.launcher.scene.is_none());
    h.get_by_label(&format!("Slot 1:1 · Clip #{copied}")).click();
    h.run_steps(2);
    h.get_by_label("Mixer").click();
    h.run_steps(2);
    remote(&mut guest, 3, json!({"command":"scene.insert","scene":1,"name":"Opening"}));
    sync(&mut h, &mut guest, &ctx, 4);
    assert!(h.state().multiplayer.launcher.selected.is_none());
    assert!(h.state().multiplayer.launcher.scene.is_none());
    h.get_by_label("Launcher").click();
    h.run_steps(2);
    assert!(h.query_by_label("Clear shared slot").is_none());
    h.get_by_label("Slot 2:1 · Empty").click();
    h.run_steps(2);
    h.get_by_label("Create empty shared slot").click();
    sync(&mut h, &mut guest, &ctx, 5);
    let empty =
        h.state().projects.get(local).unwrap().project().launcher.columns[1].slots[0].clip.as_ref().unwrap().id.0;
    h.get_by_label(&format!("Slot 2:1 · Clip #{empty}")).click();
    h.run_steps(2);
    remote(&mut guest, 5, json!({"command":"track.rename","track":2,"name":"Renamed bass"}));
    sync(&mut h, &mut guest, &ctx, 6);
    assert!(h.get_by_label("Clear shared slot").accesskit_node().is_disabled());
    assert!(h.get_by_label("Move or swap shared slot").accesskit_node().is_disabled());
    h.get_by_label("Refresh launcher draft").click();
    h.run_steps(2);
    assert!(h.state().multiplayer.launcher.selected.is_none());
    h.get_by_label(&format!("Slot 2:1 · Clip #{empty}")).click();
    h.run_steps(2);
    h.get_by_label("Clear shared slot").click();
    sync(&mut h, &mut guest, &ctx, 7);
    h.get_by_label("Scene 1 · Opening").click();
    h.run_steps(2);
    h.get_by_label("Delete shared scene").click();
    sync(&mut h, &mut guest, &ctx, 8);
    assert!(h.state().multiplayer.launcher.scene.is_none());
    h.get_by_label("Scene 1 · Intro live").click();
    h.run_steps(2);
    h.get_by_label("Insert scene before").click();
    sync(&mut h, &mut guest, &ctx, 9);
    h.get_by_label("Append shared scene").click();
    sync(&mut h, &mut guest, &ctx, 10);
    h.run_steps(3);
    assert_eq!(h.state().projects.get(original).unwrap().project(), &original_music);
    let mut a = suhina_engine::Renderer::stopped(
        h.state().projects.get(local).unwrap().project(),
        &suhina_engine::AudioPool::default(),
        8000,
    )
    .unwrap();
    let remote_id = guest.multiplayer.bound.unwrap();
    let mut b = suhina_engine::Renderer::stopped(
        guest.projects.get(remote_id).unwrap().project(),
        &suhina_engine::AudioPool::default(),
        8000,
    )
    .unwrap();
    h.state_mut().projects.get_mut(local).unwrap().attach_player(suhina_session::Player::new(a.controller(), None));
    guest.projects.get_mut(remote_id).unwrap().attach_player(suhina_session::Player::new(b.controller(), None));
    h.state_mut().projects.get_mut(local).unwrap().execute("transport.setPosition", &json!({"beats":4})).unwrap();
    guest.projects.get_mut(remote_id).unwrap().execute("transport.setPosition", &json!({"beats":20})).unwrap();
    guest.projects.get_mut(remote_id).unwrap().execute("transport.play", &json!({})).unwrap();
    h.state_mut()
        .projects
        .get_mut(local)
        .unwrap()
        .execute("launcher.setQuantization", &json!({"value":"none"}))
        .unwrap();
    h.get_by_label(&format!("Slot 1:2 · Clip #{copied}")).click();
    h.run_steps(2);
    h.get_by_label("Launch slot locally").click();
    h.run_steps(2);
    let mut out = [0.; 512];
    a.process(&mut out);
    b.process(&mut out);
    assert_eq!(a.monitor().column(0).unwrap().playing, Some(1));
    assert_eq!(b.monitor().column(0).unwrap().playing, None);
    assert!((a.monitor().beat() - b.monitor().beat()).abs() > 10.);
    h.get_by_label("Launch scene locally").click();
    h.run_steps(2);
    a.process(&mut out);
    assert_eq!(h.state().multiplayer.applied_revision, 10);
    h.get_by_label("Stop locally").click();
    h.run_steps(2);
    a.process(&mut out);
    b.process(&mut out);
    assert!(!a.monitor().running());
    assert!(b.monitor().running());
    if render {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        std::fs::create_dir_all(&dir).unwrap();
        h.render().unwrap().save(dir.join("multiplayer-launcher.png")).unwrap();
    }
    h.state_mut().leave_multiplayer();
    guest.leave_multiplayer();
}
