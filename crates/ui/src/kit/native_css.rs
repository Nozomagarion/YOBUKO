// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stylo computed styles and Taffy layout, consumed by native egui controls.
//!
//! The host supplies a bounded tree; themes never supply HTML or executable code.
//! No resource provider is installed. This module is UI-thread-only.
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use blitz_dom::{DocumentConfig, StyleThreading};
use blitz_html::HtmlDocument;
use blitz_traits::net::DummyNetProvider;
use blitz_traits::shell::{ColorScheme, Viewport};
use egui::{Color32, Rect, Vec2};

use super::{skin, theme};
mod diagnostics;
mod geometry;
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

/// Styles exposed by native painters. Parsed CSS support is broader than this paint adapter.
#[derive(Clone, Debug)]
pub struct Computed {
    pub color: Color32,
    pub background: Color32,
    pub border: Color32,
    pub font_size: f32,
    pub radius: f32,
    pub opacity: f32,
    pub visible: bool,
    pub authored: Authored,
    gradients: Arc<[paint::Gradient]>,
    shadows: Arc<[shadows::Shadow]>,
    text_shadows: Arc<[text::Shadow]>,
}
impl Computed {
    pub fn has_gradient(&self) -> bool {
        !self.gradients.is_empty()
    }
}

/// Only authored properties override native defaults. An unrelated CSS rule must not
/// replace every label's native colour, size or corner radius with browser defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct Authored {
    pub color: bool,
    pub background: bool,
    pub border: bool,
    pub font_size: bool,
    pub radius: bool,
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
    values: HashMap<u64, Computed>,
    document: Option<HtmlDocument>,
    motion: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StyleNode {
    key: u64,
    parent: Option<usize>,
    class: String,
    name: Option<String>,
    state: u8,
}
thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::default());
}

/// Finishes the style snapshot even on an early return from the native frame.
pub struct Frame(egui::Context, f64);
impl Drop for Frame {
    fn drop(&mut self) {
        let (changed, animating) = finish_tree(self.1);
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
            engine.document = None;
            engine.previous.clear();
            engine.values.clear();
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
        let node = StyleNode { key, parent, class: class.into(), name: name.map(str::to_owned), state };
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

fn html_id(node: &StyleNode) -> String {
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
    if node.class == "toolbar-module" && node.name.as_deref().is_some_and(|name| skin::MODULES.contains(&name)) {
        node.name.clone().unwrap_or_default()
    } else {
        format!("node-{}", node.key)
    }
}

fn native_state(state: u8) -> style_dom::ElementState {
    use style_dom::ElementState as S;
    let mut result = if state & 4 != 0 { S::DISABLED } else { S::ENABLED };
    for (bit, value) in [(1, S::HOVER), (2, S::ACTIVE), (8, S::FOCUS | S::FOCUSRING)] {
        if state & bit != 0 {
            result.insert(value);
        }
    }
    result
}

fn finish_tree(now: f64) -> (bool, bool) {
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.collecting = false;
        let changed = engine.nodes != engine.previous;
        let animating_before = engine.document.as_ref().is_some_and(|doc| doc.is_animating());
        if engine.source.is_none() || (!changed && !animating_before && engine.document.is_some()) {
            return (false, false);
        }
        let same_tree = engine.nodes.len() == engine.previous.len()
            && engine
                .nodes
                .iter()
                .zip(&engine.previous)
                .all(|(a, b)| a.key == b.key && a.parent == b.parent && a.class == b.class && a.name == b.name);
        let retained = if same_tree { engine.document.take() } else { None };
        let mut doc = if let Some(doc) = retained {
            doc
        } else {
            let mut body = String::from("<!doctype html><html><body>");
            let mut open = Vec::new();
            for (index, node) in engine.nodes.iter().enumerate() {
                while open.last().copied() != node.parent {
                    if open.pop().is_none() {
                        break;
                    }
                    body.push_str("</div>");
                }
                body.push_str(&format!("<div id=\"{}\" class=\"{}\"", html_id(node), node.class));
                if let Some(name) = &node.name {
                    body.push_str(&format!(" data-object=\"{name}\""));
                }
                body.push('>');
                open.push(index);
            }
            for _ in open {
                body.push_str("</div>");
            }
            body.push_str("</body></html>");
            let defaults = format!(
                "html,body {{ margin:0; color:{}; font-size:{}px; }}",
                theme::hex(theme::palette().text_primary),
                theme::TEXT
            );
            let Some(doc) = document(&engine, &body, &defaults) else {
                return (false, false);
            };
            doc
        };
        let mut states: Vec<_> = engine.nodes.iter().map(|node| node.state).collect();
        for node in &engine.nodes {
            if node.state & 1 == 0 {
                continue;
            }
            let mut parent = node.parent;
            while let Some(index) = parent {
                let Some(node) = engine.nodes.get(index) else {
                    break;
                };
                if let Some(state) = states.get_mut(index) {
                    *state |= 1;
                }
                parent = node.parent;
            }
        }
        let mut ids = Vec::new();
        for (index, node) in engine.nodes.iter().enumerate() {
            let state = states.get(index).copied().unwrap_or(0);
            let Some(id) = doc.get_element_by_id(&html_id(node)) else {
                continue;
            };
            let desired = native_state(state);
            let mask = style_dom::ElementState::HOVER
                | style_dom::ElementState::ACTIVE
                | style_dom::ElementState::DISABLED
                | style_dom::ElementState::ENABLED
                | style_dom::ElementState::FOCUS
                | style_dom::ElementState::FOCUSRING;
            if doc.get_node(id).is_some_and(|target| target.element_state().intersection(mask) != desired) {
                doc.snapshot_node_state_only(id);
                if let Some(target) = doc.get_node_mut(id) {
                    target.element_state_mut().remove(mask);
                    target.element_state_mut().insert(desired);
                    target.mark_ancestors_dirty();
                }
            }
            ids.push((node.key, id));
        }
        doc.resolve(now);
        let values = ids.into_iter().filter_map(|(key, id)| computed(&doc, id).map(|values| (key, values))).collect();
        engine.values = values;
        engine.previous = engine.nodes.clone();
        let animating = engine.motion && doc.is_animating();
        engine.document = Some(doc);
        // Publish the final sample too: once the animation stops, the preceding
        // frame still painted its penultimate values.
        (changed || animating || animating_before, animating)
    })
}

