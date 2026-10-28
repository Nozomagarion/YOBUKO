// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers
use super::*;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

fn fixture(dir: &std::path::Path) -> App {
    let tracks: Vec<_> = (1..=3)
        .map(|id| {
            let path = dir.join(format!("{id}.wav"));
            std::fs::write(&path, format!("original audio {id}")).unwrap();
            json!({"id":id,"path":path,"title":format!("Track {id}"),"seconds":180.0,"bpm":120.0,"key":"Am",
            "source_project":null})
        })
        .collect();
    std::fs::write(dir.join("dj-library.json"), serde_json::to_vec(&json!({"format":1,"tracks":tracks})).unwrap())
        .unwrap();
    let mut app = App::new();
    app.set_file_library(suhina_library::Library::new(Some(dir.to_owned())));
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    app.dj.active = true;
    app
}
fn harness(app: App) -> egui_kittest::Harness<'static, App> {
    egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}
fn settle(h: &mut egui_kittest::Harness<'_, App>) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        h.state_mut().dj.next_poll = None;
        h.run_steps(3);
        if h.state().dj.snapshot["busy"] == false {
            h.run_steps(2);
            return;
        }
        assert!(std::time::Instant::now() < until, "DJ worker timed out: {}", h.state().dj.snapshot);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn type_name(h: &mut egui_kittest::Harness<'_, App>, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label(label).type_text(text);
    h.run_steps(2);
}
fn click(h: &mut egui_kittest::Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    settle(h);
}

#[test]
fn playlist_buttons_create_order_rename_remove_and_reopen_without_editing_studio() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(dir.path()));
    settle(&mut h);
    let before = h.state().session().unwrap().project().clone();
    assert!(h.get_by_label("Create playlist").accesskit_node().is_disabled());
    type_name(&mut h, "New playlist name", "Opening set");
    click(&mut h, "Create playlist");
    assert_eq!(h.state().dj.selected_playlist, Some(1));
    for id in 1..=3 {
        click(&mut h, &format!("Add track {id}"));
    }
    assert!(h.get_by_label("Add track 1").accesskit_node().is_disabled());
    assert!(h.get_by_label("Move track 1 up").accesskit_node().is_disabled());
    assert!(h.get_by_label("Move track 3 down").accesskit_node().is_disabled());
    click(&mut h, "Move track 3 up");
    click(&mut h, "Move track 3 up");
    assert_eq!(h.state().dj.snapshot["playlists"][0]["tracks"], json!([3, 1, 2]));
    type_name(&mut h, "Rename playlist", "Late set");
    click(&mut h, "Save playlist name");
    assert_eq!(h.state().dj.snapshot["playlists"][0]["name"], "Late set");
    click(&mut h, "Remove track 2");
    assert_eq!(h.state().session().unwrap().project(), &before);
    drop(h);
    let mut app = App::new();
    app.dj.active = true;
    app.set_file_library(suhina_library::Library::new(Some(dir.path().to_owned())));
    let mut h = harness(app);
    settle(&mut h);
    assert_eq!(h.state().dj.snapshot["playlists"][0]["tracks"], json!([3, 1]));
    assert_eq!(h.state().dj.snapshot["playlists"][0]["name"], "Late set");
    h.get_by_label("DJ collection").click();
    h.run_steps(2);
    h.get_by_label("Late set · #1").click();
    h.run_steps(2);
    click(&mut h, "Delete playlist…");
    assert!(h.query_by_label("Confirm delete playlist").is_some());
    click(&mut h, "Cancel playlist deletion");
    assert!(h.query_by_label("Confirm delete playlist").is_none());
    click(&mut h, "Delete playlist…");
    click(&mut h, "Confirm delete playlist");
    assert_eq!(h.state().dj.snapshot["playlists"], json!([]));
    for id in 1..=3 {
        assert_eq!(
            std::fs::read_to_string(dir.path().join(format!("{id}.wav"))).unwrap(),
            format!("original audio {id}")
        );
    }
}

