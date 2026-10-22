// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Data-only theme skins. Interaction stays with the original widgets.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Color32, Painter, Rect};
use serde_json::{Value, json};

pub const MAX_SVG: usize = 128 * 1024;
pub const MAX_ASSETS: usize = 64;
pub const MAX_PNG: usize = 8 * 1024 * 1024;
pub const MODULES: &[&str] = &[
    "browser",
    "tempo",
    "signature",
    "grid",
    "count-in",
    "metronome",
    "transport",
    "position",
    "mapping",
    "performance",
    "views",
    "agents",
];
pub const ROLES: &[&str] =
    &["fader.cap", "fader.rail", "knob.body", "knob.pointer", "button", "play", "stop", "record", "solo", "arm", "cue"];

/// The very same original SVG sources are embedded and distributed as editable files.
pub const DEFAULT_SVGS: &[(&str, &str)] = &[
    ("fader.cap", include_str!("../../../../assets/themes/default/fader.cap.svg")),
    ("fader.rail", include_str!("../../../../assets/themes/default/fader.rail.svg")),
    ("knob.body", include_str!("../../../../assets/themes/default/knob.body.svg")),
    ("knob.pointer", include_str!("../../../../assets/themes/default/knob.pointer.svg")),
    ("button", include_str!("../../../../assets/themes/default/button.svg")),
    ("play", include_str!("../../../../assets/themes/default/play.svg")),
    ("stop", include_str!("../../../../assets/themes/default/stop.svg")),
    ("record", include_str!("../../../../assets/themes/default/record.svg")),
    ("solo", include_str!("../../../../assets/themes/default/solo.svg")),
    ("arm", include_str!("../../../../assets/themes/default/arm.svg")),
    ("cue", include_str!("../../../../assets/themes/default/cue.svg")),
];

pub fn default_assets() -> BTreeMap<String, String> {
    DEFAULT_SVGS.iter().map(|(role, svg)| ((*role).into(), (*svg).into())).collect()
}