/// Refresh only when the authored CSS, palette or viewport changes.
pub fn begin_frame(ui: &egui::Ui) {
    let source = skin::current().css.clone();
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
        variables.push_str(&format!("--{}:{};", key.replace('.', "-"), theme::hex(color)));
    }
    variables.push_str("}\n");
    let dark = theme::contrast(palette.window_background, Color32::WHITE)
        > theme::contrast(palette.window_background, Color32::BLACK);
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        if engine.source != source || engine.size != size || engine.dark != dark || engine.palette != variables {
            *engine = Engine { source, palette: variables, size, dark, ..Default::default() };
        }
    });
}

fn document(engine: &Engine, body: &str, defaults: &str) -> Option<HtmlDocument> {
    let css = engine.source.as_ref()?;
    if inspect(css).is_err() {
        return None;
    }
    // Stylesheets are supplied through the CSS API, never interpolated into HTML.
    let config = DocumentConfig {
        viewport: Some(Viewport::new(
            engine.size[0],
            engine.size[1],
            1.0,
            if engine.dark { ColorScheme::Dark } else { ColorScheme::Light },
        )),
        net_provider: Some(Arc::new(DummyNetProvider)),
        style_threading: StyleThreading::Sequential,
        ..Default::default()
    };
    let mut doc = HtmlDocument::from_html(body, config);
    doc.add_user_agent_stylesheet(defaults);
    doc.add_user_agent_stylesheet(&engine.palette);
    if !engine.motion {
        doc.add_user_agent_stylesheet("* { animation:none !important; transition:none !important; }");
    }
    let sheet = doc.make_stylesheet(css.as_ref(), style::stylesheets::Origin::Author);
    let root = doc.try_root_element()?.id;
    doc.add_stylesheet_for_node(sheet, root);
    Some(doc)
}

fn rgba(color: &style::color::AbsoluteColor) -> Color32 {
    let srgb = color.to_color_space(style::color::ColorSpace::Srgb);
    let channels = srgb.raw_components();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(byte(channels[0]), byte(channels[1]), byte(channels[2]), byte(channels[3]))
}

