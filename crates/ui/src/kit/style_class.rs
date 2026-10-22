// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable native CSS classes. A scoped palette lets existing painter code inherit styles.
use super::{skin, theme};
use std::cell::RefCell;

pub const CLASSES: &[&str] = &[
    "window",
    "application-bar",
    "menu-bar",
    "project-tabs",
    "toolbar",
    "toolbar-module",
    "status-bar",
    "browser",
    "session",
    "arrangement",
    "mixer",
    "track-header",
    "clip",
    "scene",
    "scene-column",
    "track-column",
    "slot",
    "detail",
    "clip-editor",
    "piano-roll",
    "device-panel",
    "settings",
    "dialog",
    "sample-map",
    "button",
    "fader",
    "knob",
    "meter",
    "value",
    "dropdown",
    "text-input",
];
thread_local! {static STACK: RefCell<Vec<String>> = const {RefCell::new(Vec::new())};}

pub fn valid_selector(selector: &str) -> bool {
    let parts: Vec<_> = selector.split_whitespace().collect();
    !parts.is_empty()
        && parts.len() <= 4
        && parts.iter().all(|part| part.strip_prefix('.').is_some_and(|c| CLASSES.contains(&c)))
}
pub fn valid_property(key: &str, value: &str) -> bool {
    match key {
        "color" | "background-color" | "border-color" => theme::parse_hex(value).is_some(),
        "border-radius" => value.strip_suffix("px").and_then(|v| v.parse::<u8>().ok()).is_some_and(|v| v <= 32),
        "font-size" => {
            value.strip_suffix("px").and_then(|v| v.parse::<u8>().ok()).is_some_and(|v| (7..=32).contains(&v))
        }
        _ => false,
    }
}
fn matches(selector: &str, stack: &[String]) -> bool {
    let parts: Vec<_> = selector.split_whitespace().filter_map(|s| s.strip_prefix('.')).collect();
    if parts.last().copied() != stack.last().map(String::as_str) {
        return false;
    }
    let mut at = 0;
    for part in parts {
        let Some(index) = stack.get(at..).and_then(|s| s.iter().position(|c| c == part)) else {
            return false;
        };
        at += index + 1;
    }
    true
}
pub fn property(key: &str) -> Option<String> {
    let skin = skin::current();
    STACK.with(|stack| {
        let stack = stack.borrow();
        // More specific descendants win. Equal-specificity declarations are already normalized by the CSS reader.
        skin.styles
            .iter()
            .filter(|(selector, _)| matches(selector, &stack))
            .filter_map(|(selector, properties)| {
                properties.get(key).map(|value| (selector.split_whitespace().count(), value))
            })
            .max_by_key(|(specificity, _)| *specificity)
            .map(|(_, value)| value.clone())
    })
}

