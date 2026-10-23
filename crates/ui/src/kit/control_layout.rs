// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Theme-owned native control placement. Paint and interaction use the same rectangle.
use egui::{Rect, Vec2};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use suhina_style::anchor::{Anchor, Anchors, Edge, Name};

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

/// Most anchor sources and targets remembered per frame.
const MAX_PLACED: usize = 4096;

/// Where an element that takes part in an anchor was placed, and in which container.
#[derive(Clone, Copy)]
struct Placed {
    name: Name,
    bounds: Rect,
    at: Rect,
}

/// An anchored edge resolved from the previous frame's target: checked when the frame ends.
#[derive(Clone, Copy)]
struct Borrowed {
    target: Name,
    bounds: Rect,
    used: Rect,
}

/// This frame's and the previous frame's anchor sources and targets. Elements are drawn in
/// immediate mode, so a target drawn after its source is taken from the previous frame, moved
/// as far as the source's container moved (scrolling), and the frame repaints once if the
/// target has since changed. Only active between [`Capture::begin`] and its drop.
#[derive(Default)]
struct Frames {
    active: bool,
    current: Vec<Placed>,
    previous: Vec<Placed>,
    borrowed: Vec<Borrowed>,
    /// Names of anchor targets and anchored elements, for the placements kept alive beside them
    /// (so their address cannot be reused by other placements).
    names: Option<(Source, Arc<std::collections::BTreeSet<Name>>)>,
}
thread_local! { static FRAMES: RefCell<Frames> = RefCell::new(Frames::default()); }

fn overlap(a: Rect, b: Rect) -> f32 {
    let both = a.intersect(b);
    if both.is_positive() { both.area() } else { 0.0 }
}

/// The record named `name` whose container overlaps `bounds` the most.
fn best(records: &[Placed], name: Name, bounds: Rect) -> Option<Placed> {
    records
        .iter()
        .filter(|p| p.name == name)
        .map(|p| (overlap(p.bounds, bounds), p))
        .filter(|(area, _)| *area > 0.0)
        .fold(None, |best: Option<(f32, &Placed)>, next| match best {
            Some(b) if b.0 >= next.0 => Some(b),
            _ => Some(next),
        })
        .map(|(_, p)| *p)
}

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
        FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            frames.previous = std::mem::take(&mut frames.current);
            frames.borrowed.clear();
            frames.active = true;
        });
        Self(RUNTIME.with(|runtime| runtime.replace(Some(Runtime { ctx: ctx.clone(), entries: Vec::new() }))))
    }
    pub fn entries(&self) -> Vec<Entry> {
        RUNTIME.with(|runtime| runtime.borrow().as_ref().map_or_else(Vec::new, |runtime| runtime.entries.clone()))
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let changed = FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            frames.active = false;
            // A target taken from the last frame that has moved since: one more frame settles it.
            frames.borrowed.iter().any(|b| {
                best(&frames.current, b.target, b.bounds).is_some_and(|now| {
                    (now.at.min - b.used.min).length() > 0.01 || (now.at.max - b.used.max).length() > 0.01
                })
            })
        });
        RUNTIME.with(|runtime| {
            if changed && let Some(runtime) = runtime.borrow().as_ref() {
                runtime.ctx.request_repaint();
            }
            runtime.replace(self.0.take())
        });
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
    /// Fractional offset in 1/1024 logical pixels, retained by CSS/JSON round trips.
    pub subpixel_x: i16,
    pub subpixel_y: i16,
    pub bottom: Option<i32>,
    pub right: Option<i32>,
    pub hidden: bool,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub full_height: bool,
    pub full_width: bool,
    pub direction: Direction,
    /// Edges that follow an edge of another element in the same container.
    pub anchors: Anchors,
}

pub fn valid_name(name: &str) -> bool {
    suhina_style::anchor::valid_name(name)
}