fn authored_property(
    doc: &HtmlDocument,
    id: blitz_dom::NodeId,
    property: style::properties::LonghandId,
    inherited: bool,
) -> bool {
    use style::properties::PropertyDeclarationId;
    use style::rule_tree::CascadeOrigin;
    let guard = doc.guard().read();
    let mut current = Some(id);
    for _ in 0..MAX_DEPTH + 3 {
        let Some(node) = current.and_then(|id| doc.get_node(id)) else {
            break;
        };
        if let Some(values) = node.primary_styles()
            && let Some(rules) = &values.rules
        {
            for rule in rules.self_and_ancestors() {
                if !matches!(
                    rule.cascade_level().origin(),
                    CascadeOrigin::Author | CascadeOrigin::Animations | CascadeOrigin::Transitions
                ) {
                    continue;
                }
                if let Some(source) = rule.style_source()
                    && source.read(&guard).contains(PropertyDeclarationId::Longhand(property))
                {
                    return true;
                }
            }
        }
        if !inherited {
            break;
        }
        current = node.parent;
    }
    false
}

fn computed(doc: &HtmlDocument, id: blitz_dom::NodeId) -> Option<Computed> {
    use style::properties::{LonghandId, PropertyDeclarationId};
    let node = doc.get_node(id)?;
    let values = node.primary_styles()?;
    let color = values.clone_color();
    let background = values.clone_background_color().resolve_to_absolute(&color);
    let border = values.clone_border_top_color().resolve_to_absolute(&color);
    let radius = values.computed_value_to_string(PropertyDeclarationId::Longhand(LonghandId::BorderTopLeftRadius));
    let radius = radius
        .split_whitespace()
        .next()
        .and_then(|v| v.strip_suffix("px"))
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 128.0);
    let serialized = values.computed_value_to_string(PropertyDeclarationId::Longhand(LonghandId::BackgroundImage));
    let mut hash = std::hash::DefaultHasher::new();
    serialized.hash(&mut hash);
    rgba(&color).hash(&mut hash);
    let key = hash.finish();
    let gradients = values
        .get_background()
        .background_image
        .0
        .iter()
        .take(4)
        .enumerate()
        .filter_map(|(index, image)| {
            if let style::values::computed::Image::Gradient(gradient) = image {
                paint::Gradient::from_css(gradient, &color, key.wrapping_add(index as u64))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .into();
    Some(Computed {
        gradients,
        text_shadows: values
            .clone_text_shadow()
            .0
            .iter()
            .take(4)
            .map(|shadow| text::Shadow::from_css(shadow, &color))
            .collect::<Vec<_>>()
            .into(),
        shadows: values
            .clone_box_shadow()
            .0
            .iter()
            .take(4)
            .map(|shadow| shadows::Shadow::from_css(shadow, &color))
            .collect::<Vec<_>>()
            .into(),
        authored: Authored {
            color: authored_property(doc, id, LonghandId::Color, true),
            background: authored_property(doc, id, LonghandId::BackgroundColor, false),
            border: authored_property(doc, id, LonghandId::BorderTopColor, false),
            font_size: authored_property(doc, id, LonghandId::FontSize, true),
            radius: authored_property(doc, id, LonghandId::BorderTopLeftRadius, false),
        },
        visible: values.computed_value_to_string(PropertyDeclarationId::Longhand(LonghandId::Visibility)) == "visible",
        color: rgba(&color),
        background: rgba(&background),
        border: rgba(&border),
        font_size: values.clone_font_size().computed_size().px().clamp(7.0, 96.0),
        radius,
        opacity: values.clone_opacity().clamp(0.0, 1.0),
    })
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
        if engine.source != skin::current().css {
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
        let mut body = String::from("<!doctype html><html><body>");
        for (index, class) in stack.iter().enumerate() {
            body.push_str(&format!("<div class=\"{class}\" id=\"scope-{index}\">"));
        }
        for _ in stack {
            body.push_str("</div>");
        }
        body.push_str("</body></html>");
        let defaults = format!(
            "html,body {{ margin:0; color:{}; font-size:{}px; }}",
            theme::hex(theme::text_primary()),
            theme::TEXT
        );
        let mut doc = document(&engine, &body, &defaults)?;
        doc.resolve(0.0);
        let id = doc.get_element_by_id(&format!("scope-{}", stack.len() - 1))?;
        let values = computed(&doc, id)?;
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
fn requests_layout(doc: &HtmlDocument, id: blitz_dom::NodeId) -> bool {
    use style::properties::LonghandId::*;
    [
        Display,
        Width,
        Height,
        Order,
        FlexDirection,
        FlexWrap,
        FlexGrow,
        FlexShrink,
        FlexBasis,
        JustifyContent,
        AlignItems,
        AlignSelf,
        GridTemplateColumns,
        GridTemplateRows,
        GridColumnStart,
        GridColumnEnd,
        GridRowStart,
        GridRowEnd,
        ColumnGap,
        RowGap,
        MarginLeft,
        MarginRight,
        MarginTop,
        MarginBottom,
        PaddingLeft,
        PaddingRight,
        PaddingTop,
        PaddingBottom,
        Visibility,
    ]
    .into_iter()
    .any(|property| authored_property(doc, id, property, false))
}

/// Flex/Grid layout for actual ordered, visible module ids. No theme-supplied markup.
pub fn toolbar(order: &[(String, f32)], size: Vec2) -> Option<Vec<Module>> {
    if order.len() > skin::MODULES.len() || order.iter().any(|(id, _)| !skin::MODULES.contains(&id.as_str())) {
        return None;
    }
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.source.as_ref()?;
        if engine.source != skin::current().css { return None; }
        let key = format!("{order:?}:{size:?}");
        if let Some((previous, modules)) = &engine.toolbar && previous == &key {
            return Some(modules.clone());
        }
        let mut body = String::from("<!doctype html><html><body class=\"window\"><div class=\"toolbar\" id=\"toolbar\">");
        let mut defaults = format!("html,body {{ margin:0; }} .toolbar {{ display:flex; width:{}px; height:{}px; gap:4px; align-items:center; }} .toolbar-module {{ flex-shrink:0; height:{}px; }}", size.x.max(1.0),size.y.max(1.0),size.y.max(24.0));
        for (id, width) in order {
            body.push_str(&format!("<div class=\"toolbar-module {id}\" id=\"{id}\"></div>"));
            defaults.push_str(&format!("#{id} {{ width:{}px; }}",width.clamp(1.0,4096.0)));
        }
        body.push_str("</div></body></html>");
        let mut doc = document(&engine, &body, &defaults)?;
        doc.resolve(0.0);
        engine.toolbar_layout = doc.get_element_by_id("toolbar").is_some_and(|id| requests_layout(&doc,id))
            || order.iter().any(|(name,_)| doc.get_element_by_id(name).is_some_and(|id| requests_layout(&doc,id)));
        let mut modules = Vec::new();
        for (name, _) in order {
            let id = doc.get_element_by_id(name)?;
            let node = doc.get_node(id)?;
            let layout = node.final_layout();
            if !node.has_boxes() { continue; }
            let position = node.absolute_position(0.0,0.0);
            let rect = Rect::from_min_size(egui::pos2(position.x,position.y), Vec2::new(layout.size.width,layout.size.height));
            if !rect.is_finite() || rect.width() > 16384.0 || rect.height() > 16384.0 { return None; }
            let style=computed(&doc,id)?;
            if style.visible { modules.push(Module { name:name.clone(),rect,style }); }
        }
        engine.toolbar = Some((key,modules.clone()));
        Some(modules)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn stylo_resolves_variables_layers_modern_colors_and_media() {
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
    fn retained_document_advances_keyframes_and_motion_setting_suppresses_them() {
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
                assert!(!ENGINE.with(|engine| engine.borrow().document.as_ref().unwrap().is_animating()));
            }
        }
        skin::configure(&skin::Skin::default());
    }
    #[test]
    fn native_state_change_starts_a_transition_instead_of_restarting_the_document() {
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
            .values()
            .flat_map(|d| d.iter())
            .filter(|d| d.image.size() == [256, 256])
            .collect();
        assert_eq!(images.len(), 1);
        let egui::ImageData::Color(image) = &images[0].image;
        assert_eq!(image.pixels[128 * 256 + 16], Color32::from_rgb(120, 30, 180));
        assert_eq!(image.pixels[128 * 256 + 128].a(), 0);
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