pub struct Scope {
    palette: theme::Palette,
    native: bool,
}
impl Drop for Scope {
    fn drop(&mut self) {
        theme::local_palette(self.palette);
        if self.native {
            super::native_css::leave();
        }
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}
pub fn enter(class: &str) -> Scope {
    enter_inner(class, None, None)
}
pub fn enter_named(class: &str, name: &str) -> Scope {
    enter_inner(class, Some(name), None)
}
pub fn enter_response(class: &str, response: &egui::Response) -> Scope {
    enter_inner(class, None, Some(response))
}
fn enter_inner(class: &str, name: Option<&str>, response: Option<&egui::Response>) -> Scope {
    let native = super::native_css::enter(class, name, response);
    let saved = theme::palette();
    STACK.with(|s| s.borrow_mut().push(class.into()));
    let mut palette = saved;
    if let Some(color) = property("color").as_deref().and_then(theme::parse_hex) {
        set_foreground(&mut palette, color, class);
    }
    if let Some(color) = property("background-color").as_deref().and_then(theme::parse_hex) {
        palette.window_background = color;
        palette.panel_background = color;
        palette.control_background = color;
    }
    if let Some(color) = property("border-color").as_deref().and_then(theme::parse_hex) {
        palette.border = color;
        palette.border_strong = color;
    }
    if let Some(values) = STACK.with(|stack| super::native_css::scoped(&stack.borrow())) {
        if values.authored.color {
            set_foreground(&mut palette, values.color, class);
        }
        if values.authored.border {
            palette.border = values.border;
            palette.border_strong = values.border;
        }
        if values.authored.background {
            palette.window_background = values.background;
            palette.panel_background = values.background;
            palette.control_background = values.background;
        }
    }
    theme::local_palette(palette);
    Scope { palette: saved, native }
}
fn set_foreground(palette: &mut theme::Palette, color: egui::Color32, class: &str) {
    palette.text_primary = color;
    palette.text_secondary = color;
    palette.text_value = color;
    palette.text_hint = color;
    palette.browser_text_primary = color;
    palette.browser_text_secondary = color;
    palette.browser_text_value = color;
    palette.text_on_color_dark = color;
    palette.text_on_color_light = color;
    if class == "meter" {
        palette.meter_normal = color;
    }
}
pub fn foreground(fallback: egui::Color32) -> egui::Color32 {
    if let Some(style) = computed()
        && style.authored.color
    {
        return style.color;
    }
    property("color").as_deref().and_then(theme::parse_hex).unwrap_or(fallback)
}
pub fn computed() -> Option<super::native_css::Computed> {
    STACK.with(|stack| super::native_css::scoped(&stack.borrow()))
}
pub fn radius(fallback: f32) -> f32 {
    if let Some(values) = STACK.with(|stack| super::native_css::scoped(&stack.borrow()))
        && values.authored.radius
    {
        return values.radius;
    }
    property("border-radius")
        .and_then(|v| v.strip_suffix("px").and_then(|n| n.parse::<u8>().ok()))
        .map_or(fallback, f32::from)
}
pub fn font_size(fallback: f32) -> f32 {
    if let Some(values) = STACK.with(|stack| super::native_css::scoped(&stack.borrow()))
        && values.authored.font_size
    {
        return values.font_size;
    }
    property("font-size")
        .and_then(|v| v.strip_suffix("px").and_then(|n| n.parse::<u8>().ok()))
        .map_or(fallback, f32::from)
}

/// Background override for surfaces whose fallback colour is supplied by their caller.
pub fn background(fallback: egui::Color32) -> egui::Color32 {
    if let Some(values) = STACK.with(|stack| super::native_css::scoped(&stack.borrow()))
        && values.authored.background
    {
        return values.background;
    }
    property("background-color").as_deref().and_then(theme::parse_hex).unwrap_or(fallback)
}

/// Applies authored values to ordinary egui widgets as well as native painters.
/// The guard reborrows the same Ui: widget ids and layout are unchanged, and early
/// returns restore the caller's style without unsafe pointers or a new child Ui.
pub struct UiStyle<'a> {
    ui: &'a mut egui::Ui,
    previous: Option<std::sync::Arc<egui::Style>>,
}
impl std::ops::Deref for UiStyle<'_> {
    type Target = egui::Ui;
    fn deref(&self) -> &Self::Target {
        &*self.ui
    }
}
impl std::ops::DerefMut for UiStyle<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.ui
    }
}
impl Drop for UiStyle<'_> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            self.ui.set_style(previous);
        }
    }
}
pub fn ui(ui: &mut egui::Ui) -> UiStyle<'_> {
    let values = computed();
    let color = values
        .as_ref()
        .filter(|style| style.authored.color)
        .map(|style| style.color)
        .or_else(|| property("color").as_deref().and_then(theme::parse_hex));
    let font = values
        .as_ref()
        .filter(|style| style.authored.font_size)
        .map(|style| style.font_size)
        .or_else(|| property("font-size").map(|_| font_size(theme::TEXT)));
    let radius = values
        .as_ref()
        .filter(|style| style.authored.radius)
        .map(|style| style.radius)
        .or_else(|| property("border-radius").map(|_| self::radius(0.0)));
    let previous = if color.is_some() || font.is_some() || radius.is_some() {
        let previous = ui.style().clone();
        let style = ui.style_mut();
        if let Some(color) = color {
            style.visuals.override_text_color = Some(color);
        }
        if let Some(size) = font {
            for font in style.text_styles.values_mut() {
                font.size = size;
            }
        }
        if let Some(radius) = radius {
            let radius = egui::CornerRadius::same(radius.clamp(0.0, 128.0) as u8);
            for widget in [
                &mut style.visuals.widgets.noninteractive,
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
                &mut style.visuals.widgets.open,
            ] {
                widget.corner_radius = radius;
            }
        }
        Some(previous)
    } else {
        None
    };
    UiStyle { ui, previous }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_widget_style_preserves_ui_identity_and_restores_caller() {
        let mut skin = skin::Skin::default();
        skin.styles.insert(
            ".settings".into(),
            [("color".into(), "#123456".into()), ("font-size".into(), "19px".into())].into(),
        );
        skin::configure(&skin);
        let context = egui::Context::default();
        context
            .run_ui(egui::RawInput::default(), |ui| {
                let original = ui.style().clone();
                let id = ui.id();
                {
                    let _scope = enter("settings");
                    let styled = self::ui(ui);
                    assert_eq!(styled.id(), id);
                    assert_eq!(styled.visuals().override_text_color, theme::parse_hex("#123456"));
                    assert!(styled.style().text_styles.values().all(|font| font.size == 19.0));
                }
                assert!(std::sync::Arc::ptr_eq(ui.style(), &original));
            })
            .textures_delta
            .clear();
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn descendant_styles_are_scoped_and_restore_palette() {
        let mut s = skin::Skin::default();
        s.styles.insert(".browser".into(), [("color".into(), "#123456".into())].into());
        s.styles.insert(".browser .button".into(), [("color".into(), "#ABCDEF".into())].into());
        skin::configure(&s);
        let original = theme::palette();
        {
            let _browser = enter("browser");
            assert_eq!(theme::text_primary(), theme::parse_hex("#123456").unwrap());
            {
                let _button = enter("button");
                assert_eq!(theme::text_primary(), theme::parse_hex("#ABCDEF").unwrap());
            }
            assert_eq!(theme::text_primary(), theme::parse_hex("#123456").unwrap());
        }
        assert_eq!(theme::palette(), original);
        skin::configure(&skin::Skin::default());
    }
}
