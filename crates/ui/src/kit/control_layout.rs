// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Theme-owned native control placement. Paint and interaction use the same rectangle.
use egui::{Rect, Vec2};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

pub type Placements = BTreeMap<String, Placement>;
pub const MAX_CONTROLS: usize = 256;

/// Keep public authoring names readable and stable, including SDK ids containing Unicode.
pub fn semantic_name(prefix: &str, id: &str) -> String {
    let prefix: String =
        prefix.chars().take(16).map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let direct = format!("{prefix}-{id}");
    if valid_name(&direct) {
        return direct;
    }
    let mut hash = 0xcbf29ce484222325u64;
    for byte in id.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    let slug: String =
        id.chars().take(24).map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    format!("{prefix}-{slug}-{hash:016x}")
}

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub native: Rect,
    pub at: Rect,
    pub bounds: Rect,
}

struct Runtime {
    ctx: egui::Context,
    entries: Vec<Entry>,
}
thread_local! { static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) }; }

fn draft_id() -> egui::Id {
    egui::Id::new("native-control-layout-draft")
}
pub fn editing(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Arc<Placements>>(draft_id())).is_some()
}
pub fn draft(ctx: &egui::Context, placements: Option<Placements>) {
    ctx.data_mut(|data| {
        if let Some(placements) = placements {
            data.insert_temp(draft_id(), Arc::new(placements));
        } else {
            data.remove::<Arc<Placements>>(draft_id());
        }
    });
}

/// One native frame's measured controls. The temporary context never survives the frame.
pub struct Capture(Option<Runtime>);
impl Capture {
    pub fn begin(ctx: &egui::Context) -> Self {
        Self(RUNTIME.with(|runtime| runtime.replace(Some(Runtime { ctx: ctx.clone(), entries: Vec::new() }))))
    }
    pub fn entries(&self) -> Vec<Entry> {
        RUNTIME.with(|runtime| runtime.borrow().as_ref().map_or_else(Vec::new, |runtime| runtime.entries.clone()))
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| runtime.replace(self.0.take()));
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub bottom: Option<i32>,
    pub hidden: bool,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub direction: Direction,
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

impl Placement {
    /// Explicit relative placement, in logical points; no stylesheet can escape its native panel.
    pub fn apply(self, native: Rect, bounds: Rect) -> Rect {
        if !native.is_finite() || !bounds.is_finite() || !bounds.is_positive() {
            return native;
        }
        let size =
            Vec2::new(self.width.map_or(native.width(), f32::from), self.height.map_or(native.height(), f32::from))
                .max(Vec2::splat(1.0))
                .min(bounds.size());
        let mut anchor = native.min + Vec2::new(self.x as f32, self.y as f32);
        if let Some(bottom) = self.bottom {
            anchor.y = bounds.bottom() - size.y - bottom as f32;
        }
        let min = anchor.max(bounds.min).min(bounds.max - size);
        Rect::from_min_size(min, size)
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), &'static str> {
        match key {
            "left" | "top" | "bottom" => {
                if key == "bottom" && value == "auto" {
                    self.bottom = None;
                    return Ok(());
                }
                let n = value
                    .strip_suffix("px")
                    .and_then(|v| v.parse::<i32>().ok())
                    .filter(|v| (-16384..=16384).contains(v))
                    .ok_or("offset must be -16384..16384px")?;
                match key {
                    "left" => self.x = n,
                    "top" => self.y = n,
                    _ => self.bottom = Some(n),
                }
            }
            "width" | "height" => {
                let n = if value == "auto" {
                    None
                } else {
                    Some(
                        value
                            .strip_suffix("px")
                            .and_then(|v| v.parse::<u16>().ok())
                            .filter(|v| (1..=4096).contains(v))
                            .ok_or("size must be auto or 1..4096px")?,
                    )
                };
                if key == "width" {
                    self.width = n;
                } else {
                    self.height = n;
                }
            }
            "--suhina-direction" => {
                self.direction = match value {
                    "vertical" => Direction::Vertical,
                    "horizontal" => Direction::Horizontal,
                    _ => return Err("direction must be vertical or horizontal"),
                }
            }
            "display" => {
                self.hidden = match value {
                    "none" => true,
                    "block" => false,
                    _ => return Err("display must be none or block"),
                }
            }
            _ => return Err("unsupported native control placement property"),
        }
        Ok(())
    }

