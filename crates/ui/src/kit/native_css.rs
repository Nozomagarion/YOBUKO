// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native computed styles, Taffy layout and motion, consumed by native egui controls.
//!
//! The host supplies a bounded tree; themes never supply markup or executable code.
//! Everything resolves through `suhina-style`; there is no browser engine. A sheet that does
//! not parse keeps the last working one. This module is UI-thread-only.
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Color32, Rect, Vec2};
use suhina_style::{
    motion::{self, Animation, Frames, Transition},
    property::Property,
    value::Value,
};

use super::{skin, theme};
mod diagnostics;
mod geometry;
mod native;
mod paint;
mod shadows;
mod text;
mod workspace;
pub use diagnostics::{Notice, inspect};
pub use paint::{background, background_rounded, background_with, level_fill, level_glow};
pub use text::paint_text;
pub use workspace::{Workspace, workspace};

const MAX_CONTEXTS: usize = 128;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 4096;

/// Styles exposed by native painters. Only properties a painter draws are here.
#[derive(Clone, Debug)]
pub struct Computed {
    pub color: Color32,
    pub background: Color32,
    pub border: Color32,
    pub font_size: f32,
    pub radius: f32,
    pub opacity: f32,
    pub visible: bool,
    /// `accent-color`: a control's accent, such as a knob's arc.
    pub accent: Color32,
    pub authored: Authored,
    gradients: Arc<[paint::Gradient]>,
    shadows: Arc<[shadows::Shadow]>,
    text_shadows: Arc<[text::Shadow]>,
    transitions: Arc<[Transition]>,
    animations: Arc<[(Animation, Arc<Frames>)]>,
}
impl Computed {
    pub fn has_gradient(&self) -> bool {
        !self.gradients.is_empty()
    }

    /// The value a transition or keyframe animation interpolates for `property`.
    fn animated(&self, property: Property) -> Option<Value> {
        use suhina_style::value::{Color, Length};
        let color = |c: Color32| Value::Color(Color::Rgba(c.to_srgba_unmultiplied()));
        Some(match property {
            Property::Color => color(self.color),
            Property::BackgroundColor => color(self.background),
            Property::BorderColor => color(self.border),
            Property::Opacity => Value::Number(self.opacity),
            Property::FontSize => Value::Length(Length::Px(self.font_size)),
            Property::Radius => Value::Length(Length::Px(self.radius)),
            _ => return None,
        })
    }

    fn set_animated(&mut self, property: Property, value: &Value) {
        use suhina_style::value::{Color, Length};
        let color = |v: &Value| match v {
            Value::Color(Color::Rgba([r, g, b, a])) => Some(Color32::from_rgba_unmultiplied(*r, *g, *b, *a)),
            _ => None,
        };
        let px = |v: &Value| match v {
            Value::Length(Length::Px(n)) if n.is_finite() => Some(*n),
            _ => None,
        };
        match property {
            Property::Color => {
                if let Some(c) = color(value) {
                    self.color = c;
                    self.authored.color = true;
                }
            }
            Property::BackgroundColor => {
                if let Some(c) = color(value) {
                    self.background = c;
                    self.authored.background = true;
                }
            }
            Property::BorderColor => {
                if let Some(c) = color(value) {
                    self.border = c;
                    self.authored.border = true;
                }
            }
            Property::Opacity => {
                if let Value::Number(n) = value
                    && n.is_finite()
                {
                    self.opacity = n.clamp(0.0, 1.0);
                }
            }
            Property::FontSize => {
                if let Some(n) = px(value) {
                    self.font_size = n.clamp(7.0, 96.0);
                    self.authored.font_size = true;
                }
            }
            Property::Radius => {
                if let Some(n) = px(value) {
                    self.radius = n.clamp(0.0, 128.0);
                    self.authored.radius = true;
                }
            }
            Property::TextShadow | Property::BoxShadow => {
                if let Value::Shadows(list) = value {
                    let foreground = self.color.to_srgba_unmultiplied();
                    let resolved: Vec<_> = list
                        .iter()
                        .take(suhina_style::paint::DRAWN_SHADOW_LAYERS)
                        .map(|shadow| shadow.resolve(self.font_size, foreground))
                        .collect();
                    if property == Property::TextShadow {
                        self.text_shadows = resolved.iter().map(text::Shadow::from_native).collect::<Vec<_>>().into();
                    } else {
                        self.shadows = resolved.iter().map(shadows::Shadow::from_native).collect::<Vec<_>>().into();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Only authored properties override native defaults. An unrelated CSS rule must not
/// replace every label's native colour, size or corner radius with defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Authored {
    pub color: bool,
    pub background: bool,
    pub border: bool,
    pub font_size: bool,
    pub radius: bool,
    pub accent: bool,
}

/// One running transition of one property of one element.
#[derive(Clone, Debug)]
struct Running {
    property: Property,
    from: Value,
    to: Value,
    start: f64,
    transition: Transition,
}

/// Motion state of one element, kept while its key exists.
#[derive(Clone, Debug, Default)]
struct Motion {
    /// When its current animation list started, and the names that identify the list.
    started: f64,
    names: Vec<String>,
    running: Vec<Running>,
}

#[derive(Default)]
struct Engine {
    source: Option<Arc<str>>,
    palette: String,
    size: [u32; 2],
    dark: bool,
    contexts: HashMap<Vec<String>, Computed>,
    toolbar: Option<(String, Vec<Module>)>,
    workspace: Option<(String, Option<Workspace>)>,
    toolbar_layout: bool,
    collecting: bool,
    nodes: Vec<StyleNode>,
    parents: Vec<usize>,
    occurrences: HashMap<(u64, String), usize>,
    previous: Vec<StyleNode>,
    /// Resolved styles before motion, by element key.
    targets: HashMap<u64, Computed>,
    /// What painters read: the targets with transitions and animations applied.
    values: HashMap<u64, Computed>,
    motions: HashMap<u64, Motion>,
    native: Option<native::Paint>,
    motion: bool,
    styles_changed: bool,
    /// The user's own sheet (`user.css`), above every theme and device sheet.
    user: Option<(Arc<str>, Arc<suhina_style::sheet::Sheet>)>,
    /// Parsed device panel sheets by the hash of their text, bounded.
    device_sheets: HashMap<u64, Arc<suhina_style::sheet::Sheet>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StyleNode {
    key: u64,
    parent: Option<usize>,
    class: String,
    name: Option<String>,
    state: u8,
    /// The device sheet this node is the root of (its hash), or 0.
    sheet: u64,
}
thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::default());
    static USER_SHEET: RefCell<Option<Arc<str>>> = const { RefCell::new(None) };
    /// Whether a device panel asked for its sheet since the last frame began, so the engine
    /// runs even when the theme has no style sheet of its own.
    static DEVICE_WANTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Most device panel sheets kept parsed at once.
const MAX_DEVICE_SHEETS: usize = 32;

/// Sets the user's own style sheet (`user.css`), which applies over every theme and device
/// sheet and stays when the theme changes. `None` removes it. Read at the next frame.
pub fn set_user_sheet(source: Option<Arc<str>>) {
    USER_SHEET.with(|sheet| *sheet.borrow_mut() = source);
}

/// Gives the device panel scope just entered its own style sheet: it applies only inside that
/// panel, above the application theme and below `user.css`. Unchanged text is parsed once; a
/// sheet that does not parse is ignored and the panel keeps the theme.
pub fn device_sheet(ctx: &egui::Context, source: Option<&str>) {
    let Some(source) = source.filter(|s| !s.trim().is_empty() && s.len() <= 64 * 1024) else { return };
    if !DEVICE_WANTED.with(|wanted| wanted.replace(true)) && !ENGINE.with(|engine| engine.borrow().collecting) {
        // The engine was idle: start it on the next frame.
        ctx.request_repaint();
    }
    let mut hash = std::hash::DefaultHasher::new();
    source.hash(&mut hash);
    let key = hash.finish().max(1);
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        if !engine.collecting {
            return;
        }
        if !engine.device_sheets.contains_key(&key) {
            let Ok(sheet) = suhina_style::sheet::Sheet::parse(source) else { return };
            if engine.device_sheets.len() >= MAX_DEVICE_SHEETS {
                engine.device_sheets.clear();
            }
            engine.device_sheets.insert(key, Arc::new(sheet));
        }
        if let Some(node) = engine.parents.last().copied().and_then(|at| engine.nodes.get_mut(at)) {
            node.sheet = key;
        }
    });
}

/// Finishes the style snapshot even on an early return from the native frame.
pub struct Frame(egui::Context, f64);
impl Drop for Frame {
    fn drop(&mut self) {
        let (changed, animating) = suhina_plugins::trace::timed("native CSS tree", || finish_tree(self.1));
        if animating {
            self.0.request_repaint_after(std::time::Duration::from_millis(16));
        } else if changed {
            self.0.request_repaint();
        }
    }
}

pub fn frame(ui: &egui::Ui) -> Frame {
    frame_with_motion(ui, true)
}

/// Disabling motion suppresses CSS transitions/keyframes as well as extra ticks.
pub fn frame_with_motion(ui: &egui::Ui, motion: bool) -> Frame {
    begin_frame(ui);
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        if engine.motion != motion {
            engine.motion = motion;
            engine.motions.clear();
            engine.styles_changed = true;
        }
        engine.collecting = engine.source.is_some();
        engine.nodes.clear();
        engine.parents.clear();
        engine.occurrences.clear();
    });
    Frame(ui.ctx().clone(), ui.input(|input| input.time))
}

/// Register an actual native scope. Named project roles retain identity when reordered.
pub fn enter(class: &str, name: Option<&str>, response: Option<&egui::Response>) -> bool {
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        if !engine.collecting || engine.nodes.len() >= MAX_NODES || engine.parents.len() >= MAX_DEPTH {
            return false;
        }
        if !super::style_class::CLASSES.contains(&class) {
            return false;
        }
        let name = name.filter(|name| {
            name.len() <= 96 && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        });
        let parent = engine.parents.last().copied();
        let parent_key = parent.and_then(|p| engine.nodes.get(p)).map_or(0, |n| n.key);
        let occurrence = engine.occurrences.entry((parent_key, class.into())).or_default();
        let index = *occurrence;
        *occurrence += 1;
        let mut hash = std::hash::DefaultHasher::new();
        (parent_key, class, name).hash(&mut hash);
        if let Some(response) = response {
            response.id.hash(&mut hash);
        } else if name.is_none() {
            index.hash(&mut hash);
        }
        let key = hash.finish();
        let state = response.map_or(0, |r| {
            u8::from(r.hovered())
                | u8::from(r.is_pointer_button_down_on()) << 1
                | u8::from(!r.enabled()) << 2
                | u8::from(r.has_focus()) << 3
        });
        let node = StyleNode { key, parent, class: class.into(), name: name.map(str::to_owned), state, sheet: 0 };
        let index = engine.nodes.len();
        engine.nodes.push(node);
        engine.parents.push(index);
        true
    })
}

