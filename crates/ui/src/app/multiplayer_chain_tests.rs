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
fn choose(h: &mut egui_kittest::Harness<'_, App>, label: &str) {
    h.get_by_label("Shared device").click();
    h.run_steps(2);
    h.get_by_label(label).click();
    h.run_steps(2);
}
fn remote(guest: &mut App, base: u64, edit: serde_json::Value) {
    guest.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit.to_string()}));
}
#[test]
fn visible_chain_buttons_keep_sources_and_discard_old_device_drafts_on_hidden_pages_and_index_reuse() {
    buttons(false);
}
#[test]
#[ignore = "manual GPU capture of shared chain editing"]
fn render_shared_device_chains_for_review() {
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
    let builder = egui_kittest::Harness::builder().with_size(Vec2::new(1280., 1000.));
    let builder = if render { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    theme::install(&h.ctx);
    h.run_steps(3);
    let original_music = h.state().projects.get(original).unwrap().project().clone();
    h.get_by_label("Devices").click();
    h.run_steps(3);
    assert!(h.query_by_label("Add Utility").is_some());
    assert!(h.query_by_label("Remove selected device").is_none());
    h.get_by_label("Add Utility").click();
    sync(&mut h, &mut guest, &ctx, 1);
    assert!(h.state().multiplayer.devices.target.is_none());
    assert!(h.query_by_label("Apply shared parameter").is_none());
    assert!(h.get_by_label("Add Utility").accesskit_node().is_disabled());
    choose(&mut h, "Pulse · Utility · device 1");
    h.state_mut().multiplayer.devices.value = -12.;
    h.get_by_label("Apply shared parameter").click();
    sync(&mut h, &mut guest, &ctx, 2);
    h.get_by_label("Refresh device draft").click();
    h.run_steps(2);
    h.state_mut().multiplayer.devices.value = -3.;
    h.get_by_label("Mixer").click();
    h.run_steps(2);
    remote(&mut guest, 2, json!({"command":"device.addShared","track":1,"kind":"fm-synth"}));
    sync(&mut h, &mut guest, &ctx, 3);
    h.get_by_label("Devices").click();
    h.run_steps(2);
    assert!(h.state().multiplayer.devices.target.is_none(), "hidden device drafts must also be discarded");
    assert!(h.query_by_label("Refresh device draft").is_none());
    assert!(h.query_by_label("Remove selected device").is_none());
    h.get_by_label("Refresh chain draft").click();
    h.run_steps(2);
    assert!(h.get_by_label("Add FM Synth").accesskit_node().is_disabled());
    assert!(!h.get_by_label("Add Utility").accesskit_node().is_disabled());
    if render {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        std::fs::create_dir_all(&folder).unwrap();
        h.render().unwrap().save(folder.join("multiplayer-chain-reselect.png")).unwrap();
    }
    choose(&mut h, "Pulse · Utility · device 2");
    assert_eq!(h.state().multiplayer.devices.value, -12.);
    choose(&mut h, "Pulse · FM Synth · device 1");
    if render {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        h.render().unwrap().save(folder.join("multiplayer-chain-editor.png")).unwrap();
    }
    h.get_by_label("Remove selected device").click();
    sync(&mut h, &mut guest, &ctx, 4);
    assert!(h.state().multiplayer.devices.target.is_none());
    choose(&mut h, "Pulse · Utility · device 1");
    h.state_mut().multiplayer.devices.value = -3.;
    remote(&mut guest, 4, json!({"command":"device.addShared","track":1,"kind":"utility"}));
    sync(&mut h, &mut guest, &ctx, 5);
    assert!(h.state().multiplayer.devices.target.is_none(), "even unchanged indices require a new explicit selection");
    choose(&mut h, "Pulse · Utility · device 2");
    assert_eq!(h.state().multiplayer.devices.value, 0.);
    h.get_by_label("Remove selected device").click();
    sync(&mut h, &mut guest, &ctx, 6);
    remote(&mut guest, 6, json!({"command":"device.removeShared","track":1,"device":0}));
    sync(&mut h, &mut guest, &ctx, 7);
    assert!(h.query_by_label("Add Utility").is_some());
    assert!(h.query_by_label("Shared device").is_none());
    remote(&mut guest, 7, json!({"command":"device.addShared","track":1,"kind":"utility"}));
    sync(&mut h, &mut guest, &ctx, 8);
    assert!(h.state().multiplayer.devices.target.is_none(), "a new device at an old index must not inherit its draft");
    choose(&mut h, "Pulse · Utility · device 1");
    assert_eq!(h.state().multiplayer.devices.value, 0.);
    h.get_by_label("Refresh chain draft").click();
    h.run_steps(2);
    h.get_by_label("Add FM Synth").click();
    sync(&mut h, &mut guest, &ctx, 9);
    choose(&mut h, "Pulse · FM Synth · device 1");
    h.get_by_label("Remove selected device").click();
    sync(&mut h, &mut guest, &ctx, 10);
    choose(&mut h, "Pulse · Utility · device 1");
    h.get_by_label("Remove selected device").click();
    sync(&mut h, &mut guest, &ctx, 11);
    assert!(h.query_by_label("Add FM Synth").is_some());
    assert_eq!(h.state().multiplayer.bound, Some(local));
    assert_eq!(h.state().projects.get(original).unwrap().project(), &original_music);
    let shared = h.state().projects.get(local).unwrap().project();
    assert!(shared.channels[0].devices.is_empty());
    assert_eq!(shared.contents, original_music.contents);
    h.state_mut().leave_multiplayer();
    guest.leave_multiplayer();
}
