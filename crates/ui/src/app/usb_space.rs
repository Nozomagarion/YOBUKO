// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO: the USB space shows rbxport (Chris Le's DJ library app, shipped inside YOBUKO) in
//! this window, with every one of its menus, buttons and windows. The desktop app runs rbxport
//! hidden and puts its interface in a webview over the rectangle this page leaves for it
//! (`apps/suhina/src/rbxport_panel.rs`); this page draws the rest in YOBUKO's look: rbxport's
//! menu bar, a tab for each window rbxport opened (Preferences, the Sync Manager…), and what
//! to say while rbxport starts or when it could not.
//!
//! The page and the desktop app talk through [`App::set_usb_host`] (what rbxport shows),
//! [`App::take_usb_view`] (where its webview goes this frame) and [`App::take_usb_asks`] (what
//! was picked here).

use std::path::PathBuf;

use egui::{Rect, Ui, Vec2};
use serde_json::Value;

use super::menus::{Entry, Item};
use super::*;

/// One page of rbxport: its main window, or one it opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbPage {
    /// rbxport's name for the window (`main`, `preferences`, `sync`, `report`, `log`).
    pub label: String,
    pub title: String,
}

/// One menu of rbxport's bar.
#[derive(Clone, Debug, PartialEq)]
pub struct UsbMenu {
    pub title: String,
    pub enabled: bool,
    pub entries: Vec<Entry>,
}

/// How far rbxport is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UsbStatus {
    /// Not started yet: the USB space has not been opened.
    #[default]
    Off,
    Starting,
    Ready,
    /// This build of YOBUKO has no rbxport inside.
    Missing,
    /// It could not start, or stopped: why.
    Failed(String),
}

/// What the desktop app says about rbxport, every frame it changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsbHost {
    pub status: UsbStatus,
    /// The main page first, then the windows rbxport opened, in order.
    pub pages: Vec<UsbPage>,
    /// The page in front.
    pub active: String,
    pub menus: Vec<UsbMenu>,
}

/// What was picked on the page, for the desktop app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbAsk {
    /// A menu item of rbxport's, by its id.
    Menu(String),
    /// One of the system's items of rbxport's menu: `about`, `cut`, `copy`, `paste`, `select-all`.
    Role(String),
    /// Bring this page to the front.
    Show(String),
    /// Close this page (a window rbxport opened).
    Close(String),
    /// Start rbxport again after it stopped.
    Restart,
}

/// Where rbxport's webview goes this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct UsbView {
    /// In points; the desktop app scales it to the window's pixels.
    pub rect: Rect,
    /// Something of YOBUKO's (a menu, a dialog, Settings) lies over the rectangle: the webview
    /// steps aside, since it would hide it.
    pub covered: bool,
    pub page: String,
}

/// The USB space's share of the window, kept in the DJ panel.
#[derive(Default)]
pub(super) struct UsbSpace {
    pub(super) host: UsbHost,
    pub(super) view: Option<UsbView>,
    pub(super) asks: Vec<UsbAsk>,
    /// The About box of rbxport's own menu shows.
    pub(super) about: bool,
}

/// The rbxport program to run inside this window: the one shipped with YOBUKO first, else one
/// installed on its own. On macOS, the program inside the app bundle, since it is started with
/// its connection to YOBUKO rather than opened as an app of its own.
pub fn rbxport_program() -> Option<PathBuf> {
    let found = super::dj::rbxport::find()?;
    if found.extension().is_some_and(|ext| ext == "app") {
        let inner = found.join("Contents").join("MacOS").join("rbxport");
        return inner.exists().then_some(inner);
    }
    Some(found)
}