/// The CSS property that anchors `edge`.
pub fn anchor_property(edge: Edge) -> &'static str {
    match edge {
        Edge::Left => "--suhina-anchor-left",
        Edge::Top => "--suhina-anchor-top",
        Edge::Right => "--suhina-anchor-right",
        Edge::Bottom => "--suhina-anchor-bottom",
    }
}

/// Drops the anchors that would form a cycle or a chain longer than eight links, with a warning
/// for each element. Every theme and editor draft passes through this before it is used.
pub fn validate_anchors(controls: &mut Placements, warnings: &mut Vec<String>) {
    let rejected = suhina_style::anchor::rejected(controls.iter().map(|(name, p)| (name.as_str(), p.anchors)));
    for (name, reason) in rejected {
        if let Some(placement) = controls.get_mut(&name) {
            placement.anchors = Anchors::default();
            warnings.push(format!("control {name}: anchors ignored, {reason}"));
        }
    }
}

impl Placement {
    pub fn offset(self) -> Vec2 {
        Vec2::new(
            self.x as f32 + f32::from(self.subpixel_x) / 1024.0,
            self.y as f32 + f32::from(self.subpixel_y) / 1024.0,
        )
    }
    pub fn set_offset(&mut self, offset: Vec2) {
        let split = |v: f32| {
            let v = if v.is_finite() { v.clamp(-16384.0, 16384.0) } else { 0.0 };
            let units = (v * 1024.0).round() as i32;
            (units / 1024, (units % 1024) as i16)
        };
        (self.x, self.subpixel_x) = split(offset.x);
        (self.y, self.subpixel_y) = split(offset.y);
    }