pub fn inherited_assets() -> BTreeMap<String, String> {
    default_assets()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MeterStyle {
    #[default]
    Continuous,
    Segmented,
    Needle,
}
impl MeterStyle {
    pub fn name(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Segmented => "segmented",
            Self::Needle => "needle",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolbar {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}
impl Default for Toolbar {
    fn default() -> Self {
        Self { order: MODULES.iter().map(|s| (*s).into()).collect(), hidden: Vec::new() }
    }
}
impl Toolbar {
    pub fn move_before(&mut self, from: &str, to: &str) {
        if from == to || !self.order.iter().any(|s| s == to) {
            return;
        }
        if let Some(index) = self.order.iter().position(|s| s == from) {
            let item = self.order.remove(index);
            if let Some(index) = self.order.iter().position(|s| s == to) {
                self.order.insert(index, item);
            }
        }
    }
    pub fn move_after(&mut self, from: &str, to: &str) {
        if from == to || !self.order.iter().any(|item| item == to) {
            return;
        }
        if let Some(index) = self.order.iter().position(|item| item == from) {
            let item = self.order.remove(index);
            if let Some(index) = self.order.iter().position(|item| item == to) {
                self.order.insert(index + 1, item);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skin {
    /// Authored modern CSS, preserved losslessly alongside the native JSON projection.
    pub css: Option<Arc<str>>,
    /// A portable PNG data URL, or a package-relative PNG path before resource resolution.
    pub background: Option<Arc<str>>,
    pub background_opacity: u8,
    pub background_blur: u8,
    /// Inline SVG, or a relative path resolved by the desktop's theme file loader.
    pub assets: BTreeMap<String, String>,
    pub styles: BTreeMap<String, BTreeMap<String, String>>,
    pub meter: MeterStyle,
    pub segments: u8,
    /// None preserves the original centred control-bar layout.
    pub toolbar: Option<Toolbar>,
    /// Horizontal offsets from each module's native/CSS anchor, in logical points.
    pub toolbar_offsets: BTreeMap<String, i32>,
}
impl Default for Skin {
    fn default() -> Self {
        Self {
            css: None,
            background: None,
            background_opacity: 70,
            background_blur: 0,
            assets: BTreeMap::new(),
            styles: BTreeMap::new(),
            meter: MeterStyle::Continuous,
            segments: 24,
            toolbar: None,
            toolbar_offsets: BTreeMap::new(),
        }
    }
}

pub fn valid_role(role: &str) -> bool {
    ROLES
        .iter()
        .any(|base| role == *base || ["hover", "active", "disabled"].iter().any(|s| role == format!("{base}.{s}")))
}

/// A lexical portable path; filesystem readers also reject symlinks component by component.
pub fn asset_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|p| !p.is_empty() && p != "." && p != "..")
        && path.ends_with(".svg")
}

pub fn parse_svg(source: &str) -> Result<resvg::usvg::Tree, String> {
    if source.len() > MAX_SVG {
        return Err("SVG exceeds 128 KiB".into());
    }
    // Resource resolvers below are disabled as well. Reject these explicitly so authors get a useful error.
    let lower = source.to_ascii_lowercase();
    if ["<!doctype", "<!entity", "<script", "<image", "<foreignobject", "href=", "href ="]
        .iter()
        .any(|s| lower.contains(s))
    {
        return Err("SVG must contain only self-contained vector artwork".into());
    }
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    resvg::usvg::Tree::from_str(source, &options).map_err(|e| format!("invalid SVG: {e}"))
}

impl Skin {
    pub fn from_json(value: &Value, warnings: &mut Vec<String>) -> Self {
        let mut skin = Self::default();
        let Some(root) = value.as_object() else {
            warnings.push("skin must be an object".into());
            return skin;
        };
        for key in root.keys() {
            if !matches!(
                key.as_str(),
                "css"
                    | "background"
                    | "background-opacity"
                    | "background-blur"
                    | "styles"
                    | "assets"
                    | "meter"
                    | "segments"
                    | "toolbar"
                    | "toolbar-offsets"
            ) {
                warnings.push(format!("skin.{key} is unknown"));
            }
        }
        if let Some(value) = root.get("css") {
            if let Some(css) = value.as_str().filter(|css| css.len() <= super::theme_css::MAX_CSS) {
                match super::native_css::inspect(css) {
                    Ok(_) => skin.css = (!css.is_empty()).then(|| Arc::from(css)),
                    Err(error) => warnings.push(error),
                }
            } else {
                warnings.push("skin.css must be a stylesheet of at most 128 KiB".into());
            }
        }
        if let Some(value) = root.get("background") {
            if let Some(source) = value.as_str() {
                if source.starts_with("data:image/png;base64,") {
                    if decode_png(source).is_ok() {
                        skin.background = Some(Arc::from(source));
                    } else {
                        warnings.push("invalid embedded background PNG".into());
                    }
                } else if asset_path(&source.replace(".png", ".svg")) && source.ends_with(".png") {
                    skin.background = Some(Arc::from(source));
                } else {
                    warnings.push("background must be a relative PNG path or embedded PNG".into());
                }
            } else {
                warnings.push("background must be a PNG path".into());
            }
        }
        for (key, slot) in
            [("background-opacity", &mut skin.background_opacity), ("background-blur", &mut skin.background_blur)]
        {
            if let Some(value) = root.get(key) {
                if let Some(n) = value.as_u64().filter(|n| *n <= 100) {
                    *slot = n as u8;
                } else {
                    warnings.push(format!("skin.{key} must be 0..100"));
                }
            }
        }
        if let Some(value) = root.get("styles") {
            if let Some(styles) = value.as_object().filter(|v| v.len() <= 128) {
                for (selector, properties) in styles {
                    if !super::style_class::valid_selector(selector) {
                        warnings.push(format!("unknown CSS class {selector}"));
                        continue;
                    }
                    if let Some(properties) = properties.as_object().filter(|v| v.len() <= 8) {
                        for (key, value) in properties {
                            if let Some(value) = value.as_str().filter(|v| super::style_class::valid_property(key, v)) {
                                skin.styles.entry(selector.clone()).or_default().insert(key.clone(), value.into());
                            } else {
                                warnings.push(format!("invalid style {selector} {key}"));
                            }
                        }
                    } else {
                        warnings.push(format!("{selector}: styles must be a bounded object"));
                    }
                }
            } else {
                warnings.push("skin.styles must be a bounded object".into());
            }
        }
        if let Some(value) = root.get("assets") {
            if let Some(assets) = value.as_object().filter(|a| a.len() <= MAX_ASSETS) {
                for (role, value) in assets {
                    let Some(source) = value.as_str() else {
                        warnings.push(format!("skin.assets.{role} must be SVG or a path"));
                        continue;
                    };
                    if !valid_role(role) {
                        warnings.push(format!("unknown skin asset {role}"));
                        continue;
                    }
                    if source.trim_start().starts_with('<') {
                        if let Err(e) = parse_svg(source) {
                            warnings.push(format!("{role}: {e}"));
                            continue;
                        }
                    } else if !asset_path(source) {
                        warnings.push(format!("{role}: invalid relative SVG path"));
                        continue;
                    }
                    skin.assets.insert(role.clone(), source.to_string());
                }
            } else {
                warnings.push("skin.assets must be an object of at most 64 SVGs".into());
            }
        }
        if let Some(style) = root.get("meter") {
            skin.meter = match style.as_str() {
                Some("continuous") => MeterStyle::Continuous,
                Some("segmented") => MeterStyle::Segmented,
                Some("needle") => MeterStyle::Needle,
                _ => {
                    warnings.push("skin.meter must be continuous, segmented or needle".into());
                    MeterStyle::Continuous
                }
            };
        }
        if let Some(value) = root.get("segments") {
            if let Some(n) = value.as_u64().filter(|n| (4..=64).contains(n)) {
                skin.segments = n as u8;
            } else {
                warnings.push("skin.segments must be an integer from 4 to 64".into());
            }
        }
        if let Some(value) = root.get("toolbar") {
            if let Some(root) = value.as_object() {
                let mut toolbar = Toolbar { order: Vec::new(), hidden: Vec::new() };
                for (key, list) in [("order", &mut toolbar.order), ("hidden", &mut toolbar.hidden)] {
                    if let Some(value) = root.get(key) {
                        if let Some(items) = value.as_array().filter(|v| v.len() <= MODULES.len()) {
                            for item in items {
                                match item.as_str().filter(|s| MODULES.contains(s)) {
                                    Some(s) if !list.iter().any(|v| v == s) => list.push(s.to_string()),
                                    _ => warnings.push(format!("skin.toolbar.{key}: unknown or duplicate module")),
                                }
                            }
                        } else {
                            warnings.push(format!("skin.toolbar.{key} must be a bounded list of modules"));
                        }
                    }
                }
                for module in MODULES {
                    if !toolbar.order.iter().any(|s| s == module) {
                        toolbar.order.push((*module).into());
                    }
                }
                skin.toolbar = Some(toolbar);
            } else {
                warnings.push("skin.toolbar must be an object".into());
            }
        }
        if let Some(value) = root.get("toolbar-offsets") {
            if let Some(offsets) = value.as_object().filter(|v| v.len() <= MODULES.len()) {
                for (name, value) in offsets {
                    if let Some(offset) =
                        value.as_i64().filter(|n| MODULES.contains(&name.as_str()) && (-16384..=16384).contains(n))
                    {
                        skin.toolbar_offsets.insert(name.clone(), offset as i32);
                    } else {
                        warnings.push(format!(
                            "skin.toolbar-offsets.{name}: expected a known module and integer from -16384 to 16384"
                        ));
                    }
                }
            } else {
                warnings.push("skin.toolbar-offsets must be a bounded object".into());
            }
        }
        skin
    }

    pub fn to_json(&self) -> Value {
        let mut value =
            json!({"styles":self.styles,"assets":self.assets,"meter":self.meter.name(),"segments":self.segments});
        if let Some(css) = &self.css {
            value["css"] = Value::String(css.to_string());
        }
        if let Some(toolbar) = &self.toolbar {
            value["toolbar"] = json!({"order": toolbar.order, "hidden": toolbar.hidden});
        }
        if !self.toolbar_offsets.is_empty() {
            value["toolbar-offsets"] = json!(self.toolbar_offsets);
        }
        if let Some(background) = &self.background {
            value["background"] = Value::String(background.to_string());
        }
        value["background-opacity"] = json!(self.background_opacity);
        value["background-blur"] = json!(self.background_blur);
        value
    }
}

thread_local! { static ACTIVE: RefCell<Arc<Skin>> = RefCell::new(Arc::new(Skin::default())); }
pub fn configure(skin: &Skin) {
    ACTIVE.with(|active| {
        if active.borrow().as_ref() != skin {
            *active.borrow_mut() = Arc::new(skin.clone());
        }
    });
}

pub fn active_toolbar_offset(module: &str) -> i32 {
    ACTIVE.with(|skin| skin.borrow().toolbar_offsets.get(module).copied().unwrap_or(0))
}
pub fn current() -> Arc<Skin> {
    ACTIVE.with(|skin| Arc::clone(&skin.borrow()))
}

pub fn decode_png(source: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let data = source.strip_prefix("data:image/png;base64,").ok_or("not an embedded PNG")?;
    if data.len() > MAX_PNG * 4 / 3 + 4 {
        return Err("PNG exceeds 8 MiB".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PNG || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("invalid background PNG".into());
    }
    Ok(bytes)
}
pub fn encode_png(bytes: &[u8]) -> Result<Arc<str>, String> {
    use base64::Engine;
    if bytes.len() > MAX_PNG || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("invalid or oversized background PNG".into());
    }
    Ok(Arc::from(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}

#[derive(Debug, Clone, Copy)]
pub enum State {
    Normal,
    Hover,
    Active,
    Disabled,
}
impl State {
    pub fn response(response: &egui::Response, on: bool) -> Self {
        if !response.enabled() {
            Self::Disabled
        } else if on || response.is_pointer_button_down_on() {
            Self::Active
        } else if response.hovered() {
            Self::Hover
        } else {
            Self::Normal
        }
    }
}
#[derive(Default)]
struct Textures {
    images: HashMap<u64, Option<egui::TextureHandle>>,
}

/// Cached static vector artwork. Rotation affects only the final mesh, not the cached image.
pub fn paint(painter: &Painter, rect: Rect, role: &str, state: State, ink: Color32, angle: f32) -> bool {
    paint_inner(painter, rect, role, state, ink, angle, 0.0)
}

pub fn button(painter: &Painter, rect: Rect, state: State, fill: Color32, radius: f32) -> bool {
    paint_inner(painter, rect, "button", state, fill, 0.0, radius)
}

fn paint_inner(painter: &Painter, rect: Rect, role: &str, state: State, ink: Color32, angle: f32, radius: f32) -> bool {
    if !rect.is_positive() || !rect.is_finite() {
        return false;
    }
    let skin = current();
    let suffix = match state {
        State::Normal => "",
        State::Hover => ".hover",
        State::Active => ".active",
        State::Disabled => ".disabled",
    };
    static DEFAULTS: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    let defaults = DEFAULTS.get_or_init(default_assets);
    let key = format!("{role}{suffix}");
    let own = skin.assets.get(&key).or_else(|| skin.assets.get(role));
    let Some(source) = own.or_else(|| defaults.get(&key)).or_else(|| defaults.get(role)) else {
        return false;
    };
    // Semantic ink travels with the SVG, so exported inherited artwork retains its colour.
    let ink = if source.contains(&format!("data-suhina-ink=\"{role}\"")) {
        match role {
            "knob.body" => super::theme::control_background(),
            "fader.rail" => super::motion::mix(super::theme::control_background(), Color32::BLACK, 0.65),
            "fader.cap" => super::motion::mix(
                if matches!(state, State::Hover | State::Active) {
                    super::theme::control_hover()
                } else {
                    super::theme::control_background()
                },
                ink,
                0.28,
            ),
            _ => ink,
        }
    } else {
        ink
    };
    let scale = painter.ctx().pixels_per_point();
    let w = (rect.width() * scale).ceil().clamp(1.0, 1024.0) as u32;
    let h = (rect.height() * scale).ceil().clamp(1.0, 1024.0) as u32;
    let mut hash = std::hash::DefaultHasher::new();
    (source, ink.to_array(), w, h).hash(&mut hash);
    let key = hash.finish();
    let cache_id = egui::Id::new("skin-textures");
    let cache = painter
        .ctx()
        .data_mut(|data| data.get_temp_mut_or_default::<Arc<std::sync::Mutex<Textures>>>(cache_id).clone());
    let Ok(mut textures) = cache.lock() else {
        return false;
    };
    if !textures.images.contains_key(&key) {
        if textures.images.len() >= 64 {
            textures.images.clear();
        }
        let color = super::theme::hex_rgba(ink.to_srgba_unmultiplied());
        let source = source.replace("currentColor", &color);
        let texture = parse_svg(&source).ok().and_then(|tree| {
            let mut pixels = resvg::tiny_skia::Pixmap::new(w, h)?;
            let transform = resvg::tiny_skia::Transform::from_scale(
                w as f32 / tree.size().width(),
                h as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixels.as_mut());
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for p in pixels.pixels() {
                let p = p.demultiply();
                rgba.extend_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
            }
            let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            Some(painter.ctx().load_texture(format!("skin-{key}"), image, egui::TextureOptions::LINEAR))
        });
        textures.images.insert(key, texture);
    }
    let Some(Some(texture)) = textures.images.get(&key) else {
        return false;
    };
    if role == "button" {
        painter.add(
            egui::epaint::RectShape::filled(rect, super::style_class::radius(radius), Color32::WHITE)
                .with_texture(texture.id(), Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)))
                .with_angle(angle),
        );
        return true;
    }
    let mut mesh = egui::Mesh::with_texture(texture.id());
    mesh.add_rect_with_uv(rect, Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
    mesh.rotate(egui::emath::Rot2::from_angle(angle), rect.center());
    painter.add(egui::Shape::mesh(mesh));
    true
}

/// Original editable artwork supplied with the theme template.
pub fn example_assets() -> BTreeMap<String, String> {
    [
        ("fader.cap", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 26"><rect x="1" y="1" width="16" height="24" rx="3" fill="#555b62" stroke="currentColor"/><path d="M3 13h12" stroke="currentColor" stroke-width="2"/></svg>"##),
        ("fader.rail", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 5 100"><rect width="5" height="100" rx="2" fill="#11151a"/></svg>"##),
        ("knob.body", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#30363d" stroke="currentColor"/></svg>"##),
        ("knob.pointer", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 4v7" stroke="currentColor" stroke-width="2"/></svg>"##),
        ("button", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="1" y="1" width="22" height="22" rx="4" fill="#30363d" stroke="currentColor"/></svg>"##),
        ("play", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M8 5l11 7L8 19z" fill="currentColor"/></svg>"##),
        ("stop", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" fill="currentColor"/></svg>"##),
        ("record", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="7" fill="currentColor"/></svg>"##),
        ("solo", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M17 7H10a3 3 0 0 0 0 6h4a3 3 0 0 1 0 6H7" fill="none" stroke="currentColor" stroke-width="2"/></svg>"##),
        ("arm", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="6" fill="none" stroke="currentColor" stroke-width="2"/></svg>"##),
        ("cue", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 14v-3a7 7 0 0 1 14 0v3M5 13v6M19 13v6" fill="none" stroke="currentColor" stroke-width="3"/></svg>"##),
    ].into_iter().map(|(k,v)| (k.to_string(), v.to_string())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_svg_renders_missing_roles_while_theme_artwork_overrides_one_role() {
        assert_eq!(DEFAULT_SVGS.len(), ROLES.len());
        for (role, svg) in DEFAULT_SVGS {
            assert!(parse_svg(svg).is_ok(), "{role}");
        }
        let skin = Skin {assets:BTreeMap::from([("play".into(), "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"28\" height=\"28\"><rect width=\"28\" height=\"28\" fill=\"#00ff00\"/></svg>".into())]), ..Default::default()};
        configure(&skin);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(28.0));
            assert!(paint(ui.painter(), rect, "button", State::Normal, Color32::BLUE, 0.0));
            assert!(paint(ui.painter(), rect, "play", State::Normal, Color32::RED, 0.0));
            assert!(paint(ui.painter(), rect, "stop", State::Normal, Color32::RED, 0.0));
        });
        let pixels: Vec<_> = output
            .textures_delta
            .set
            .values()
            .flat_map(|deltas| deltas.iter())
            .map(|delta| match &delta.image {
                egui::ImageData::Color(image) => image.pixels[14 * 28 + 14],
            })
            .collect();
        for color in [Color32::BLUE, Color32::GREEN, Color32::RED] {
            assert!(pixels.contains(&color));
        }
        assert_eq!(skin.assets.len(), 1, "inheritance must not populate the theme's overrides");
        output.textures_delta.clear();
        configure(&Skin::default());
    }
    #[test]
    fn svg_states_publish_distinct_pixels_and_reuse_cached_textures() {
        let artwork = |color: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"><rect width=\"16\" height=\"16\" fill=\"{color}\"/></svg>"
            )
        };
        let skin = Skin {
            assets: [("button".into(), artwork("#FF0000")), ("button.active".into(), artwork("#00FF00"))].into(),
            ..Default::default()
        };
        configure(&skin);
        let ctx = egui::Context::default();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(16.0));
            assert!(paint(ui.painter(), rect, "button", State::Normal, Color32::WHITE, 0.0));
            assert!(paint(ui.painter(), rect, "button", State::Normal, Color32::WHITE, 0.0));
            assert!(paint(ui.painter(), rect, "button", State::Active, Color32::WHITE, 0.0));
            assert!(!paint(ui.painter(), rect, "absent", State::Normal, Color32::WHITE, 0.0));
        });
        let images: Vec<_> = frame
            .textures_delta
            .set
            .values()
            .flat_map(|deltas| deltas.iter())
            .filter(|delta| delta.image.size() == [16, 16])
            .collect();
        assert_eq!(images.len(), 2, "a repeated state should reuse its texture");
        let pixels: Vec<_> = images
            .iter()
            .map(|delta| match &delta.image {
                egui::ImageData::Color(image) => image.pixels[8 * 16 + 8],
            })
            .collect();
        assert!(pixels.contains(&Color32::RED));
        assert!(pixels.contains(&Color32::GREEN));
        frame.textures_delta.clear();
        configure(&Skin::default());
    }
    #[test]
    fn skin_round_trip_and_old_defaults() {
        let skin = Skin {
            css: Some(Arc::from("/* preserved */ @layer test { .browser { color:var(--ink); } }")),
            background: None,
            background_opacity: 70,
            background_blur: 0,
            styles: Default::default(),
            assets: example_assets(),
            meter: MeterStyle::Segmented,
            segments: 16,
            toolbar: Some(Toolbar::default()),
            toolbar_offsets: BTreeMap::from([("transport".into(), 80)]),
        };
        let mut warnings = Vec::new();
        assert_eq!(Skin::from_json(&skin.to_json(), &mut warnings), skin);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(Skin::from_json(&json!({}), &mut warnings), Skin::default());
    }
    #[test]
    fn malformed_assets_and_layout_are_bounded() {
        let mut warnings = Vec::new();
        let skin = Skin::from_json(
            &json!({"assets":{"fader.cap":"../secret.svg", "play":"<svg>"},"segments":999,"meter":"oops","toolbar":{"order":["tempo","tempo","bogus"]}}),
            &mut warnings,
        );
        assert!(skin.assets.is_empty());
        assert_eq!(skin.segments, 24);
        assert_eq!(
            skin.toolbar.unwrap().order,
            Toolbar::default().order.into_iter().filter(|s| s != "tempo").fold(vec!["tempo".into()], |mut v, s| {
                v.push(s);
                v
            })
        );
        assert_eq!(warnings.len(), 6);
        for path in ["/tmp/a.svg", "../a.svg", "a/../b.svg", "C:/a.svg", "a\\b.svg", "a//b.svg"] {
            assert!(!asset_path(path));
        }
        for svg in ["<!DOCTYPE svg><svg/>", "<svg><image href=\"file:///tmp/x\"/></svg>"] {
            assert!(parse_svg(svg).is_err());
        }
    }

    #[test]
    fn toolbar_offsets_reject_unknown_modules_and_non_integer_or_unbounded_values() {
        let mut warnings = Vec::new();
        let skin = Skin::from_json(
            &json!({"toolbar-offsets":{"transport":80,"tempo":0.5,"views":16385,"bogus":4}}),
            &mut warnings,
        );
        assert_eq!(skin.toolbar_offsets, BTreeMap::from([("transport".into(), 80)]));
        assert_eq!(warnings.len(), 3);
        assert!(skin.toolbar.is_none());
    }
    #[test]
    fn moving_module_preserves_all_others() {
        let mut toolbar = Toolbar::default();
        toolbar.move_before("views", "tempo");
        assert_eq!(toolbar.order[1], "views");
        assert_eq!(toolbar.order.len(), MODULES.len());
        let before = toolbar.clone();
        toolbar.move_before("views", "views");
        toolbar.move_before("nope", "tempo");
        assert_eq!(toolbar, before);
        toolbar.move_after("browser", "agents");
        assert_eq!(toolbar.order.last().map(String::as_str), Some("browser"));
        assert_eq!(toolbar.order.len(), MODULES.len());
    }
}