/// rbxport's menu bar from the tree its program sends (`yobuko_embed.rs › menu_tree`).
pub fn menus_from_tree(tree: &Value, mac: bool) -> Vec<UsbMenu> {
    let Some(menus) = tree.as_array() else { return Vec::new() };
    menus
        .iter()
        .filter(|menu| menu.get("k").and_then(Value::as_str) == Some("sub"))
        .map(|menu| UsbMenu {
            title: text(menu, "text"),
            enabled: menu.get("on").and_then(Value::as_bool).unwrap_or(true),
            entries: entries(menu.get("items").unwrap_or(&Value::Null), mac),
        })
        .filter(|menu| !menu.entries.is_empty())
        .collect()
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn entries(items: &Value, mac: bool) -> Vec<Entry> {
    let Some(items) = items.as_array() else { return Vec::new() };
    items
        .iter()
        .filter_map(|item| {
            let enabled = item.get("on").and_then(Value::as_bool).unwrap_or(true);
            match item.get("k").and_then(Value::as_str)? {
                "sep" => Some(Entry::Separator),
                "sub" => Some(Entry::Submenu {
                    label: text(item, "text"),
                    enabled,
                    entries: entries(item.get("items").unwrap_or(&Value::Null), mac),
                }),
                "role" => Some(Item::new(format!("role:{}", text(item, "role")), text(item, "text")).into()),
                "item" => {
                    let mut label = text(item, "text");
                    if item.get("checked").and_then(Value::as_bool) == Some(true) {
                        label = format!("✔ {label}");
                    }
                    let keys =
                        item.get("key").and_then(Value::as_str).map(|key| shortcut(key, mac)).unwrap_or_default();
                    Some(Item::new(text(item, "id"), label).keys(keys).enabled(enabled).into())
                }
                _ => None,
            }
        })
        .collect()
}

/// A menu key as rbxport writes it (`CmdOrCtrl+Shift+O`), as this system shows keys.
pub fn shortcut(accelerator: &str, mac: bool) -> String {
    let mut parts: Vec<&str> = accelerator.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let Some(key) = parts.pop() else { return String::new() };
    let has = |name: &str| parts.iter().any(|p| p.eq_ignore_ascii_case(name));
    let command = has("CmdOrCtrl") || has("Cmd") || has("Ctrl") || has("Command") || has("Control");
    let key = key.to_uppercase();
    if mac {
        let mut out = String::new();
        if has("Alt") || has("Option") {
            out.push('⌥');
        }
        if has("Shift") {
            out.push('⇧');
        }
        if command {
            out.push('⌘');
        }
        out + &key
    } else {
        let mut out = Vec::new();
        if command {
            out.push("Ctrl");
        }
        if has("Alt") {
            out.push("Alt");
        }
        if has("Shift") {
            out.push("Shift");
        }
        out.push(&key);
        out.join("+")
    }
}

/// Whether anything of YOBUKO's own floats over `rect`: a menu, a popup, a window or a dialog.
fn covered(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.memory(|memory| {
        memory.areas().visible_layer_ids().into_iter().any(|layer| {
            matches!(layer.order, egui::Order::Middle | egui::Order::Foreground)
                && memory.area_rect(layer.id).is_some_and(|area| area.intersects(rect))
        })
    })
}

/// The radius of the page's soft shapes, as the DJ space's.
const RADIUS: u8 = 10;

/// rbxport's entries with their ids marked as rbxport's, for the window's bar.
fn marked(entries: &[Entry]) -> Vec<Entry> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Item(item) => Entry::Item(Item { id: format!("usb:{}", item.id), ..item.clone() }),
            Entry::Separator => Entry::Separator,
            Entry::Submenu { label, enabled, entries } => {
                Entry::Submenu { label: label.clone(), enabled: *enabled, entries: marked(entries) }
            }
        })
        .collect()
}

impl App {
    /// In the USB space, the window's menu bar (and the system's on macOS) is rbxport's, after
    /// a YOBUKO menu for the studio's settings and quitting. `None` elsewhere.
    pub(super) fn usb_menus(&self) -> Option<Vec<menus::Menu>> {
        let host = &self.dj.space.host;
        if self.workspace_mode() != super::menu_bar::WorkspaceMode::Usb
            || host.status != UsbStatus::Ready
            || host.menus.is_empty()
        {
            return None;
        }
        let mut out = vec![menus::Menu {
            title: "YOBUKO".into(),
            entries: vec![
                Item::new("settings.open", "YOBUKO settings").into(),
                Entry::Separator,
                Item::new("menu.quit", "Quit YOBUKO").into(),
            ],
        }];
        out.extend(
            host.menus
                .iter()
                .filter(|menu| menu.enabled)
                .map(|menu| menus::Menu { title: menu.title.clone().into(), entries: marked(&menu.entries) }),
        );
        Some(out)
    }