#[test]
fn stale_rename_drafts_require_refresh_and_disk_conflicts_keep_the_typed_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(dir.path()));
    settle(&mut h);
    type_name(&mut h, "New playlist name", "Set");
    click(&mut h, "Create playlist");
    type_name(&mut h, "Rename playlist", "Unsaved name");
    click(&mut h, "Add track 1");
    assert!(h.get_by_label("Save playlist name").accesskit_node().is_disabled());
    assert_eq!(h.state().dj.draft.as_ref().unwrap().name, "Unsaved name");
    click(&mut h, "Refresh playlist draft");
    assert_eq!(h.state().dj.draft.as_ref().unwrap().name, "Set");
    type_name(&mut h, "Rename playlist", "Keep this on failure");
    let path = dir.path().join("dj-library.json");
    let mut external: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    external["revision"] = json!(99);
    external["playlists"][0]["name"] = json!("Other app");
    std::fs::write(&path, serde_json::to_vec(&external).unwrap()).unwrap();
    click(&mut h, "Save playlist name");
    assert!(h.state().dj.snapshot["error"].as_str().unwrap().contains("another app"));
    assert_eq!(h.state().dj.draft.as_ref().unwrap().name, "Keep this on failure");
    click(&mut h, "Refresh DJ library");
    assert_eq!(h.state().dj.snapshot["playlists"][0]["name"], "Other app");
    assert!(h.get_by_label("Save playlist name").accesskit_node().is_disabled());
    click(&mut h, "Refresh playlist draft");
    assert_eq!(h.state().dj.draft.as_ref().unwrap().name, "Other app");
}

#[test]
#[ignore = "requires a GPU adapter; renders playlist review artifacts"]
fn render_dj_playlists_for_review() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = fixture(dir.path());
    app.dj.snapshot = json!({"tracks":(1..=3).map(|id|json!({"id":id,"title":format!("Track {id}"),"seconds":180.0,"bpm":120.0,"key":"Am"})).collect::<Vec<_>>(),
        "playlists":[{"id":1,"name":"Opening set","tracks":[3,1]}],"busy":false,"revision":1});
    app.dj.selected_playlist = Some(1);
    let mut h = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&h.ctx);
    h.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    h.render().unwrap().save(folder.join("dj-playlists.png")).unwrap();
}

#[test]
fn a_large_library_can_be_browsed_and_a_removed_playlist_invalidates_its_confirmation() {
    let mut app = App::new();
    app.dj.active = true;
    app.dj.snapshot = json!({"busy":false,"revision":1,
        "tracks":(1..=70).map(|id|json!({"id":id,"title":format!("Track {id}"),"seconds":1.0})).collect::<Vec<_>>(),
        "playlists":[{"id":1,"name":"Set","tracks":[]}]});
    let mut h = harness(app);
    h.run();
    assert!(h.query_by_label("Track 70").is_none());
    h.get_by_label("Next page").click();
    h.run();
    assert!(h.query_by_label("Track 25").is_some());
    assert!(h.query_by_label("Track 1").is_none());
    h.get_by_label("Next page").click();
    h.run();
    assert!(h.query_by_label("Track 70").is_some());
    assert!(h.get_by_label("Next page").accesskit_node().is_disabled());
    h.state_mut().dj.selected_playlist = Some(1);
    h.run();
    h.get_by_label("Delete playlist…").click();
    h.run();
    assert!(h.query_by_label("Confirm delete playlist").is_some());
    h.state_mut().dj.snapshot["revision"] = json!(2);
    h.state_mut().dj.snapshot["playlists"] = json!([]);
    h.run();
    assert!(h.state().dj.selected_playlist.is_none());
    assert!(h.state().dj.delete_confirmation.is_none());
    assert!(h.query_by_label("Confirm delete playlist").is_none());
}

#[test]
fn switching_settings_folders_drops_old_library_selections_and_drafts() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let mut h = harness(fixture(first.path()));
    settle(&mut h);
    type_name(&mut h, "New playlist name", "Old collection");
    click(&mut h, "Create playlist");
    type_name(&mut h, "Rename playlist", "Unsaved draft");
    drop(fixture(second.path()));
    h.state_mut().set_file_library(suhina_library::Library::new(Some(second.path().to_owned())));
    assert!(h.state().dj.snapshot.is_null());
    assert!(h.state().dj.selected_playlist.is_none());
    assert!(h.state().dj.draft.is_none());
    settle(&mut h);
    assert_eq!(h.state().dj.snapshot["playlists"], json!([]));
    assert!(h.state().dj.active);
    let saved: Value = serde_json::from_slice(&std::fs::read(first.path().join("dj-library.json")).unwrap()).unwrap();
    assert_eq!(saved["playlists"][0]["name"], "Old collection");
}