pub fn leave() {
    ENGINE.with(|engine| {
        engine.borrow_mut().parents.pop();
    });
}

/// The element id a theme selects a scope by: documented controls and panels keep their
/// literal ids; others get one derived from their key.
fn element_id(node: &StyleNode) -> String {
    if node.class == "button"
        && let Some(name) = node
            .name
            .as_deref()
            .filter(|name| ["play", "stop", "record", "browser-toggle", "metronome-toggle"].contains(name))
    {
        return format!("control-{name}");
    }
    if (node.class == "workspace" && node.name.as_deref() == Some("workspace"))
        || (node.class == "workspace-panel"
            && node.name.as_deref().is_some_and(|name| ["panel-browser", "panel-main", "panel-detail"].contains(&name)))
    {
        return node.name.clone().unwrap_or_default();
    }
    // A device panel's control is selected by its id (`#cutoff`), as the panel sheet names it.
    if node.class == "device-control"
        && let Some(name) = &node.name
    {
        return name.clone();
    }
    if node.class == "toolbar-module" && node.name.as_deref().is_some_and(|name| skin::MODULES.contains(&name)) {
        node.name.clone().unwrap_or_default()
    } else {
        format!("node-{}", node.key)
    }
}

/// Applies running transitions and keyframe animations to `target` at time `now`; returns
/// the painted values and whether anything is still moving.
fn animate(motion: &mut Motion, target: &Computed, now: f64) -> (Computed, bool) {
    let mut value = target.clone();
    let mut moving = false;
    if !target.animations.is_empty() {
        let names: Vec<String> = target.animations.iter().map(|(a, _)| a.name.clone()).collect();
        if motion.names != names {
            motion.names = names;
            motion.started = now;
        }
        let elapsed = (now - motion.started) as f32;
        for (animation, frames) in target.animations.iter() {
            moving |= motion::running(animation, elapsed);
            let Some(progress) = motion::progress(animation, elapsed) else { continue };
            let base = |property: Property| target.animated(property).unwrap_or(Value::Keyword(""));
            for (property, sampled) in motion::sample(frames, progress, animation.easing, base) {
                value.set_animated(property, &sampled);
            }
        }
    } else {
        motion.names.clear();
    }
    motion.running.retain(|running| {
        let duration = f64::from(running.transition.duration.max(f32::EPSILON));
        let t = ((now - running.start) / duration) as f32;
        if t >= 1.0 {
            return false;
        }
        let eased = running.transition.easing.apply(t.max(0.0));
        let sampled = motion::sample(
            &vec![
                (0.0, vec![(running.property, running.from.clone())]),
                (1.0, vec![(running.property, running.to.clone())]),
            ],
            eased,
            motion::Easing::Linear,
            |_| running.to.clone(),
        );
        for (property, sampled) in sampled {
            value.set_animated(property, &sampled);
        }
        moving = true;
        true
    });
    (value, moving)
}