    /// An item of rbxport's menu was picked (its id without the `usb:` mark).
    pub(super) fn usb_menu_picked(&mut self, id: &str) {
        match id.strip_prefix("role:") {
            Some("about") => self.dj.space.about = true,
            Some(role) => self.dj.space.asks.push(UsbAsk::Role(role.to_owned())),
            None => self.dj.space.asks.push(UsbAsk::Menu(id.to_owned())),
        }
    }

    /// What rbxport shows, from the desktop app.
    pub fn set_usb_host(&mut self, host: UsbHost) {
        if self.dj.space.host != host {
            self.dj.space.host = host;
        }
    }

    /// What rbxport shows, as last set.
    pub fn usb_host(&self) -> &UsbHost {
        &self.dj.space.host
    }

    /// Where rbxport's webview goes this frame; `None` when the USB space is not showing, and
    /// the webview hides. Taken once a frame, after [`App::ui`].
    pub fn take_usb_view(&mut self, ctx: &egui::Context) -> Option<UsbView> {
        let mut view = self.dj.space.view.take()?;
        view.covered = covered(ctx, view.rect);
        Some(view)
    }

    /// Whether the USB space shows: rbxport starts the first time it does.
    pub fn usb_space_shown(&self) -> bool {
        self.workspace_mode() == super::menu_bar::WorkspaceMode::Usb
    }

    /// What was picked on the USB page since the last call.
    pub fn take_usb_asks(&mut self) -> Vec<UsbAsk> {
        std::mem::take(&mut self.dj.space.asks)
    }