    /// Explicit relative placement, in logical points; no stylesheet can escape its native panel.
    pub fn apply(self, native: Rect, bounds: Rect) -> Rect {
        if !native.is_finite() || !bounds.is_finite() || !bounds.is_positive() {
            return native;
        }
        let mut size =
            Vec2::new(self.width.map_or(native.width(), f32::from), self.height.map_or(native.height(), f32::from))
                .max(Vec2::splat(1.0))
                .min(bounds.size());
        let mut anchor = native.min + self.offset();
        if self.full_width {
            let left = self.offset().x.clamp(0.0, bounds.width());
            let right = (self.right.unwrap_or(0) as f32).clamp(0.0, bounds.width() - left);
            anchor.x = bounds.left() + left;
            size.x = (bounds.width() - left - right).max(0.0);
        } else if let Some(right) = self.right {
            anchor.x = bounds.right() - size.x - right as f32;
        }
        if self.full_height {
            let top = self.offset().y.clamp(0.0, bounds.height());
            let bottom = (self.bottom.unwrap_or(0) as f32).clamp(0.0, bounds.height() - top);
            anchor.y = bounds.top() + top;
            size.y = (bounds.height() - top - bottom).max(0.0);
        } else if let Some(bottom) = self.bottom {
            anchor.y = bounds.bottom() - size.y - bottom as f32;
        }
        let min = anchor.max(bounds.min).min(bounds.max - size);
        Rect::from_min_size(min, size)
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), &'static str> {
        if let Some(edge) = key.strip_prefix("--suhina-anchor-").and_then(Edge::parse) {
            let anchor = if value == "none" { None } else { Some(Anchor::parse(edge, value)?) };
            self.anchors.set(edge, anchor);
            return Ok(());
        }
        match key {
            "left" | "top" | "bottom" | "right" => {
                if matches!(key, "bottom" | "right") && value == "auto" {
                    if key == "bottom" {
                        self.bottom = None;
                    } else {
                        self.right = None;
                    }
                    return Ok(());
                }
                if !matches!(key, "bottom" | "right") {
                    let n = value
                        .strip_suffix("px")
                        .and_then(|v| v.parse::<f32>().ok())
                        .filter(|v| v.is_finite() && (-16384.0..=16384.0).contains(v))
                        .ok_or("offset must be finite and within -16384..16384px")?;
                    let mut offset = self.offset();
                    if key == "left" {
                        offset.x = n;
                    } else {
                        offset.y = n;
                    }
                    self.set_offset(offset);
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
                    "bottom" => self.bottom = Some(n),
                    _ => self.right = Some(n),
                }
            }
            "width" | "height" => {
                if key == "width" && value == "100%" {
                    self.full_width = true;
                    self.width = None;
                    return Ok(());
                }
                if key == "height" && value == "100%" {
                    self.full_height = true;
                    self.height = None;
                    return Ok(());
                }
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
                    self.full_width = false;
                } else {
                    self.height = n;
                    self.full_height = false;
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
        let mut value = self.to_json_without_anchors();
        if !self.anchors.is_empty()
            && let Some(object) = value.as_object_mut()
        {
            let anchors: serde_json::Map<String, Value> =
                self.anchors.iter().map(|(edge, anchor)| (edge.name().to_owned(), json!(anchor.to_string()))).collect();
            object.insert("anchors".into(), Value::Object(anchors));
        }
        value
    }

    fn to_json_without_anchors(self) -> Value {
        json!({"left": self.offset().x, "top": self.offset().y, "bottom": self.bottom, "right": self.right, "hidden": self.hidden, "width": if self.full_width { json!("100%") } else { json!(self.width) }, "height": if self.full_height { json!("100%") } else { json!(self.height) },
            "direction": if self.direction == Direction::Horizontal { "horizontal" } else { "vertical" }})
    }

    pub fn from_json(value: &Value, warnings: &mut Vec<String>) -> Self {
        let mut result = Self::default();
        let Some(properties) = value.as_object().filter(|p| p.len() <= 9) else {
            warnings.push("control placement must be an object of at most nine properties".into());
            return result;
        };
        for (key, value) in properties {
            if key == "anchors" {
                let Some(anchors) = value.as_object().filter(|a| a.len() <= 4) else {
                    warnings.push("control anchors must be an object of at most four edges".into());
                    continue;
                };
                for (edge, anchor) in anchors {
                    let property = format!("--suhina-anchor-{edge}");
                    if let Err(error) = result.set(&property, anchor.as_str().unwrap_or("")) {
                        warnings.push(format!("control anchors.{edge}: {error}"));
                    }
                }
                continue;
            }
            if matches!(key.as_str(), "width" | "height" | "bottom" | "right") && value.is_null() {
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
            let (key, text) = if matches!(key.as_str(), "width" | "height") && value.as_str() == Some("100%") {
                (key.as_str(), "100%".to_owned())
            } else if key == "direction" {
                ("--suhina-direction", value.as_str().unwrap_or("").to_owned())
            } else {
                (key.as_str(), value.as_f64().map_or(String::new(), |v| format!("{v}px")))
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
    let mut at = if layout == Placement::default() { native } else { layout.apply(native, bounds) };
    if !layout.anchors.is_empty() && !layout.hidden {
        at = anchored(name, layout, at, bounds);
    }
    remember(name, bounds, at);
    at
}

/// The placements in effect: the editor's draft while editing, else the theme's.
#[derive(Clone)]
enum Source {
    Draft(Arc<Placements>),
    Skin(Arc<super::skin::Skin>),
}

impl Source {
    fn current() -> Self {
        let draft = RUNTIME.with(|runtime| {
            runtime
                .borrow()
                .as_ref()
                .and_then(|runtime| runtime.ctx.data(|data| data.get_temp::<Arc<Placements>>(draft_id())))
        });
        draft.map_or_else(|| Self::Skin(super::skin::current()), Self::Draft)
    }
    fn placements(&self) -> &Placements {
        match self {
            Self::Draft(draft) => draft,
            Self::Skin(skin) => &skin.controls,
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Draft(a), Self::Draft(b)) => Arc::ptr_eq(a, b),
            (Self::Skin(a), Self::Skin(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Names of every anchored element and anchor target, computed once per placement map.
fn anchor_names() -> Arc<std::collections::BTreeSet<Name>> {
    let source = Source::current();
    if let Some(names) = FRAMES.with(|frames| {
        frames.borrow().names.as_ref().filter(|(cached, _)| cached.same(&source)).map(|(_, names)| Arc::clone(names))
    }) {
        return names;
    }
    let names: std::collections::BTreeSet<Name> = source
        .placements()
        .iter()
        .filter(|(_, p)| !p.anchors.is_empty())
        .flat_map(|(name, p)| Name::new(name).into_iter().chain(p.anchors.iter().map(|(_, a)| a.target)))
        .collect();
    let names = Arc::new(names);
    FRAMES.with(|frames| frames.borrow_mut().names = Some((source, Arc::clone(&names))));
    names
}

fn remember(name: &str, bounds: Rect, at: Rect) {
    let Some(name) = Name::new(name) else { return };
    if !bounds.is_finite() || !at.is_finite() || !anchor_names().contains(&name) {
        return;
    }
    FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        if frames.active && frames.current.len() < MAX_PLACED {
            frames.current.push(Placed { name, bounds, at });
        }
    });
}

/// Where `target` is in the container `bounds` of an element named `source`: drawn earlier this
/// frame, or last frame moved as far as the source's container moved. `None` when the target is
/// hidden, missing or in another container: the anchored edge then keeps its own placement.
fn target_rect(source: Name, target: Name, bounds: Rect) -> Option<Rect> {
    if placement(target.as_str()).hidden {
        return None;
    }
    FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        if !frames.active {
            return None;
        }
        if let Some(now) = best(&frames.current, target, bounds) {
            return Some(now.at);
        }
        // The same source last frame: the record of the same size nearest to its container now.
        let before = frames
            .previous
            .iter()
            .filter(|p| p.name == source && (p.bounds.size() - bounds.size()).length() < 0.5)
            .min_by(|a, b| {
                let da = (a.bounds.min - bounds.min).length();
                let db = (b.bounds.min - bounds.min).length();
                da.total_cmp(&db)
            })
            .copied()?;
        let then = best(&frames.previous, target, before.bounds)?;
        let used = then.at.translate(bounds.min - before.bounds.min);
        if frames.borrowed.len() < MAX_PLACED {
            frames.borrowed.push(Borrowed { target, bounds, used });
        }
        Some(used)
    })
}

/// `at` with its anchored edges moved to their targets, inside `bounds`.
fn anchored(name: &str, layout: Placement, at: Rect, bounds: Rect) -> Rect {
    let Some(source) = Name::new(name) else { return at };
    if !bounds.is_finite() || !bounds.is_positive() {
        return at;
    }
    let edge_of = |edge: Edge| -> Option<f32> {
        let anchor = layout.anchors.get(edge)?;
        let target = target_rect(source, anchor.target, bounds)?;
        let value = match anchor.edge {
            Edge::Left => target.left(),
            Edge::Top => target.top(),
            Edge::Right => target.right(),
            Edge::Bottom => target.bottom(),
        };
        Some(anchor.place(edge, value))
    };
    let (x0, x1) = suhina_style::anchor::resolve_axis(
        (at.left(), at.right()),
        edge_of(Edge::Left),
        edge_of(Edge::Right),
        layout.full_width,
        (bounds.left(), bounds.right()),
    );
    let (y0, y1) = suhina_style::anchor::resolve_axis(
        (at.top(), at.bottom()),
        edge_of(Edge::Top),
        edge_of(Edge::Bottom),
        layout.full_height,
        (bounds.top(), bounds.bottom()),
    );
    Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
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
    fn full_width_uses_left_and_right_insets_and_round_trips() {
        let mut p = Placement::default();
        p.set("width", "100%").unwrap();
        p.set("left", "4.5px").unwrap();
        p.set("right", "6px").unwrap();
        let bounds = Rect::from_min_size(egui::pos2(10.0, 20.0), Vec2::new(100.0, 80.0));
        let native = Rect::from_min_size(bounds.min + Vec2::splat(12.0), Vec2::splat(20.0));
        let at = p.apply(native, bounds);
        assert_eq!(at.left(), 14.5);
        assert_eq!(at.right(), 104.0);
        assert_eq!(at.height(), 20.0);
        let mut warnings = Vec::new();
        assert_eq!(Placement::from_json(&p.to_json(), &mut warnings), p);
        assert!(warnings.is_empty());
        p.set("width", "20px").unwrap();
        assert!(!p.full_width);
    }

    #[test]
    fn fractional_offsets_survive_storage_and_reject_nonfinite_input() {
        let mut p = Placement::default();
        p.set("left", "1.25px").unwrap();
        p.set("top", "-2.125px").unwrap();
        assert_eq!(p.offset(), Vec2::new(1.25, -2.125));
        let mut warnings = Vec::new();
        assert_eq!(Placement::from_json(&p.to_json(), &mut warnings), p);
        assert!(warnings.is_empty());
        assert!(p.set("left", "NaNpx").is_err());
        assert!(p.set("top", "infpx").is_err());
    }

    #[test]
    fn full_height_tracks_panel_edges_and_round_trips() {
        let mut p = Placement::default();
        p.set("height", "100%").unwrap();
        p.set("top", "4px").unwrap();
        p.set("bottom", "6px").unwrap();
        for height in [30.0, 200.0] {
            let bounds = Rect::from_min_size(egui::pos2(10.0, -40.0), Vec2::new(100.0, height));
            let native = Rect::from_min_size(bounds.min + Vec2::new(80.0, 8.0), Vec2::new(10.0, 12.0));
            let at = p.apply(native, bounds);
            assert_eq!(at.top(), bounds.top() + 4.0);
            assert_eq!(at.bottom(), bounds.bottom() - 6.0);
            assert_eq!(at.width(), 10.0);
        }
        let mut warnings = Vec::new();
        assert_eq!(Placement::from_json(&p.to_json(), &mut warnings), p);
        assert!(warnings.is_empty());
        p.set("height", "auto").unwrap();
        assert!(!p.full_height);
        assert!(p.set("height", "200%").is_err());
    }

    fn anchored_skin(entries: &[(&str, &[(&str, &str)])]) -> super::super::skin::Skin {
        let mut skin = super::super::skin::Skin::default();
        for (name, properties) in entries {
            let mut p = Placement::default();
            for (key, value) in *properties {
                p.set(key, value).unwrap_or_else(|e| panic!("{key}: {e}"));
            }
            skin.controls.insert((*name).into(), p);
        }
        skin
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, h))
    }

    #[test]
    fn anchors_round_trip_through_css_properties_and_json() {
        let mut p = Placement::default();
        p.set("height", "100%").unwrap();
        p.set("--suhina-anchor-bottom", "arrangement-volume top 4px").unwrap();
        p.set("--suhina-anchor-left", "arrangement-name right").unwrap();
        let mut warnings = Vec::new();
        assert_eq!(Placement::from_json(&p.to_json(), &mut warnings), p);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(p.to_json()["anchors"]["bottom"], "arrangement-volume top 4px");
        assert!(p.set("--suhina-anchor-bottom", "arrangement-volume left").is_err(), "axis must match");
        assert!(p.set("--suhina-anchor-middle", "arrangement-volume top").is_err());
        p.set("--suhina-anchor-bottom", "none").unwrap();
        p.set("--suhina-anchor-left", "none").unwrap();
        assert!(p.anchors.is_empty());
        assert!(Placement::default().to_json().get("anchors").is_none(), "no anchors, no key");
    }

    #[test]
    fn a_full_height_meter_ends_above_the_control_under_it_in_the_same_frame() {
        super::super::skin::configure(&anchored_skin(&[(
            "meter",
            &[("height", "100%"), ("top", "2px"), ("--suhina-anchor-bottom", "volume top 4px")],
        )]));
        let ctx = egui::Context::default();
        let header = rect(0.0, 100.0, 200.0, 60.0);
        let capture = Capture::begin(&ctx);
        let volume = place("volume", rect(150.0, 140.0, 40.0, 16.0), header);
        let meter = place("meter", rect(190.0, 100.0, 6.0, 60.0), header);
        drop(capture);
        assert_eq!(meter.top(), 102.0);
        assert_eq!(meter.bottom(), volume.top() - 4.0);
        assert_eq!(meter.width(), 6.0);
    }

    #[test]
    fn a_target_drawn_later_comes_from_the_last_frame_moved_with_the_container() {
        super::super::skin::configure(&anchored_skin(&[(
            "meter",
            &[("height", "100%"), ("--suhina-anchor-bottom", "volume top 4px")],
        )]));
        let ctx = egui::Context::default();
        let frame = |y: f32| {
            let header = rect(0.0, y, 200.0, 60.0);
            let _capture = Capture::begin(&ctx);
            let meter = place("meter", rect(190.0, y, 6.0, 60.0), header);
            let volume = place("volume", rect(150.0, y + 40.0, 40.0, 16.0), header);
            (meter, volume)
        };
        // First frame: nothing known yet, the meter keeps its own placement.
        let (meter, _) = frame(100.0);
        assert_eq!(meter.bottom(), 160.0);
        let (meter, volume) = frame(100.0);
        assert_eq!(meter.bottom(), volume.top() - 4.0);
        // Scrolled by 25 points: the last frame's target moves with the header.
        let (meter, volume) = frame(75.0);
        assert_eq!(meter.bottom(), volume.top() - 4.0);
    }

    #[test]
    fn hidden_missing_and_foreign_targets_fall_back_to_the_own_placement() {
        let ctx = egui::Context::default();
        let header = rect(0.0, 0.0, 200.0, 60.0);
        let native = rect(190.0, 0.0, 6.0, 60.0);
        for (volume_hidden, volume_bounds) in [(true, header), (false, rect(0.0, 300.0, 200.0, 60.0))] {
            let mut entries: Vec<(&str, &[(&str, &str)])> =
                vec![("meter", &[("height", "100%"), ("--suhina-anchor-bottom", "volume top 4px")])];
            if volume_hidden {
                entries.push(("volume", &[("display", "none")]));
            }
            super::super::skin::configure(&anchored_skin(&entries));
            // An empty frame: nothing from the previous case is remembered.
            drop(Capture::begin(&ctx));
            for _ in 0..2 {
                let _capture = Capture::begin(&ctx);
                place("volume", rect(150.0, volume_bounds.top() + 40.0, 40.0, 16.0), volume_bounds);
                assert_eq!(place("meter", native, header), native);
            }
        }
        // A target that is never drawn at all.
        super::super::skin::configure(&anchored_skin(&[("meter", &[("--suhina-anchor-bottom", "nowhere top")])]));
        let _capture = Capture::begin(&ctx);
        assert_eq!(place("meter", native, header), native);
    }

    #[test]
    fn anchors_outside_a_frame_capture_are_inert() {
        super::super::skin::configure(&anchored_skin(&[("meter", &[("--suhina-anchor-bottom", "volume top")])]));
        let header = rect(0.0, 0.0, 200.0, 60.0);
        let native = rect(190.0, 0.0, 6.0, 20.0);
        place("volume", rect(150.0, 40.0, 40.0, 16.0), header);
        assert_eq!(place("meter", native, header), native);
    }

    #[test]
    fn cyclic_anchors_are_dropped_with_a_warning() {
        let mut controls = anchored_skin(&[
            ("a", &[("--suhina-anchor-left", "b right")]),
            ("b", &[("--suhina-anchor-left", "a right")]),
            ("c", &[("--suhina-anchor-top", "a bottom 2px")]),
        ])
        .controls;
        let mut warnings = Vec::new();
        validate_anchors(&mut controls, &mut warnings);
        assert!(controls["a"].anchors.is_empty() && controls["b"].anchors.is_empty());
        assert!(!controls["c"].anchors.is_empty());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

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