/// Starts transitions for the animatable properties that changed from `old` to `new`, from the
/// value currently painted.
fn start_transitions(motion: &mut Motion, old: &Computed, new: &Computed, painted: &Computed, now: f64) {
    for property in [
        Property::Color,
        Property::BackgroundColor,
        Property::BorderColor,
        Property::Opacity,
        Property::FontSize,
        Property::Radius,
    ] {
        let (Some(before), Some(after)) = (old.animated(property), new.animated(property)) else { continue };
        if before == after {
            continue;
        }
        let Some(transition) = motion::find(&new.transitions, property) else { continue };
        let from = painted.animated(property).unwrap_or(before);
        motion.running.retain(|running| running.property != property);
        motion.running.push(Running {
            property,
            from,
            to: after,
            start: now + f64::from(transition.delay),
            transition,
        });
    }
}

fn finish_tree(now: f64) -> (bool, bool) {
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.collecting = false;
        let changed = engine.nodes != engine.previous;
        let moving_before = !engine.motions.values().all(|m| m.running.is_empty() && m.names.is_empty());
        if engine.source.is_none()
            || engine.native.is_none()
            || (!changed && !engine.styles_changed && !moving_before && !engine.targets.is_empty())
        {
            return (false, false);
        }
        if changed || engine.styles_changed || engine.targets.is_empty() {
            let Some(resolved) = suhina_plugins::trace::timed("style paint resolve", || native::retained(&engine))
            else {
                // A tree the engine cannot resolve keeps the last painted values.
                engine.previous = engine.nodes.clone();
                engine.styles_changed = false;
                return (false, false);
            };
            let targets: HashMap<u64, Computed> = resolved.into_iter().collect();
            if engine.motion {
                let Engine { motions, values, targets: old, .. } = &mut *engine;
                for (key, new) in &targets {
                    if let (Some(old), Some(painted)) = (old.get(key), values.get(key)) {
                        start_transitions(motions.entry(*key).or_default(), old, new, painted, now);
                    }
                }
            }
            engine.motions.retain(|key, _| targets.contains_key(key));
            engine.targets = targets;
            engine.previous = engine.nodes.clone();
        }
        let styles_changed = std::mem::take(&mut engine.styles_changed);
        let mut moving = false;
        let mut values = HashMap::with_capacity(engine.targets.len());
        let motion_enabled = engine.motion;
        let Engine { targets, motions, .. } = &mut *engine;
        for (key, target) in targets.iter() {
            if motion_enabled
                && (!target.animations.is_empty() || motions.get(key).is_some_and(|m| !m.running.is_empty()))
            {
                let (value, still) = animate(motions.entry(*key).or_default(), target, now);
                moving |= still;
                values.insert(*key, value);
            } else {
                values.insert(*key, target.clone());
            }
        }
        if !moving {
            engine.motions.retain(|_, m| !m.running.is_empty() || !m.names.is_empty());
        }
        engine.values = values;
        // Publish the final sample too: once motion stops, the preceding frame still
        // painted its penultimate values.
        (changed || styles_changed || moving || moving_before, moving)
    })
}

/// Refresh only when the authored CSS, palette or viewport changes.
pub fn begin_frame(ui: &egui::Ui) {
    let user_present = USER_SHEET.with(|sheet| sheet.borrow().is_some());
    let device_wanted = DEVICE_WANTED.with(|wanted| wanted.replace(false));
    // Without a theme sheet the engine still runs for user.css and device panel sheets.
    let source = skin::current().css.clone().or_else(|| (user_present || device_wanted).then(|| Arc::from("")));
    if source.is_none() {
        ENGINE.with(|engine| {
            if engine.borrow().source.is_some() {
                *engine.borrow_mut() = Engine::default();
            }
        });
        return;
    }
    let size = ui.ctx().input(|i| i.content_rect().size());
    let size = [size.x.clamp(1.0, 16384.0) as u32, size.y.clamp(1.0, 16384.0) as u32];
    let palette = theme::palette();
    let mut variables = String::from(":root {");
    for (key, _, color) in palette.tokens() {
        let variable = key.replace('.', "-");
        // Only referenced palette variables affect this stylesheet. Resetting all
        // style/layout caches for an unrelated opacity tick rebuilt every control's
        // style during every drag frame.
        // Escaped identifiers are conservatively treated as depending on all tokens.
        if key != "text.primary"
            && !source.as_deref().is_some_and(|css| css.contains('\\') || css.contains(&format!("--{variable}")))
        {
            continue;
        }
        variables.push_str(&format!("--{variable}:{};", theme::hex(color)));
    }
    variables.push_str("}\n");
    let dark = theme::contrast(palette.window_background, Color32::WHITE)
        > theme::contrast(palette.window_background, Color32::BLACK);
    let user = USER_SHEET.with(|sheet| sheet.borrow().clone());
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        if engine.user.as_ref().map(|(text, _)| text) != user.as_ref() {
            // A user sheet that does not parse keeps the last working one.
            match user {
                Some(text) => {
                    if let Ok(sheet) = suhina_style::sheet::Sheet::parse(&text) {
                        engine.user = Some((text, Arc::new(sheet)));
                    }
                }
                None => engine.user = None,
            }
            engine.styles_changed = true;
            engine.contexts.clear();
            engine.toolbar = None;
            engine.workspace = None;
        }
        if engine.source != source {
            let native = suhina_plugins::trace::timed("style paint parse", || {
                source.as_deref().and_then(|source| native::Paint::new(source, &variables))
            });
            match native {
                Some(native) => {
                    let motion = engine.motion;
                    let user = engine.user.take();
                    let device_sheets = std::mem::take(&mut engine.device_sheets);
                    *engine = Engine {
                        source,
                        palette: variables,
                        size,
                        dark,
                        native: Some(native),
                        motion,
                        user,
                        device_sheets,
                        ..Default::default()
                    };
                }
                None => {
                    // A sheet that does not parse (a half-saved file) keeps the last
                    // working styles; its error is shown by the theme editor.
                    engine.source = source;
                }
            }
        } else if engine.size != size || engine.dark != dark || engine.palette != variables {
            // Palette drags and native resizing do not change the scope tree or
            // authored CSS. Keep the last computed snapshot and resolve the new inputs
            // once at the end of this pass.
            engine.palette = variables;
            let palette = engine.palette.clone();
            if let Some(paint) = engine.native.as_mut() {
                let _ = paint.palette(&palette);
            }
            engine.size = size;
            engine.dark = dark;
            engine.styles_changed = true;
            engine.contexts.clear();
            engine.toolbar = None;
            engine.workspace = None;
        }
    });
}

/// Whether the engine's sheet is the active theme's (an empty one stands in for none).
fn same_source(engine: Option<&Arc<str>>) -> bool {
    let current = skin::current().css.clone();
    match (engine, current.as_ref()) {
        (Some(a), Some(b)) => a == b,
        (Some(a), None) => a.is_empty(),
        _ => false,
    }
}