    /// The USB space: rbxport's menus and pages over the rectangle its interface fills.
    pub(super) fn show_usb(&mut self, ui: &mut Ui) {
        use super::menu_bar::WorkspaceMode;
        let host = self.dj.space.host.clone();
        if host.status != UsbStatus::Ready {
            self.show_usb_waiting(ui, &host.status);
            return;
        }
        let outer = ui.available_rect_before_wrap().shrink(theme::PAD);
        let strip_height = 34.0;
        let strip = Rect::from_min_size(outer.min, Vec2::new(outer.width(), strip_height));
        let body = Rect::from_min_max(egui::pos2(outer.left(), strip.bottom() + theme::PAD / 2.0), outer.max);
        // The strip: rbxport's menus, its pages, and YOBUKO's two ways into the library.
        ui.painter().rect_filled(strip, RADIUS, theme::panel_background());
        ui.painter().rect_stroke(strip, RADIUS, egui::Stroke::new(1.0, theme::border()), egui::StrokeKind::Inside);
        let mut strip_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(strip.shrink2(Vec2::new(8.0, 3.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        // rbxport's menus are in the window's bar (`usb_menus`); without one in the window (and
        // on macOS, where the system's bar has them), here.
        if !self.settings.view.menu_bar && !cfg!(target_os = "macos") {
            let menus: Vec<(String, bool, Vec<Entry>)> =
                host.menus.iter().map(|menu| (menu.title.clone(), menu.enabled, menu.entries.clone())).collect();
            if let Some(picked) = menus::show_named(&mut strip_ui, &menus) {
                self.usb_menu_picked(&picked);
            }
            strip_ui.separator();
        }
        if host.pages.len() > 1 {
            for page in &host.pages {
                let on = page.label == host.active;
                let name = if page.label == "main" { "Library".to_owned() } else { page.title.clone() };
                let tab = egui::Button::selectable(on, name).corner_radius(RADIUS);
                if strip_ui.add(tab).on_hover_text("Show this rbxport page").clicked() && !on {
                    self.dj.space.asks.push(UsbAsk::Show(page.label.clone()));
                }
                if page.label != "main"
                    && strip_ui
                        .add(egui::Button::new("✕").frame(false).small())
                        .on_hover_text(format!("Close {}", page.title))
                        .clicked()
                {
                    self.dj.space.asks.push(UsbAsk::Close(page.label.clone()));
                }
            }
        }
        strip_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("Share library now").corner_radius(RADIUS))
                .on_hover_text("Bring the DJ library and rbxport up to date with each other")
                .clicked()
            {
                self.share_library_now();
            }
            if ui
                .add_enabled(
                    self.session().is_some(),
                    egui::Button::new("Add this project to the library…").corner_radius(RADIUS),
                )
                .on_hover_text("Render the project to audio and add it to your DJ library")
                .on_disabled_hover_text("Open a project first")
                .clicked()
            {
                self.open_export();
                self.dj.next_export_to_dj = true;
            }
            let (bird, _) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
            let mood = self.piou_mood(ui.ctx());
            let beat = self.beat_seconds();
            crate::kit::piou::show(ui.ctx(), ui.painter(), bird, mood, WorkspaceMode::Usb.coat(), beat);
            if let Some(note) = &self.dj.note {
                let (text, color) = match note {
                    Ok(text) => (text.as_str(), theme::text_secondary()),
                    Err(text) => (text.as_str(), theme::accent()),
                };
                let first = text.lines().next().unwrap_or_default();
                ui.add(egui::Label::new(egui::RichText::new(first).small().color(color)).truncate())
                    .on_hover_text(text);
            }
        });
        // The card rbxport's interface fills: rounded, in the space's colour along its edge.
        ui.painter().rect_filled(body, RADIUS, theme::panel_background());
        ui.painter().rect_stroke(
            body,
            RADIUS,
            egui::Stroke::new(1.5, WorkspaceMode::Usb.color().gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        let inner = body.shrink(4.0);
        ui.allocate_rect(outer, egui::Sense::hover());
        self.dj.space.view = Some(UsbView { rect: inner, covered: false, page: host.active.clone() });
        if self.dj.space.about {
            let mut open = true;
            egui::Window::new("About rbxport")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.label(egui::RichText::new("rbxport").strong().size(18.0));
                    ui.label("DJ library and USB export manager, by Chris Le.");
                    ui.label("Free software (GPL-2.0-or-later), shipped inside YOBUKO.");
                    ui.hyperlink("https://github.com/chrisle/rbxport");
                });
            self.dj.space.about = open;
        }
    }

    /// The USB page while rbxport is not on it: starting, missing or stopped.
    fn show_usb_waiting(&mut self, ui: &mut Ui, status: &UsbStatus) {
        use super::menu_bar::WorkspaceMode;
        let (heading, about) = match status {
            UsbStatus::Missing => (
                "USB export · rbxport",
                "This copy of YOBUKO has no rbxport inside, so USB sticks cannot be prepared here.".to_owned(),
            ),
            UsbStatus::Failed(why) => ("USB export · rbxport", format!("rbxport stopped: {why}")),
            UsbStatus::Off | UsbStatus::Starting | UsbStatus::Ready => (
                "USB export · rbxport",
                "rbxport is starting inside this window: your playlists, beat grids and hot cues, ready to \
                 go on a USB stick for CDJ and XDJ players."
                    .to_owned(),
            ),
        };
        let failed = matches!(status, UsbStatus::Failed(_));
        let missing = *status == UsbStatus::Missing;
        self.space_page(ui, WorkspaceMode::Usb, heading, &about, |app, ui| {
            if failed && shell::main_button(ui, WorkspaceMode::Usb, "Start rbxport again").clicked() {
                app.dj.space.asks.push(UsbAsk::Restart);
            }
            if missing && shell::main_button(ui, WorkspaceMode::Usb, "Get rbxport").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(super::dj::rbxport::DOWNLOAD_PAGE));
            }
            app.dj_note(ui);
            ui.add_space(theme::PAD * 3.0);
            ui.weak("rbxport by Chris Le, free software (GPL-2.0-or-later): github.com/chrisle/rbxport");
        });
    }
}

#[cfg(test)]
#[path = "usb_space_tests.rs"]
mod tests;
