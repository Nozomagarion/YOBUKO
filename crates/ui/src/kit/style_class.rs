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
    "status-bar",
    "browser",
    "session",
    "arrangement",
    "mixer",
    "track-header",
    "clip",
    "scene",
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
}
impl Drop for Scope {
    fn drop(&mut self) {
        theme::local_palette(self.palette);
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}
pub fn enter(class: &str) -> Scope {
    let saved = theme::palette();
    STACK.with(|s| s.borrow_mut().push(class.into()));
    let mut palette = saved;
    if let Some(color) = property("color").as_deref().and_then(theme::parse_hex) {
        palette.text_primary = color;
        palette.text_secondary = color;
        palette.text_value = color;
        palette.browser_text_primary = color;
        palette.browser_text_secondary = color;
        palette.browser_text_value = color;
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
    theme::local_palette(palette);
    Scope { palette: saved }
}
pub fn radius(fallback: f32) -> f32 {
    property("border-radius")
        .and_then(|v| v.strip_suffix("px").and_then(|n| n.parse::<u8>().ok()))
        .map_or(fallback, f32::from)
}
pub fn font_size(fallback: f32) -> f32 {
    property("font-size")
        .and_then(|v| v.strip_suffix("px").and_then(|n| n.parse::<u8>().ok()))
        .map_or(fallback, f32::from)
}

#[cfg(test)]
mod tests {
    use super::*;
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
