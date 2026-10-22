// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stylo computed styles and Taffy layout, consumed by native egui controls.
//!
//! The host supplies a bounded tree; themes never supply HTML or executable code.
//! No resource provider is installed. This module is UI-thread-only.
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use blitz_dom::{DocumentConfig, StyleThreading};
use blitz_html::HtmlDocument;
use blitz_traits::net::DummyNetProvider;
use blitz_traits::shell::{ColorScheme, Viewport};
use egui::{Color32, Rect, Vec2};

use super::{skin, theme};

const MAX_CONTEXTS: usize = 128;
const MAX_DEPTH: usize = 16;

/// Styles exposed by native painters. Parsed CSS support is broader than this paint adapter.
#[derive(Clone, Debug)]
pub struct Computed {
    pub color: Color32,
    pub background: Color32,
    pub border: Color32,
    pub font_size: f32,
    pub radius: f32,
    pub opacity: f32,
}

#[derive(Default)]
struct Engine {
    source: Option<Arc<str>>,
    palette: String,
    size: [u32; 2],
    dark: bool,
    contexts: HashMap<Vec<String>, Computed>,
    toolbar: Option<(String, Vec<Module>)>,
}
thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::default());
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
    if css.len() > super::theme_css::MAX_CSS {
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
    Some(Computed {
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
        if let Some((previous, modules)) = &engine.toolbar {
            if previous == &key { return Some(modules.clone()); }
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
        let mut modules = Vec::new();
        for (name, _) in order {
            let id = doc.get_element_by_id(name)?;
            let node = doc.get_node(id)?;
            let layout = node.final_layout();
            if !node.has_boxes() { continue; }
            let position = node.absolute_position(0.0,0.0);
            let rect = Rect::from_min_size(egui::pos2(position.x,position.y), Vec2::new(layout.size.width,layout.size.height));
            if !rect.is_finite() || rect.width() > 16384.0 || rect.height() > 16384.0 { return None; }
            modules.push(Module { name:name.clone(),rect,style:computed(&doc,id)? });
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
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(width, 600.0))),
                ..Default::default()
            },
            |ui| begin_frame(ui),
        );
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
}
