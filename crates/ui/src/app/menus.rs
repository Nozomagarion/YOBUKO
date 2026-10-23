// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu bar as data: the window builds it every frame from what it shows (`App::menus`), the
//! window's own bar draws it here, and on macOS the system's bar is built from the same menus, so
//! the two always offer the same items. A pick from either runs `App::run_menu_item` by the item's
//! id.

use egui::Ui;

use crate::kit::theme;

/// One menu of the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: &'static str,
    pub entries: Vec<Entry>,
}

/// A line in a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Item(Item),
    Separator,
    Submenu { label: String, enabled: bool, entries: Vec<Entry> },
}

/// An item: what it runs, what it reads, its keys as text and whether it can be picked now.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// What `App::run_menu_item` runs: a keymap action's id, or a `menu.*` id of the bar's own.
    pub id: String,
    pub label: String,
    pub shortcut: String,
    pub enabled: bool,
    /// Shown when the pointer rests on it.
    pub hint: Option<String>,
}

impl Item {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), shortcut: String::new(), enabled: true, hint: None }
    }

    pub fn keys(mut self, shortcut: String) -> Self {
        self.shortcut = shortcut;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl From<Item> for Entry {
    fn from(item: Item) -> Self {
        Entry::Item(item)
    }
}

/// What the window's bar did this frame.
#[derive(Default)]
pub struct Shown {
    /// The item picked, by id.
    pub picked: Option<String>,
    /// The menus whose items showed.
    pub open: Vec<&'static str>,
}

/// Draws `menus` as the window's menu bar.
pub fn show(ui: &mut Ui, menus: &[Menu]) -> Shown {
    let mut shown = Shown::default();
    ui.style_mut().visuals.widgets.inactive.fg_stroke = theme::hairline(theme::text_secondary());
    egui::MenuBar::new().ui(ui, |ui| {
        for menu in menus {
            bar_menu(ui, menu.title, |ui| {
                shown.open.push(menu.title);
                entries(ui, &menu.entries, &mut shown.picked);
            });
        }
    });
    shown
}

fn entries(ui: &mut Ui, entries: &[Entry], picked: &mut Option<String>) {
    for entry in entries {
        match entry {
            Entry::Separator => {
                ui.separator();
            }
            Entry::Item(item) => {
                let mut button = egui::Button::new(&item.label);
                // An empty shortcut would still add a gap to the label.
                if !item.shortcut.is_empty() {
                    button = button.shortcut_text(&item.shortcut);
                }
                let mut response = ui.add_enabled(item.enabled, button);
                if let Some(hint) = &item.hint {
                    response = response.on_hover_text(hint);
                }
                if response.clicked() {
                    *picked = Some(item.id.clone());
                }
            }
            Entry::Submenu { label, enabled, entries: inner } => {
                ui.add_enabled_ui(*enabled, |ui| {
                    ui.menu_button(label, |ui| self::entries(ui, inner, picked));
                });
            }
        }
    }
}

/// A menu of the bar that opens when the pointer moves onto it while another is open.
fn bar_menu<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<Option<R>> {
    let shown = ui.menu_button(title, |ui| {
        // `.menu` styles the open menu: its background, text colour, corner radius and font size.
        let _menu = crate::kit::style_class::enter("menu");
        crate::kit::native_css::surface(ui.painter(), ui.clip_rect(), 0.0, egui::Color32::TRANSPARENT);
        let mut styled = crate::kit::style_class::ui(ui);
        content(&mut styled)
    });
    let id = egui::Popup::default_response_id(&shown.response);
    if shown.response.hovered() && !egui::Popup::is_id_open(ui.ctx(), id) && egui::Popup::is_any_open(ui.ctx()) {
        egui::Popup::open_id(ui.ctx(), id);
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(entries: &'a [Entry], id: &str) -> Option<&'a Item> {
        entries.iter().find_map(|entry| match entry {
            Entry::Item(item) if item.id == id => Some(item),
            Entry::Submenu { entries, .. } => find(entries, id),
            _ => None,
        })
    }

    #[test]
    fn without_a_project_saving_is_greyed_out_and_opening_is_not() {
        let menus = crate::App::new().menus();
        let titles: Vec<_> = menus.iter().map(|menu| menu.title).collect();
        assert_eq!(titles, ["File", "Edit", "View", "Projects", "Options", "Help"]);
        let file = &menus[0].entries;
        assert!(find(file, "file.open").is_some_and(|item| item.enabled));
        assert!(find(file, "file.save").is_some_and(|item| !item.enabled));
        assert!(
            file.iter()
                .any(|entry| matches!(entry, Entry::Submenu { label, enabled: false, .. } if label == "Open recent"))
        );
        assert!(find(&menus[1].entries, "edit.undo").is_some_and(|item| !item.enabled));
    }
}