/// Compute inherited colours and native typography for the active class ancestry.
/// Sibling/object selectors require the host's complete retained component tree.
pub fn scoped(stack: &[String]) -> Option<Computed> {
    if stack.is_empty()
        || stack.len() > MAX_DEPTH
        || stack.iter().any(|s| !super::style_class::CLASSES.contains(&s.as_str()))
    {
        return None;
    }
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.source.as_ref()?;
        if !same_source(engine.source.as_ref()) {
            return None;
        }
        if let Some(values) =
            engine.parents.last().and_then(|at| engine.nodes.get(*at)).and_then(|node| engine.values.get(&node.key))
        {
            return Some(values.clone());
        }
        if let Some(values) = engine.contexts.get(stack) {
            return Some(values.clone());
        }
        if engine.contexts.len() >= MAX_CONTEXTS {
            return None;
        }
        let values = native::scoped(&engine, stack)?;
        engine.contexts.insert(stack.to_vec(), values.clone());
        Some(values)
    })
}

/// One real toolbar child. Drawing and input use this same Taffy rectangle.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub rect: Rect,
    pub style: Computed,
}

pub fn toolbar_layout_requested() -> bool {
    ENGINE.with(|engine| engine.borrow().toolbar_layout)
}

/// Flex/Grid layout for actual ordered, visible module ids. No theme-supplied markup.
pub fn toolbar(order: &[(String, f32)], size: Vec2) -> Option<Vec<Module>> {
    if order.len() > skin::MODULES.len()
        || order.iter().any(|(id, _)| !skin::MODULES.contains(&id.as_str()))
        || !size.is_finite()
    {
        return None;
    }
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.source.as_ref()?;
        if !same_source(engine.source.as_ref()) {
            return None;
        }
        let key = format!("{order:?}:{size:?}");
        if let Some((previous, modules)) = &engine.toolbar
            && previous == &key
        {
            return Some(modules.clone());
        }
        let mut defaults = format!(
            ".toolbar {{ display:flex; width:{}px; height:{}px; gap:4px; align-items:center; }} .toolbar-module {{ flex-shrink:0; height:{}px; }}",
            size.x.clamp(1.0, 16384.0),
            size.y.clamp(1.0, 16384.0),
            size.y.clamp(24.0, 16384.0)
        );
        for (id, width) in order {
            defaults.push_str(&format!("#{id} {{ width:{}px; }}", width.clamp(1.0, 4096.0)));
        }
        let mut elements = vec![native::element("toolbar", Some("toolbar"), None, Some(1))];
        for (id, _) in order {
            let mut module = native::element("toolbar-module", Some(id), None, Some(2));
            module.classes.push(id.clone());
            elements.push(module);
        }
        let laid = native::layout(&engine, elements, &defaults, [size.x, size.y])?;
        engine.toolbar_layout = laid.requested;
        let mut modules = Vec::new();
        for ((name, _), (rect, style)) in order.iter().zip(laid.items.into_iter().skip(1)) {
            let Some(rect) = rect else { continue };
            if style.visible {
                modules.push(Module { name: name.clone(), rect, style });
            }
        }
        engine.toolbar = Some((key, modules.clone()));
        Some(modules)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_gradients_publish_every_layer_and_their_end_colours() {
        for (image, layers, first, last) in [
            ("linear-gradient(red,blue)", 1, Color32::RED, Color32::BLUE),
            ("linear-gradient(to right, red, transparent)", 1, Color32::RED, Color32::TRANSPARENT),
            (
                "linear-gradient(to top right, white 0 45%, green 45% 100%)",
                1,
                Color32::WHITE,
                Color32::from_rgb(0, 128, 0),
            ),
            (
                "repeating-linear-gradient(90deg, transparent 0 21px, #e5ece7 21px 22px)",
                1,
                Color32::TRANSPARENT,
                Color32::from_rgb(0xe5, 0xec, 0xe7),
            ),
            ("linear-gradient(red, green, blue), linear-gradient(90deg, #fff3, #0008)", 2, Color32::RED, Color32::BLUE),
            (
                "radial-gradient(ellipse at 65% 45%, #29424280, transparent 70%)",
                1,
                Color32::from_rgba_unmultiplied(0x29, 0x42, 0x42, 0x80),
                Color32::TRANSPARENT,
            ),
            ("radial-gradient(circle 10px at left top, red, blue)", 1, Color32::RED, Color32::BLUE),
            (
                "conic-gradient(from 90deg at 25% 75%, red, blue 0.25turn, green 50%)",
                1,
                Color32::RED,
                Color32::from_rgb(0, 128, 0),
            ),
            ("repeating-conic-gradient(red 0 10%, blue 10% 20%)", 1, Color32::RED, Color32::BLUE),
        ] {
            let css = format!(".button {{color:#34789a; background-image:{image};}}");
            prepare(&css, 800.0);
            let style = scoped(&["button".into()]).unwrap();
            assert_eq!(style.gradients.len(), layers, "{image}");
            let gradient = &style.gradients[0];
            let samples = gradient.pixel_samples(100.0);
            let close = |a: Color32, b: Color32| a.to_array().iter().zip(b.to_array()).all(|(x, y)| x.abs_diff(y) <= 2);
            assert!(close(samples[0], first), "{image}: {:?}", samples[0]);
            if !image.starts_with("repeating") {
                assert!(close(samples[1023], last), "{image}: {:?}", samples[1023]);
            } else {
                assert!(samples.iter().any(|s| close(*s, last)), "{image}");
            }
            let geometry = gradient.geometry_samples(Vec2::new(100.0, 50.0));
            assert!(geometry.iter().all(|v| v.is_finite()), "{image}");
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn native_paint_values_resolve_variables_currentcolor_em_and_media() {
        let rgba = Color32::from_rgba_unmultiplied;
        prepare(
            ".button { color:#123456; background-color:#abcdef80; border-color:currentColor; border-radius:4px; opacity:.7; }",
            800.0,
        );
        let style = scoped(&["toolbar".into(), "button".into()]).unwrap();
        assert_eq!(style.color, Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!(style.background, rgba(0xab, 0xcd, 0xef, 0x80));
        assert_eq!(style.border, Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!(style.radius, 4.0);
        assert!((style.opacity - 0.7).abs() < 1e-6);
        assert_eq!(
            style.authored,
            Authored { color: true, background: true, border: true, font_size: false, radius: true, accent: false }
        );
        prepare(
            ":root { --ink:#2468ac; } .toolbar { color:var(--ink); font-size:12px; } .button { font-size:1.5em; border-radius:.25em; box-shadow:inset 0 1px 2px -1px #fff4, 1px 2px 3px currentColor; text-shadow:0 1px 1px #000a; }",
            800.0,
        );
        let style = scoped(&["toolbar".into(), "button".into()]).unwrap();
        assert_eq!(style.color, Color32::from_rgb(0x24, 0x68, 0xac));
        assert_eq!(style.font_size, 18.0);
        assert_eq!(style.radius, 4.5);
        assert_eq!(style.shadows.len(), 2);
        assert_eq!(style.text_shadows.len(), 1);
        assert!(style.authored.color && style.authored.font_size);
        for (width, expected) in
            [(800.0, Color32::from_rgb(0x0c, 0x22, 0x38)), (600.0, Color32::from_rgb(0x65, 0x43, 0x21))]
        {
            prepare(
                ".toolbar { font-size:12px; color:rgba(12, 34, 56, .5); } @media (max-width:700px) { .button { color:#654321; } }",
                width,
            );
            let style = scoped(&["toolbar".into(), "button".into()]).unwrap();
            assert_eq!(style.color.to_srgba_unmultiplied()[..3], expected.to_srgba_unmultiplied()[..3], "{width}");
        }
        skin::configure(&skin::Skin::default());
    }

    #[test]
    fn unsupported_declarations_are_skipped_and_the_rest_of_the_sheet_applies() {
        for css in [
            ".button { color:red; background-image:radial-gradient(in oklch, blue, green); }",
            ".button { color:red; filter:blur(2px); }",
        ] {
            prepare(css, 800.0);
            let actual = scoped(&["button".into()]).unwrap();
            assert!(actual.authored.color, "{css}");
            assert_eq!(actual.color, Color32::RED, "{css}");
            assert!(!actual.has_gradient(), "{css}");
            assert!(!inspect(css).unwrap().is_empty(), "{css}");
        }
        // A variable that does not make a valid colour makes the property unset, as in CSS:
        // the colour is inherited, not the earlier declaration's.
        prepare(":root { --ink:color-mix(in lab, red, blue); } .button { color:red; color:var(--ink); }", 800.0);
        assert_ne!(scoped(&["button".into()]).unwrap().color, Color32::RED);
        // A sheet that cannot be parsed at all keeps the last working styles.
        prepare(".button { color:blue; }", 800.0);
        let _ = scoped(&["button".into()]);
        let deep = format!(".button {{ color:red; }} {}", "@media (min-width:1px) {".repeat(20));
        prepare(&deep, 800.0);
        ENGINE.with(|engine| assert!(engine.borrow().native.is_some(), "the previous sheet stays"));
        skin::configure(&skin::Skin::default());
    }

    #[test]
    fn native_states_live_reload_and_compatibility_switch_keep_stable_keys() {
        let nodes = vec![
            StyleNode { key: 1, parent: None, class: "toolbar".into(), name: None, state: 0, sheet: 0 },
            StyleNode {
                key: 2,
                parent: Some(0),
                class: "button".into(),
                name: Some("play".into()),
                state: 0,
                sheet: 0,
            },
        ];
        prepare(
            ".toolbar { color:#112233; } .toolbar:hover { color:#445566; } #control-play { box-shadow:0 1px; }",
            800.0,
        );
        ENGINE.with(|engine| engine.borrow_mut().nodes = nodes.clone());
        assert_eq!(finish_tree(0.0), (true, false));
        ENGINE.with(|engine| assert_eq!(engine.borrow().values[&2].color, Color32::from_rgb(0x11, 0x22, 0x33)));
        assert_eq!(finish_tree(0.1), (false, false), "unchanged scopes must use the retained result");
        ENGINE.with(|engine| engine.borrow_mut().nodes[1].state = 1);
        assert_eq!(finish_tree(0.2), (true, false));
        ENGINE.with(|engine| {
            let engine = engine.borrow();
            assert_eq!(engine.values[&2].color, Color32::from_rgb(0x44, 0x55, 0x66));
        });
        for source in [
            ".button { color:#abcdef; }",
            ".button { color:#abcdef; background-image:radial-gradient(in oklch, red, blue); }",
            ".button { color:#abcdef; }",
        ] {
            prepare(source, 800.0);
            ENGINE.with(|engine| engine.borrow_mut().nodes = nodes.clone());
            assert_eq!(finish_tree(0.3), (true, false));
            ENGINE.with(|engine| {
                let engine = engine.borrow();
                assert_eq!(engine.values[&2].color, Color32::from_rgb(0xab, 0xcd, 0xef), "{source}");
                assert!(engine.native.is_some());
                assert_eq!(engine.values.len(), 2);
            });
        }
        skin::configure(&skin::Skin::default());
    }
    /// Styles of the `device-control` scopes named `cutoff` in two device panels, with the
    /// first panel given `device` as its own sheet.
    fn device_controls(theme: &str, device: &str, user: Option<&str>) -> (Computed, Computed, Computed) {
        prepare(theme, 800.0);
        set_user_sheet(user.map(Arc::from));
        let ctx = egui::Context::default();
        let mut styles = None;
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let _frame = frame(ui);
                let _window = super::super::style_class::enter("window");
                let mut found = Vec::new();
                for panel in ["track-1-device-0", "track-1-device-1"] {
                    let _panel = super::super::style_class::enter_named("device-panel", panel);
                    if panel.ends_with('0') {
                        device_sheet(ui.ctx(), Some(device));
                    }
                    let _control = super::super::style_class::enter_named("device-control", "cutoff");
                    found.push(super::super::style_class::computed().unwrap());
                }
                let _browser = super::super::style_class::enter("browser");
                found.push(super::super::style_class::computed().unwrap());
                if let [a, b, browser] = found.as_slice() {
                    styles = Some((a.clone(), b.clone(), browser.clone()));
                }
            })
            .textures_delta
            .clear();
        }
        set_user_sheet(None);
        skin::configure(&skin::Skin::default());
        styles.unwrap()
    }

    #[test]
    fn a_device_sheet_styles_its_own_panel_by_control_id_and_never_leaks() {
        let (styled, other, browser) = device_controls(
            ".device-control { color: #102030; }",
            "#cutoff { color: #ff0000; accent-color: #0000ff; } .browser { color: #00ff00; }",
            None,
        );
        assert_eq!(styled.color, Color32::from_rgb(255, 0, 0), "the device sheet wins over the theme");
        assert!(styled.authored.accent);
        assert_eq!(styled.accent, Color32::from_rgb(0, 0, 255));
        assert_eq!(other.color, Color32::from_rgb(0x10, 0x20, 0x30), "another device keeps the theme");
        assert!(!other.authored.accent);
        assert_ne!(browser.color, Color32::from_rgb(0, 255, 0), "a device sheet stays inside its panel");
    }

    #[test]
    fn the_user_sheet_wins_over_device_and_theme_and_runs_without_a_theme_sheet() {
        let (styled, other, _) = device_controls(
            ".device-control { color: #102030; }",
            "#cutoff { color: #ff0000 !important; }",
            Some("#cutoff { color: #abcdef; }"),
        );
        assert_eq!(styled.color, Color32::from_rgb(0xab, 0xcd, 0xef), "user beats even device !important");
        assert_eq!(other.color, Color32::from_rgb(0xab, 0xcd, 0xef));
        // No theme sheet at all: device and user sheets still apply.
        let (styled, other, _) = device_controls("", "#cutoff { color: #ff0000; }", None);
        assert_eq!(styled.color, Color32::from_rgb(255, 0, 0));
        assert!(!other.authored.color, "unstyled parts keep the application theme");
    }

    #[test]
    fn workspace_example_collapses_closed_panels() {
        prepare(theme::WORKSPACE_GRID_EXAMPLE, 800.0);
        let open = workspace(Vec2::new(800.0, 600.0), Some(240.0), Some((false, 180.0))).unwrap();
        assert_eq!(open.browser.unwrap().width(), 240.0);
        assert_eq!(open.detail.unwrap().height(), 180.0);
        let closed = workspace(Vec2::new(800.0, 600.0), None, None).unwrap();
        assert!(closed.browser.is_none() && closed.detail.is_none());
        assert!(
            closed.main.width() > 780.0 && closed.main.height() > 580.0,
            "closed panels reserved their fixed dimensions: {:?}",
            closed.main
        );
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn gloss_surfaces_have_opaque_bases_and_sheet_overlays_preserve_native_fills() {
        prepare(theme::GLOSS_EXAMPLE, 1138.0);
        for class in ["session", "arrangement", "browser", "detail", "device-panel", "mixer", "button"] {
            let style = scoped(&[class.into()]).unwrap();
            assert!(style.authored.background, "{class} has no explicit base");
            assert_eq!(style.background.a(), 255, "{class} exposes the desktop");
            assert!(style.has_gradient(), "{class} lost its gradient");
        }
        for css in [theme::GLOSS_EXAMPLE, include_str!("../../themes/suhina-sheet.css")] {
            prepare(css, 1138.0);
            for class in ["track-header", "track-name"] {
                let style = scoped(&[class.into()]).unwrap();
                assert!(!style.authored.background, "{class} overlay replaces the native fill");
                assert!(style.has_gradient());
            }
        }
        prepare(include_str!("../../themes/suhina-meadow.css"), 1138.0);
        for class in ["window", "application-bar", "mixer", "track-header", "track-name"] {
            assert_eq!(scoped(&[class.into()]).unwrap().background.a(), 0, "Meadow's deliberate transparency changed");
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn responsive_workspace_and_toolbar_survive_resize() {
        let css = include_str!("../../tests/fixtures/responsive-theme.css");
        prepare(css, 1138.0);
        let ctx = egui::Context::default();
        for size in
            [Vec2::new(1138.0, 1032.0), Vec2::new(1138.0, 1032.0), Vec2::new(800.0, 700.0), Vec2::new(1138.0, 1032.0)]
        {
            ctx.run_ui(
                egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() },
                |ui| {
                    let _frame = frame(ui);
                    let _window = super::super::style_class::enter("window");
                    {
                        let _toolbar = super::super::style_class::enter("toolbar");
                        for name in ["transport", "position", "performance", "mapping"] {
                            let _module = super::super::style_class::enter_named("toolbar-module", name);
                            let _button = super::super::style_class::enter("button");
                        }
                    }
                    let modules = toolbar(
                        &[
                            ("transport".into(), 100.0),
                            ("position".into(), 100.0),
                            ("performance".into(), 80.0),
                            ("mapping".into(), 60.0),
                        ],
                        Vec2::new(size.x, 56.0),
                    )
                    .unwrap();
                    assert_eq!(modules.iter().any(|module| module.name == "performance"), size.x > 1000.0);
                    assert_eq!(modules.iter().any(|module| module.name == "mapping"), size.x > 820.0);
                    let layout = workspace(Vec2::new(size.x, size.y - 100.0), None, Some((false, 200.0))).unwrap();
                    assert!(layout.browser.is_none());
                    assert_eq!(layout.detail.unwrap().height(), if size.y <= 760.0 { 150.0 } else { 200.0 });
                    let _workspace = super::super::style_class::enter_named("workspace", "workspace");
                    {
                        let _main = super::super::style_class::enter_named("workspace-panel", "panel-main");
                        let _view = super::super::style_class::enter("arrangement");
                    }
                    {
                        let _detail = super::super::style_class::enter_named("workspace-panel", "panel-detail");
                        let _editor = super::super::style_class::enter("clip-editor");
                    }
                },
            )
            .textures_delta
            .clear();
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn palette_and_resize_restyle_retained_tree_and_preserve_author_colours() {
        let saved = theme::palette();
        prepare(
            ".button { background-color:currentColor; } .browser { color:#123456; } @media (max-width:600px) { .browser { color:#654321; } }",
            800.0,
        );
        let ctx = egui::Context::default();
        for (pass, width) in [(0, 800.0), (1, 800.0), (2, 500.0)] {
            let mut palette = saved;
            palette.text_primary = Color32::from_rgba_unmultiplied(24, 48, 72, 80 + pass * 40);
            theme::local_palette(palette);
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(width, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    let _frame = frame(ui);
                    if pass > 0 {
                        ENGINE.with(|engine| {
                            let engine = engine.borrow();
                            assert!(engine.native.is_some(), "live edit discarded the native style engine");
                            assert!(!engine.values.is_empty(), "live edit forced per-control fallback documents");
                        });
                    }
                    let _window = super::super::style_class::enter("window");
                    {
                        let _button = super::super::style_class::enter("button");
                    }
                    {
                        let _browser = super::super::style_class::enter("browser");
                    }
                },
            )
            .textures_delta
            .clear();
            ENGINE.with(|engine| {
                let engine = engine.borrow();
                let button = engine.nodes.iter().find(|node| node.class == "button").unwrap();
                assert_eq!(engine.values[&button.key].background, palette.text_primary);
                let browser = engine.nodes.iter().find(|node| node.class == "browser").unwrap();
                let expected = if width > 600.0 {
                    Color32::from_rgb(0x12, 0x34, 0x56)
                } else {
                    Color32::from_rgb(0x65, 0x43, 0x21)
                };
                assert_eq!(engine.values[&browser.key].color, expected);
                assert!(!engine.styles_changed);
            });
        }
        theme::local_palette(saved);
        skin::configure(&skin::Skin::default());
    }
    #[test]
    #[ignore = "manual timing comparison for live palette edits"]
    fn measure_primary_colour_edits() {
        let saved = theme::palette();
        let css = include_str!("../../themes/suhina-sheet.css");
        prepare(css, 800.0);
        let ctx = egui::Context::default();
        let mut samples = Vec::new();
        for sample in 0..62 {
            let mut palette = saved;
            palette.text_primary = Color32::from_rgba_unmultiplied(40, 60, 50, 100 + sample);
            theme::local_palette(palette);
            let start = std::time::Instant::now();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let _frame = frame(ui);
                let _window = super::super::style_class::enter("window");
                for class in ["toolbar", "browser", "session", "mixer", "settings"] {
                    let _scope = super::super::style_class::enter(class);
                    for _ in 0..8 {
                        let _button = super::super::style_class::enter("button");
                    }
                }
            })
            .textures_delta
            .clear();
            if sample > 1 {
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        eprintln!("primary colour edit: median {:.3} ms, max {:.3} ms", samples[30], samples[59]);
        theme::local_palette(saved);
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn unreferenced_palette_edits_keep_computed_contexts_but_variables_invalidate() {
        let saved = theme::palette();
        prepare(".browser { color: var(--accent); background-color: white; }", 800.0);
        let stack = ["browser".into()];
        let original = scoped(&stack).unwrap();
        let mut palette = saved;
        palette.grid_major = Color32::from_rgba_unmultiplied(120, 120, 120, 90);
        theme::local_palette(palette);
        prepare(".browser { color: var(--accent); background-color: white; }", 800.0);
        ENGINE.with(|engine| assert_eq!(engine.borrow().contexts.len(), 1, "unrelated opacity cleared style cache"));
        assert_eq!(scoped(&stack).unwrap().color, original.color);
        palette.accent = Color32::from_rgb(10, 30, 70);
        theme::local_palette(palette);
        prepare(".browser { color: var(--accent); background-color: white; }", 800.0);
        assert_eq!(scoped(&stack).unwrap().color, palette.accent);
        theme::local_palette(saved);
        skin::configure(&skin::Skin::default());
    }
    fn prepare(css: &str, width: f32) {
        let ctx = egui::Context::default();
        let s = skin::Skin { css: Some(Arc::from(css)), ..Default::default() };
        skin::configure(&s);
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(width, 600.0))),
                ..Default::default()
            },
            |ui| begin_frame(ui),
        )
        .textures_delta
        .clear();
    }
    #[test]
    fn native_engine_resolves_variables_layers_calc_and_media() {
        prepare(
            "@layer base,custom; @layer base { .browser { color:red; }} @layer custom { :root { --ink:rgb(10 20 30); } .browser { color:var(--ink); font-size:calc(10px + 2px); }} @media (max-width:500px) { .browser { color:rgb(30 40 50); }}",
            800.0,
        );
        let values = scoped(&["window".into(), "browser".into()]).unwrap();
        assert_eq!(values.color, Color32::from_rgb(10, 20, 30));
        assert!((values.font_size - 12.0).abs() < 0.1);
        prepare("@media (max-width:500px) { .browser { color:rgb(30 40 50); }}", 400.0);
        assert_eq!(scoped(&["browser".into()]).unwrap().color, Color32::from_rgb(30, 40, 50));
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn taffy_grid_changes_real_module_rectangles_and_hidden_modules() {
        prepare(
            ".toolbar { display:grid; grid-template-columns:80px 120px; gap:10px; } #tempo { width:auto; } #transport { width:auto; } #mapping { display:none; }",
            800.0,
        );
        let modules = toolbar(
            &[("tempo".into(), 62.0), ("transport".into(), 86.0), ("mapping".into(), 64.0)],
            Vec2::new(400.0, 32.0),
        )
        .unwrap();
        assert_eq!(modules.len(), 2);
        assert!((modules[0].rect.width() - 80.0).abs() < 0.1);
        assert!((modules[1].rect.left() - 90.0).abs() < 0.1);
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn retained_scopes_match_siblings_and_keep_named_track_identity() {
        prepare(
            ".mixer[data-object=track-2] { color:rgb(10 20 30); } .mixer + .mixer { background-color:rgb(40 50 60); }",
            800.0,
        );
        let ctx = egui::Context::default();
        for pass in 0..4 {
            let order = if pass < 2 { [1, 2] } else { [2, 1] };
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let _frame = frame(ui);
                let _window = super::super::style_class::enter("window");
                let _session = super::super::style_class::enter("session");
                for (index, track) in order.iter().enumerate() {
                    let _track = super::super::style_class::enter_named("mixer", &format!("track-{track}"));
                    if pass > 0 && *track == 2 {
                        assert_eq!(theme::text_primary(), Color32::from_rgb(10, 20, 30));
                    }
                    if (pass == 1 || pass == 3) && index == 1 {
                        assert_eq!(theme::panel_background(), Color32::from_rgb(40, 50, 60));
                    }
                }
            })
            .textures_delta
            .clear();
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn disabled_state_uses_the_native_response_without_changing_its_id() {
        prepare(".button:disabled { background-color:rgb(50 60 70); }", 800.0);
        let ctx = egui::Context::default();
        let mut previous_id = None;
        for pass in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let _frame = frame(ui);
                let _window = super::super::style_class::enter("window");
                let response = ui.add_enabled(false, egui::Button::new("Disabled"));
                let _button = super::super::style_class::enter_response("button", &response);
                if let Some(id) = previous_id {
                    assert_eq!(response.id, id);
                }
                previous_id = Some(response.id);
                assert!(!response.enabled());
                if pass == 1 {
                    assert_eq!(theme::control_background(), Color32::from_rgb(50, 60, 70));
                }
            })
            .textures_delta
            .clear();
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn computed_linear_gradients_publish_their_real_rgba_pixels_and_reuse_textures() {
        prepare(".browser { background-image:linear-gradient(90deg in srgb, rgb(255 0 0 / 0), blue); }", 800.0);
        let style = scoped(&["browser".into()]).unwrap();
        ENGINE.with(|engine| assert!(engine.borrow().native.is_some()));
        assert_eq!(style.gradients.len(), 1);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(200.0, 40.0));
            assert!(background_with(ui.painter(), rect, &style));
            assert!(background_with(ui.painter(), rect, &style));
        });
        let ramps: Vec<_> = output
            .textures_delta
            .set
            .values()
            .flat_map(|deltas| deltas.iter())
            .filter(|delta| delta.image.size() == [1024, 1])
            .collect();
        assert_eq!(ramps.len(), 1);
        let egui::ImageData::Color(image) = &ramps[0].image;
        let midpoint = image.pixels[512].to_srgba_unmultiplied();
        assert_eq!(midpoint[0], 0, "CSS alpha interpolation is premultiplied");
        assert!(midpoint[2] > 250);
        assert!((127..=129).contains(&midpoint[3]));
        assert_eq!(image.pixels[1023], Color32::BLUE);
        output.textures_delta.clear();
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn radial_and_conic_gradients_publish_spatial_pixels_at_the_authored_origin() {
        for (css, red, blue) in [
            (
                ".browser { background-image:radial-gradient(circle 20px at 25% 75%, red 0 99%, blue 100%); }",
                (64, 191),
                (255, 0),
            ),
            (
                ".browser { background-image:conic-gradient(from 90deg at 50% 50%, red 0deg 90deg, blue 90deg 360deg); }",
                (224, 160),
                (128, 0),
            ),
            (
                ".browser { background-image:repeating-radial-gradient(circle 20px at 50% 50%, red 0 50%, blue 50% 100%); }",
                (128, 128),
                (147, 128),
            ),
        ] {
            prepare(css, 800.0);
            let style = scoped(&["browser".into()]).unwrap();
            assert_eq!(style.gradients.len(), 1);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(256.0));
                assert!(background_with(ui.painter(), rect, &style));
                assert!(background_with(ui.painter(), rect, &style));
            });
            let images: Vec<_> = output
                .textures_delta
                .set
                .values()
                .flat_map(|d| d.iter())
                .filter(|d| d.image.size() == [256, 256])
                .collect();
            assert_eq!(images.len(), 1, "spatial textures are cached");
            let egui::ImageData::Color(image) = &images[0].image;
            assert_eq!(image.pixels[red.1 * 256 + red.0], Color32::RED, "{css}");
            assert_eq!(image.pixels[blue.1 * 256 + blue.0], Color32::BLUE, "{css}");
            output.textures_delta.clear();
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn keyframes_advance_with_the_clock_and_the_motion_setting_suppresses_them() {
        let css = ".browser { color:red; animation:ink 1s linear forwards; } @keyframes ink { from {color:red;} to {color:blue;} }";
        for motion in [true, false] {
            prepare(css, 800.0);
            let ctx = egui::Context::default();
            let mut colors = Vec::new();
            for time in [0.0, 0.5, 1.1] {
                ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0))),
                        ..Default::default()
                    },
                    |ui| {
                        let _frame = frame_with_motion(ui, motion);
                        let _browser = super::super::style_class::enter("browser");
                    },
                )
                .textures_delta
                .clear();
                colors.push(ENGINE.with(|engine| {
                    let engine = engine.borrow();
                    let key = engine.nodes.iter().find(|n| n.class == "browser").unwrap().key;
                    engine.values[&key].color
                }));
            }
            if motion {
                assert!(colors[1].r() > 20 && colors[1].b() > 20, "{colors:?}");
                assert_eq!(colors[2], Color32::BLUE);
            } else {
                assert_eq!(colors, [Color32::RED; 3]);
                assert!(ENGINE.with(|engine| engine.borrow().motions.values().all(|m| m.running.is_empty())));
            }
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn native_state_change_starts_a_transition() {
        prepare(
            ".button { background-color:red; transition:background-color 1s linear; } .button:disabled { background-color:blue; }",
            800.0,
        );
        let ctx = egui::Context::default();
        let mut colors = Vec::new();
        for (index, time) in [0.0, 0.1, 0.6, 1.2].into_iter().enumerate() {
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    let _frame = frame(ui);
                    let response = ui.add_enabled(index == 0, egui::Button::new("Native state"));
                    let _scope = super::super::style_class::enter_response("button", &response);
                },
            )
            .textures_delta
            .clear();
            colors.push(ENGINE.with(|engine| {
                let engine = engine.borrow();
                let key = engine.nodes.iter().find(|n| n.class == "button").unwrap().key;
                engine.values[&key].background
            }));
        }
        assert_eq!(colors[0], Color32::RED);
        assert!(colors[2].r() > 20 && colors[2].b() > 20, "{colors:?}");
        assert_eq!(colors[3], Color32::BLUE);
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn computed_inset_shadow_uses_current_color_and_reuses_its_texture() {
        prepare(".browser { color:rgb(120 30 180); box-shadow:inset 8px 0 0 0 currentColor; }", 800.0);
        let style = scoped(&["browser".into()]).unwrap();
        assert_eq!(style.shadows.len(), 1);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(40.0));
            assert!(background_with(ui.painter(), rect, &style));
            assert!(background_with(ui.painter(), rect, &style));
        });
        let images: Vec<_> = output
            .textures_delta
            .set
            .iter()
            .filter(|(id, _)| **id != egui::TextureId::default())
            .flat_map(|(_, deltas)| deltas.iter())
            .map(|d| d.image.clone())
            .collect();
        output.textures_delta.clear();
        assert_eq!(images.len(), 1);
        let egui::ImageData::Color(image) = &images[0];
        assert_eq!(
            image.pixels[(image.size[1] / 2) * image.size[0] + image.size[0] / 16],
            Color32::from_rgb(120, 30, 180)
        );
        assert_eq!(image.pixels[(image.size[1] / 2) * image.size[0] + image.size[0] / 2].a(), 0);
        output.textures_delta.clear();
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn keyframe_only_properties_are_applied_without_a_static_declaration() {
        prepare(
            ".browser { animation:ink 1s linear forwards; } @keyframes ink { from {color:red;} to {color:blue;} }",
            800.0,
        );
        let ctx = egui::Context::default();
        for time in [0.0, 1.1] {
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    let _frame = frame(ui);
                    let _scope = super::super::style_class::enter("browser");
                },
            )
            .textures_delta
            .clear();
        }
        ENGINE.with(|engine| {
            let engine = engine.borrow();
            let key = engine.nodes.iter().find(|n| n.class == "browser").unwrap().key;
            assert!(engine.values[&key].authored.color);
            assert_eq!(engine.values[&key].color, Color32::BLUE);
        });
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn workspace_grid_resolves_real_panels_and_respects_native_visibility() {
        prepare(
            "#workspace { display:grid; grid-template-columns:minmax(0,1fr) 240px; grid-template-rows:minmax(0,1fr) 160px; gap:8px; } #panel-browser {grid-column:2;grid-row:1 / -1;} #panel-main {grid-column:1;grid-row:1;} #panel-detail {grid-column:1;grid-row:2;}",
            800.0,
        );
        let layout = workspace(Vec2::new(800.0, 600.0), Some(200.0), Some((false, 120.0))).unwrap();
        assert_eq!(layout.main, Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(552.0, 432.0)));
        assert_eq!(layout.browser.unwrap().left(), 560.0);
        assert_eq!(layout.detail.unwrap().top(), 440.0);
        let closed = workspace(Vec2::new(800.0, 600.0), None, None).unwrap();
        assert!(closed.browser.is_none() && closed.detail.is_none());
        prepare(".browser {color:red;}", 800.0);
        assert!(workspace(Vec2::new(800.0, 600.0), Some(200.0), Some((false, 120.0))).is_none());
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn unrelated_css_preserves_native_colours_font_sizes_and_corner_defaults() {
        prepare(".browser { color:rgb(10 20 30); }", 800.0);
        let original = theme::palette();
        {
            let _mixer = super::super::style_class::enter("mixer");
            assert_eq!(theme::palette(), original);
            assert_eq!(super::super::style_class::font_size(9.0), 9.0);
            assert_eq!(super::super::style_class::radius(4.0), 4.0);
        }
        {
            let _browser = super::super::style_class::enter("browser");
            assert_eq!(theme::text_primary(), Color32::from_rgb(10, 20, 30));
        }
        skin::configure(&skin::Skin::default());
    }
}
