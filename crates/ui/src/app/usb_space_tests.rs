// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn tree() -> Value {
    json!([
        { "k": "sub", "id": "app", "text": "rbxport", "on": true, "items": [
            { "k": "role", "role": "about", "text": "About rbxport" },
            { "k": "item", "id": "settings", "text": "Settings…", "on": true, "key": "CmdOrCtrl+," },
        ]},
        { "k": "sub", "id": "file", "text": "File", "on": true, "items": [
            { "k": "item", "id": "import", "text": "Import", "on": true, "key": "CmdOrCtrl+O" },
            { "k": "item", "id": "import-folder", "text": "Import Folder…", "on": true, "key": "CmdOrCtrl+Shift+O" },
            { "k": "sep" },
            { "k": "item", "id": "export-xml", "text": "Export Collection in xml format…", "on": false, "key": null },
        ]},
        { "k": "sub", "id": "track", "text": "Track", "on": true, "items": [
            { "k": "sub", "id": "analysis-lock", "text": "Analysis Lock", "on": true, "items": [
                { "k": "item", "id": "analysis-lock-on", "text": "Analysis Lock On", "on": true, "key": null, "checked": true },
            ]},
        ]},
        { "k": "sub", "id": "empty", "text": "Nothing", "on": true, "items": [] },
    ])
}

fn ready(app: &mut App) {
    app.set_usb_host(UsbHost {
        status: UsbStatus::Ready,
        pages: vec![
            UsbPage { label: "main".into(), title: "rbxport".into() },
            UsbPage { label: "preferences".into(), title: "Preferences".into() },
        ],
        active: "main".into(),
        menus: menus_from_tree(&tree(), false),
    });
}

fn window(app: App) -> egui_kittest::Harness<'static, App> {
    egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1400.0, 860.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

#[test]
fn the_menu_bar_reads_from_rbxports_tree_with_its_keys_greyed_items_and_submenus() {
    let menus = menus_from_tree(&tree(), false);
    let titles: Vec<_> = menus.iter().map(|m| m.title.as_str()).collect();
    assert_eq!(titles, ["rbxport", "File", "Track"], "an empty menu is left out");
    let Entry::Item(about) = &menus[0].entries[0] else { panic!("about") };
    assert_eq!(about.id, "role:about");
    let Entry::Item(import) = &menus[1].entries[1] else { panic!("import folder") };
    assert_eq!((import.id.as_str(), import.shortcut.as_str()), ("import-folder", "Ctrl+Shift+O"));
    assert_eq!(menus[1].entries[2], Entry::Separator);
    let Entry::Item(export) = &menus[1].entries[3] else { panic!("export") };
    assert!(!export.enabled && export.shortcut.is_empty());
    let Entry::Submenu { label, entries, .. } = &menus[2].entries[0] else { panic!("submenu") };
    assert_eq!(label, "Analysis Lock");
    let Entry::Item(lock) = &entries[0] else { panic!("lock") };
    assert_eq!(lock.label, "✔ Analysis Lock On");
    assert!(menus_from_tree(&Value::Null, false).is_empty());
}

#[test]
fn keys_are_written_the_way_each_system_shows_them() {
    assert_eq!(shortcut("CmdOrCtrl+Shift+O", false), "Ctrl+Shift+O");
    assert_eq!(shortcut("CmdOrCtrl+Shift+O", true), "⇧⌘O");
    assert_eq!(shortcut("Shift+CmdOrCtrl+F", false), "Ctrl+Shift+F");
    assert_eq!(shortcut("CmdOrCtrl+,", true), "⌘,");
    assert_eq!(shortcut("", false), "");
}

#[test]
fn rbxport_fills_the_usb_space_inside_the_window_with_its_menus_and_pages() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = window(app);
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    ready(harness.state_mut());
    harness.run();
    let view = harness.state_mut().take_usb_view(&egui::Context::default()).expect("a place for rbxport");
    assert_eq!(view.page, "main");
    assert!(view.rect.width() > 1000.0 && view.rect.height() > 600.0, "rbxport gets most of the window: {view:?}");
    assert!(harness.query_by_label("Open rbxport").is_none(), "nothing opens a window of its own");
    assert!(harness.query_by_label("Library").is_some() && harness.query_by_label("Preferences").is_some());
    // The studio's transport is not in the USB space.
    assert!(harness.query_by_label("Play").is_none());
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label_contains("Import Folder…").click();
    harness.run();
    harness.get_by_label("Preferences").click();
    harness.run();
    harness.get_by_label("✕").click();
    harness.run();
    assert_eq!(
        harness.state_mut().take_usb_asks(),
        [UsbAsk::Menu("import-folder".into()), UsbAsk::Show("preferences".into()), UsbAsk::Close("preferences".into()),]
    );
    assert!(harness.state_mut().take_usb_asks().is_empty(), "asks are taken once");
}

