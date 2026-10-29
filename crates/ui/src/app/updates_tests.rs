// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::Queryable;

#[test]
fn the_github_button_is_visible_on_agents_updates_and_checks_without_changing_settings() {
    use crate::settings::settings_view::{self, About, PageState, PluginsView, Sources, ThemesView};
    use crate::settings::{Settings, keymap::Keymap};
    struct State {
        settings: Settings,
        page: PageState,
        pressed: bool,
        changed: bool,
    }
    let settings = Settings::default();
    let before = settings.to_text();
    let state = State { settings, page: PageState::default(), pressed: false, changed: false };
    let mut harness = egui_kittest::Harness::builder().with_size(Vec2::new(1000.0, 700.0)).build_ui_state(
        |ui, state: &mut State| {
            let mut plugins = PluginsView::default();
            let mut keymap = Keymap::default();
            let about = About::default();
            let themes = ThemesView::default();
            let sources =
                Sources { about: &about, plugins: &mut plugins, themes: &themes, keymap: &mut keymap, tag_limit: 64 };
            let outcome = settings_view::show(ui, &mut state.settings, sources, &mut state.page);
            state.pressed |= outcome.check_updates;
            state.changed |= outcome.changed;
        },
        state,
    );
    harness.run();
    harness.get_by_label("Settings page: Agents and updates").click();
    harness.run();
    let button = harness.get_by_label("Check GitHub components and app updates");
    assert!(
        button.rect().top() >= 0.0 && button.rect().bottom() <= 700.0,
        "the update action must be visible without scrolling"
    );
    button.click();
    harness.run();
    assert!(harness.state().pressed);
    assert!(!harness.state().changed);
    assert_eq!(harness.state().settings.to_text(), before);
}

#[test]
#[ignore = "requires a GPU adapter; renders the GitHub update dialog for review"]
fn render_github_updates_dialog_for_review() {
    let sources = ["Suhina", "rbxport", "Multiplayer + DJ"]
        .into_iter()
        .map(|name| suhina_update::components::Component {
            name,
            installed: "a".repeat(40),
            latest: Ok("a".repeat(40)),
            page: "https://github.com",
            fork: name == "Multiplayer + DJ",
        })
        .collect();
    let updates = Updates { sources, open: true, ..Default::default() };
    let mut harness = egui_kittest::Harness::builder().with_size(Vec2::new(1000.0, 700.0)).wgpu().build_ui_state(
        |ui, updates: &mut Updates| {
            updates.show(ui, ui.max_rect(), &UpdateSettings::default());
        },
        updates,
    );
    theme::install(&harness.ctx);
    harness.run();
    let folder = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    harness.render().unwrap().save(folder.join("github-updates.png")).unwrap();
}