    pub fn to_json(self) -> Value {
        json!({"left": self.x, "top": self.y, "bottom": self.bottom, "hidden": self.hidden, "width": self.width, "height": self.height,
            "direction": if self.direction == Direction::Horizontal { "horizontal" } else { "vertical" }})
    }

    pub fn from_json(value: &Value, warnings: &mut Vec<String>) -> Self {
        let mut result = Self::default();
        let Some(properties) = value.as_object().filter(|p| p.len() <= 7) else {
            warnings.push("control placement must be an object of at most seven properties".into());
            return result;
        };
        for (key, value) in properties {
            if matches!(key.as_str(), "width" | "height" | "bottom") && value.is_null() {
                continue;
            }
            if key == "hidden" {
                if let Some(hidden) = value.as_bool() {
                    result.hidden = hidden;
                } else {
                    warnings.push("control hidden must be boolean".into());
                }
                continue;
            }
            let (key, text) = if key == "direction" {
                ("--suhina-direction", value.as_str().unwrap_or("").to_owned())
            } else {
                (key.as_str(), value.as_i64().map_or(String::new(), |v| format!("{v}px")))
            };
            if let Err(error) = result.set(key, &text) {
                warnings.push(format!("control {key}: {error}"));
            }
        }
        result
    }
}

pub fn placement(name: &str) -> Placement {
    if let Some(placement) = RUNTIME.with(|runtime| {
        runtime.borrow().as_ref().and_then(|runtime| {
            runtime
                .ctx
                .data(|data| data.get_temp::<Arc<Placements>>(draft_id()))
                .map(|draft| draft.get(name).copied().unwrap_or_default())
        })
    }) {
        return placement;
    }
    super::skin::current().controls.get(name).copied().unwrap_or_default()
}

pub fn place(name: &str, native: Rect, bounds: Rect) -> Rect {
    let at = resolve(name, native, bounds);
    register(name, native, at, bounds);
    at
}

/// Resolve geometry without registering an edit handle for a part a renderer does not draw.
pub fn resolve(name: &str, native: Rect, bounds: Rect) -> Rect {
    let layout = placement(name);
    if layout == Placement::default() { native } else { layout.apply(native, bounds) }
}

fn register(name: &str, native: Rect, at: Rect, bounds: Rect) {
    RUNTIME.with(|runtime| {
        if let Some(runtime) = runtime.borrow_mut().as_mut()
            && runtime.entries.len() < 4096
            && runtime.ctx.data(|data| data.get_temp::<Arc<Placements>>(draft_id())).is_some()
            && native.is_finite()
            && at.is_finite()
            && at.is_positive()
        {
            runtime.entries.push(Entry { name: name.into(), native, at, bounds });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_names_are_stable_bounded_and_keep_distinct_non_ascii_ids() {
        assert_eq!(semantic_name("device", "enabled"), "device-enabled");
        for id in ["Ääni 🌲", "volume.1", &"large".repeat(100)] {
            let name = semantic_name("device", id);
            assert!(valid_name(&name));
            assert_eq!(name, semantic_name("device", id));
        }
        assert_ne!(semantic_name("device", "volume.1"), semantic_name("device", "volume-1"));
    }

    #[test]
    fn placement_is_bounded_and_round_trips_without_changing_the_native_default() {
        let native = Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(20.0, 60.0));
        let bounds = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(100.0));
        let mut p = Placement::default();
        assert_eq!(p.apply(native, bounds), native);
        assert!(p.set("width", "NaNpx").is_err());
        assert!(p.set("left", "2147483648px").is_err());
        assert!(p.set("--suhina-direction", "diagonal").is_err());
        assert!(p.set("width", "4096px").is_ok());
        assert!(p.set("left", "-16384px").is_ok());
        let placed = p.apply(native, bounds);
        assert!(bounds.contains_rect(placed));
        assert_eq!(placed.width(), 100.0);
        let mut warnings = Vec::new();
        assert_eq!(Placement::from_json(&p.to_json(), &mut warnings), p);
        assert!(warnings.is_empty());
    }
}