#[test]
fn an_open_menu_over_rbxport_moves_its_webview_aside() {
    let mut harness = window(App::new());
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    ready(harness.state_mut());
    harness.run();
    let ctx = harness.ctx.clone();
    assert!(!harness.state_mut().take_usb_view(&ctx).expect("view").covered);
    harness.get_by_label("File").click();
    harness.run();
    let ctx = harness.ctx.clone();
    assert!(harness.state_mut().take_usb_view(&ctx).expect("view").covered, "the open menu would be under it");
}

#[test]
fn the_usb_space_says_when_rbxport_is_starting_missing_or_stopped() {
    let mut harness = window(App::new());
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    harness.state_mut().set_usb_host(UsbHost { status: UsbStatus::Starting, ..UsbHost::default() });
    harness.run_steps(2);
    assert!(harness.state_mut().take_usb_view(&egui::Context::default()).is_none(), "no webview until it runs");
    assert!(harness.query_by_label_contains("rbxport is starting inside this window").is_some());
    harness.state_mut().set_usb_host(UsbHost { status: UsbStatus::Missing, ..UsbHost::default() });
    harness.run_steps(2);
    assert!(harness.query_by_label("Get rbxport").is_some());
    harness.state_mut().set_usb_host(UsbHost { status: UsbStatus::Failed("it closed".into()), ..UsbHost::default() });
    harness.run_steps(2);
    assert!(harness.query_by_label_contains("rbxport stopped: it closed").is_some());
    harness.get_by_label("Start rbxport again").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().take_usb_asks(), [UsbAsk::Restart]);
}

#[test]
fn leaving_the_usb_space_hides_rbxport() {
    let mut harness = window(App::new());
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    ready(harness.state_mut());
    harness.run();
    assert!(harness.state_mut().take_usb_view(&egui::Context::default()).is_some());
    super::super::menu_bar::pick_mode(&mut harness, "DJ");
    harness.run();
    assert!(harness.state_mut().take_usb_view(&egui::Context::default()).is_none());
}

#[test]
fn the_window_bar_holds_rbxports_menus_in_the_usb_space_and_the_studios_elsewhere() {
    let mut harness = window(App::new());
    harness.run();
    let studio: Vec<String> = harness.state().menus().iter().map(|m| m.title.to_string()).collect();
    assert!(studio.contains(&"Projects".to_owned()));
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    ready(harness.state_mut());
    harness.run();
    let usb: Vec<String> = harness.state().menus().iter().map(|m| m.title.to_string()).collect();
    assert_eq!(usb, ["YOBUKO", "rbxport", "File", "Track"]);
    harness.get_by_label("rbxport").click();
    harness.run();
    harness.get_by_label("About rbxport").click();
    harness.run();
    assert!(harness.query_by_label_contains("DJ library and USB export manager").is_some());
    super::super::menu_bar::pick_mode(&mut harness, "Studio");
    let back: Vec<String> = harness.state().menus().iter().map(|m| m.title.to_string()).collect();
    assert_eq!(back, studio);
}

#[test]
fn without_a_menu_bar_in_the_window_rbxports_menus_sit_on_the_usb_page() {
    if cfg!(target_os = "macos") {
        return; // the system's bar has them there
    }
    let mut app = App::new();
    app.settings.view.menu_bar = false;
    let mut harness = window(app);
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "USB");
    ready(harness.state_mut());
    harness.run();
    harness.get_by_label("Track").click();
    harness.run();
    harness.get_by_label("Analysis Lock").hover();
    harness.run();
    harness.get_by_label_contains("✔ Analysis Lock On").click();
    harness.run();
    assert_eq!(harness.state_mut().take_usb_asks(), [UsbAsk::Menu("analysis-lock-on".into())]);
}
