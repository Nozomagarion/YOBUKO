// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared style resolution for window elements and scoped SDK device panels.
use crate::{
    media::Viewport,
    property::Property,
    selector::{Specificity, Tree},
    sheet::{Declaration, Sheet},
    value::{Color, Value},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Layers are an explicit host policy; a device cannot choose a higher layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Default,
    Theme,
    Device,
    User,
}

pub struct AppliedSheet<'a> {
    pub sheet: &'a Sheet,
    pub layer: Layer,
    pub root: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ComputedStyle {
    values: BTreeMap<Property, Value>,
    variables: Arc<BTreeMap<String, String>>,
    authored: BTreeSet<Property>,
    /// The element's `animation` items with their keyframes resolved for it.
    keyframes: Vec<(crate::motion::Animation, Arc<crate::motion::Frames>)>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            values: Property::ALL.iter().map(|p| (*p, p.initial())).collect(),
            variables: Arc::default(),
            authored: BTreeSet::new(),
            keyframes: Vec::new(),
        }
    }
}

impl ComputedStyle {
    pub fn get(&self, property: Property) -> Value {
        self.values.get(&property).cloned().unwrap_or_else(|| property.initial())
    }
    pub fn variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(String::as_str)
    }
    /// Host defaults must not replace a native widget's own appearance. Inherited
    /// authored properties retain this marker across the component tree.
    pub fn authored(&self, property: Property) -> bool {
        self.authored.contains(&property)
    }
    /// Keyframe animations that apply, each with its frames resolved for this element.
    pub fn animations(&self) -> &[(crate::motion::Animation, Arc<crate::motion::Frames>)] {
        &self.keyframes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Priority(u8, u8, u16, Specificity, usize);

fn priority(layer: Layer, important: bool, cascade: Option<u16>, specificity: Specificity, order: usize) -> Priority {
    // Native layers deliberately guarantee that a user's override wins over
    // device/theme sheets, including author !important. Within a layer normal
    // CSS importance, `@layer` order, specificity and source order apply.
    Priority(layer as u8, u8::from(important), Sheet::layer_rank(cascade, important), specificity, order)
}

/// Compute a bounded immutable snapshot. The host calls this on relevant changes,
/// never from the audio callback. Malformed declarations fall back to inherited/default values.
pub fn compute(tree: &Tree, viewport: Viewport, sheets: &[AppliedSheet<'_>]) -> Result<Vec<ComputedStyle>, String> {
    compute_inner(tree, viewport, sheets, false)
}

/// Migration adapters require every substituted value to be supported before
/// publishing a snapshot. Unsupported expressions leave the legacy path in charge.
pub fn compute_checked(
    tree: &Tree,
    viewport: Viewport,
    sheets: &[AppliedSheet<'_>],
) -> Result<Vec<ComputedStyle>, String> {
    compute_inner(tree, viewport, sheets, true)
}

fn compute_inner(
    tree: &Tree,
    viewport: Viewport,
    sheets: &[AppliedSheet<'_>],
    checked: bool,
) -> Result<Vec<ComputedStyle>, String> {
    if !viewport.valid() || sheets.len() > 32 {
        return Err("Invalid style viewport or sheet count".into());
    }
    if sheets.iter().any(|sheet| sheet.root.is_some_and(|root| tree.elements().get(root).is_none())) {
        return Err("Invalid style scope root".into());
    }
    let mut result: Vec<ComputedStyle> = Vec::with_capacity(tree.elements().len());
    let mut work = 2_000_000usize;
    let mut variable_bytes = 0usize;
    for (node, element) in tree.elements().iter().enumerate() {
        let parent = element.parent.and_then(|parent| result.get(parent));
        let mut computed = ComputedStyle::default();
        if let Some(parent) = parent {
            for property in Property::ALL.iter().filter(|p| p.inherits()) {
                computed.values.insert(*property, parent.get(*property));
                if parent.authored(*property) {
                    computed.authored.insert(*property);
                }
            }
            computed.variables = Arc::clone(&parent.variables);
        }
        let mut winners: BTreeMap<&str, (Priority, &Declaration)> = BTreeMap::new();
        let mut order = 0usize;
        for applied in sheets {
            if applied.root.is_some_and(|root| !tree.contains(node, root)) {
                continue;
            }
            for rule in &applied.sheet.rules {
                order += 1;
                work = work.checked_sub(1).ok_or("Style cascade exceeded its work budget")?;
                if !rule.media.iter().all(|media| media.matches(viewport)) {
                    continue;
                }
                let specificity = rule
                    .selectors
                    .iter()
                    .filter_map(|selector| {
                        selector
                            .matches_with_budget(tree, node, applied.root, &mut work)
                            .then_some(selector.specificity)
                    })
                    .max();
                if work == 0 {
                    return Err("Style selector matching exceeded its work budget".into());
                }
                let Some(specificity) = specificity else {
                    continue;
                };
                for declaration in &rule.declarations {
                    let priority = priority(applied.layer, declaration.important, rule.layer, specificity, order);
                    if winners.get(declaration.name.as_str()).is_none_or(|(previous, _)| *previous <= priority) {
                        winners.insert(&declaration.name, (priority, declaration));
                    }
                }
            }
        }
        if winners.keys().any(|key| key.starts_with("--")) {
            let mut variables = (*computed.variables).clone();
            for (name, (_, declaration)) in winners.iter().filter(|(name, _)| name.starts_with("--")) {
                match declaration.value.trim() {
                    "initial" => {
                        variables.remove(*name);
                    }
                    "inherit" | "unset" => {}
                    _ => {
                        variables.insert((*name).into(), declaration.value.clone());
                    }
                }
            }
            let resolved = crate::variables::resolve(&variables)?;
            variable_bytes = variable_bytes
                .checked_add(resolved.values().map(String::len).sum::<usize>())
                .filter(|bytes| *bytes <= 16 * 1024 * 1024)
                .ok_or("Computed variables exceed 16 MiB")?;
            computed.variables = Arc::new(resolved);
        }
        for (name, (priority, declaration)) in winners {
            let Some(property) = Property::from_name(name) else {
                continue;
            };
            let value = match crate::variables::substitute(&declaration.value, &computed.variables).and_then(|value| {
                match declaration.shorthand {
                    Some(shorthand) => shorthand.part(property, &value),
                    None => Ok(value),
                }
            }) {
                Ok(value) => value,
                Err(error) if checked => return Err(error),
                Err(_) => continue,
            };
            let value = match value.trim() {
                "inherit" => parent.map_or_else(|| property.initial(), |parent| parent.get(property)),
                "initial" => property.initial(),
                "unset" if property.inherits() => {
                    parent.map_or_else(|| property.initial(), |parent| parent.get(property))
                }
                "unset" => property.initial(),
                other => match property.parse(&crate::value::viewport_units(other, viewport.width, viewport.height)) {
                    Ok(value) => value,
                    Err(error) if checked => return Err(error),
                    Err(_) => continue,
                },
            };
            computed.values.insert(property, value);
            if priority.0 > Layer::Default as u8 {
                computed.authored.insert(property);
            }
        }
        if let Value::Animations(animations) = computed.get(Property::Animation) {
            for animation in animations {
                // The last keyframes rule of that name in the sheets that reach this element.
                let rule = sheets
                    .iter()
                    .rev()
                    .filter(|applied| applied.root.is_none_or(|root| tree.contains(node, root)))
                    .find_map(|applied| applied.sheet.keyframes.iter().rev().find(|(name, _)| *name == animation.name));
                let Some((_, frames)) = rule else { continue };
                let mut resolved: crate::motion::Frames = Vec::with_capacity(frames.len());
                for (offset, declarations) in frames {
                    let mut values = Vec::new();
                    for declaration in declarations.iter().filter(|d| !d.important) {
                        work = work.checked_sub(1).ok_or("Style cascade exceeded its work budget")?;
                        let Some(property) = Property::from_name(&declaration.name) else { continue };
                        let value = crate::variables::substitute(&declaration.value, &computed.variables)
                            .and_then(|value| match declaration.shorthand {
                                Some(shorthand) => shorthand.part(property, &value),
                                None => Ok(value),
                            })
                            .and_then(|value| {
                                property.parse(&crate::value::viewport_units(&value, viewport.width, viewport.height))
                            });
                        match value {
                            Ok(value) => values.push((property, value)),
                            Err(error) if checked => return Err(error),
                            Err(_) => {}
                        }
                    }
                    resolved.push((*offset, values));
                }
                for (_, values) in &resolved {
                    for (property, _) in values {
                        computed.authored.insert(*property);
                    }
                }
                computed.keyframes.push((animation, Arc::new(resolved)));
            }
        }
        // Inherited font size is a computed size, not an em expression to apply
        // again at each descendant. Other em lengths resolve against this size.
        let parent_size = match parent.map(|style| style.get(Property::FontSize)) {
            Some(Value::Length(crate::value::Length::Px(size))) => size,
            _ => 11.0,
        };
        let font_size = match computed.get(Property::FontSize) {
            Value::Length(crate::value::Length::Px(size)) => size,
            Value::Length(crate::value::Length::Em(scale) | crate::value::Length::Percent(scale)) => {
                parent_size * scale
            }
            _ => parent_size,
        }
        .clamp(1.0, 256.0);
        computed.values.insert(Property::FontSize, Value::Length(crate::value::Length::Px(font_size)));
        // Text shadows inherit their computed lengths. Resolve em once at the
        // authoring element; a child's larger font must not scale them again.
        for value in computed.values.values_mut() {
            if let Value::Shadows(shadows) = value {
                for shadow in shadows {
                    for length in shadow.offset.iter_mut().chain([&mut shadow.blur, &mut shadow.spread]) {
                        if let crate::value::Length::Em(scale) = *length {
                            *length = crate::value::Length::Px(scale * font_size);
                        }
                    }
                }
            } else if let Value::Gradients(gradients) = value {
                for gradient in gradients {
                    for stop in &mut gradient.stops {
                        if let Some(crate::value::Length::Em(scale)) = stop.position {
                            stop.position = Some(crate::value::Length::Px(scale * font_size));
                        }
                    }
                }
            }
        }
        let foreground = match computed.get(Property::Color) {
            Value::Color(Color::Current) => {
                parent.map_or(Value::Color(Color::Rgba([255; 4])), |parent| parent.get(Property::Color))
            }
            other => other,
        };
        computed.values.insert(Property::Color, foreground.clone());
        for value in computed.values.values_mut() {
            if *value == Value::Color(Color::Current) {
                *value = foreground.clone();
            }
        }
        result.push(computed);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{selector::Element, value::Length};

    fn viewport(width: f32) -> Viewport {
        Viewport { width, height: 700.0, dark: true, reduced_motion: false }
    }
    fn tree() -> Tree {
        Tree::new(vec![
            Element::new("window", None),
            Element::new("device-panel", Some(0)),
            Element::new("knob", Some(1)).with_id("cutoff"),
            Element::new("device-panel", Some(0)),
            Element::new("knob", Some(3)).with_id("cutoff"),
        ])
        .unwrap()
    }

    #[test]
    fn device_styles_inherit_app_defaults_and_user_overrides_stay_scoped() {
        let app = Sheet::parse("window { --accent: #79e2bd; color: #eeeeee; } knob { radius: 4px; }").unwrap();
        let device = Sheet::parse("knob#cutoff { color: var(--accent); radius: 8px !important; }").unwrap();
        let user = Sheet::parse("#cutoff { radius: 12px; }").unwrap();
        let styles = compute(
            &tree(),
            viewport(1138.0),
            &[
                AppliedSheet { sheet: &app, layer: Layer::Theme, root: None },
                AppliedSheet { sheet: &device, layer: Layer::Device, root: Some(1) },
                AppliedSheet { sheet: &user, layer: Layer::User, root: Some(1) },
            ],
        )
        .unwrap();
        assert_eq!(styles[2].get(Property::Color), Value::Color(Color::Rgba([121, 226, 189, 255])));
        assert_eq!(styles[2].get(Property::Radius), Value::Length(Length::Px(12.0)));
        assert_eq!(styles[4].get(Property::Color), Value::Color(Color::Rgba([238, 238, 238, 255])));
        assert_eq!(styles[4].get(Property::Radius), Value::Length(Length::Px(4.0)));
    }

    #[test]
    fn viewport_changes_and_inherited_variables_recompute_without_stale_values() {
        let sheet = Sheet::parse("window { --ink: #123456; --inherited: var(--ink); } device-panel { --ink: #abcdef; } knob { color: var(--inherited); width: 50%; } @media (max-width: 900px) { knob { color: var(--ink); } }").unwrap();
        assert!(sheet.notices.is_empty(), "{:?}", sheet.notices);
        let applied = [AppliedSheet { sheet: &sheet, layer: Layer::Theme, root: None }];
        let wide = compute(&tree(), viewport(1138.0), &applied).unwrap();
        let narrow = compute(&tree(), viewport(800.0), &applied).unwrap();
        assert_eq!(wide[2].get(Property::Color), Value::Color(Color::Rgba([18, 52, 86, 255])));
        assert_eq!(narrow[2].get(Property::Color), Value::Color(Color::Rgba([171, 205, 239, 255])));
        assert_eq!(wide[2].get(Property::Width), Value::Length(Length::Percent(0.5)));
    }

    #[test]
    fn malformed_declarations_report_locations_and_do_not_erase_valid_ones() {
        let sheet =
            Sheet::parse("knob { color: red; radius: NaNpx; opacity: 0.5; }\nknob { imaginary: 1; color: wat; }")
                .unwrap();
        assert_eq!(sheet.notices.len(), 3);
        assert_eq!(sheet.notices[1].line, 2);
        let values =
            compute(&tree(), viewport(800.0), &[AppliedSheet { sheet: &sheet, layer: Layer::Theme, root: None }])
                .unwrap();
        assert_eq!(values[2].get(Property::Color), Value::Color(Color::Rgba([255, 0, 0, 255])));
        assert_eq!(values[2].get(Property::Opacity), Value::Number(0.5));
        assert!(compute(&tree(), viewport(f32::NAN), &[]).is_err());
    }

    #[test]
    fn relative_font_size_is_computed_once_before_inheritance() {
        let sheet = Sheet::parse("window { font-size: 10px; } device-panel { font-size: 2em; }").unwrap();
        let styles =
            compute(&tree(), viewport(800.0), &[AppliedSheet { sheet: &sheet, layer: Layer::Theme, root: None }])
                .unwrap();
        assert_eq!(styles[1].get(Property::FontSize), Value::Length(Length::Px(20.0)));
        assert_eq!(styles[2].get(Property::FontSize), Value::Length(Length::Px(20.0)));
    }

    #[test]
    fn cascade_layers_order_normal_and_reverse_important_declarations() {
        let color = |css: &str| {
            let sheet = Sheet::parse(css).unwrap_or_else(|e| panic!("{e}"));
            assert!(sheet.notices.is_empty(), "{:?}", sheet.notices);
            let styles =
                compute(&tree(), viewport(800.0), &[AppliedSheet { sheet: &sheet, layer: Layer::Theme, root: None }])
                    .unwrap_or_else(|e| panic!("{e}"));
            styles[2].get(Property::Color)
        };
        let red = Value::Color(Color::Rgba([255, 0, 0, 255]));
        let blue = Value::Color(Color::Rgba([0, 0, 255, 255]));
        // A later layer wins over an earlier one whatever the specificity.
        assert_eq!(color("@layer a { #cutoff { color: red; } } @layer b { knob { color: blue; } }"), blue);
        // The order statement decides, not where the rules appear.
        assert_eq!(color("@layer b, a; @layer a { knob { color: red; } } @layer b { knob { color: blue; } }"), red);
        // Unlayered rules win over layered ones.
        assert_eq!(color("knob { color: blue; } @layer a { #cutoff { color: red; } }"), blue);
        // !important reverses: the earliest layer's important wins, unlayered important loses.
        assert_eq!(
            color("@layer a { knob { color: red !important; } } @layer b { knob { color: blue !important; } }"),
            red
        );
        assert_eq!(color("knob { color: blue !important; } @layer a { knob { color: red !important; } }"), red);
        // A layer's own rules win over its nested layers.
        assert_eq!(color("@layer a { knob { color: red; } @layer b { knob { color: blue; } } }"), red);
        assert_eq!(
            color("@layer a.b; @layer a { @layer b { knob { color: blue; } } } @layer c { knob { color: red; } }"),
            red
        );
        // Media inside a layer keeps the layer.
        assert_eq!(
            color("@layer a { @media (max-width: 900px) { knob { color: red; } } } @layer b { knob { color: blue; } }"),
            blue
        );
    }

    #[test]
    fn invalid_or_excessive_layers_are_notices_not_crashes() {
        let sheet = Sheet::parse("@layer 1bad { knob { color: red; } } knob { color: blue; }").unwrap_or_default();
        assert_eq!(sheet.notices.len(), 1);
        let many: String = (0..80).map(|i| format!("@layer l{i} {{ knob {{ color: red; }} }}")).collect();
        assert!(Sheet::parse(&many).is_err());
    }

    #[test]
    fn background_shorthand_sets_colour_and_images_and_resets_an_omitted_colour() {
        let style = |css: &str| {
            let sheet = Sheet::parse(css).unwrap_or_else(|e| panic!("{e}"));
            let styles =
                compute(&tree(), viewport(800.0), &[AppliedSheet { sheet: &sheet, layer: Layer::Theme, root: None }])
                    .unwrap_or_else(|e| panic!("{e}"));
            (sheet.notices.len(), styles[2].clone())
        };
        let (notices, knob) = style("knob { background-color: red; background: linear-gradient(red, blue); }");
        assert_eq!(notices, 0);
        // As in CSS, the shorthand resets the colour it omits: the native fill becomes transparent.
        assert_eq!(knob.get(Property::BackgroundColor), Value::Color(Color::Rgba([0; 4])));
        assert!(knob.authored(Property::BackgroundColor));
        assert!(matches!(knob.get(Property::BackgroundImage), Value::Gradients(g) if g.len() == 1));
        let (_, knob) =
            style(":root { --base: #102030; } knob { background: linear-gradient(red, blue), var(--base); }");
        assert_eq!(knob.get(Property::BackgroundColor), Value::Color(Color::Rgba([16, 32, 48, 255])));
        let (_, knob) = style("knob { background: none; }");
        assert_eq!(knob.get(Property::BackgroundImage), Value::Gradients(Vec::new()));
        for bad in
            ["url(a.png)", "red center / cover", "red, blue", "linear-gradient(red, blue) linear-gradient(red, blue)"]
        {
            let (notices, _) = style(&format!("knob {{ background: {bad}; }}"));
            assert_eq!(notices, 1, "{bad}");
        }
        // Background longhands written later still win over the shorthand.
        let (_, knob) = style("knob { background: red; background-color: blue; }");
        assert_eq!(knob.get(Property::BackgroundColor), Value::Color(Color::Rgba([0, 0, 255, 255])));
    }

    #[test]
    fn keyframes_resolve_for_each_element_with_its_variables() {
        let css = ":root { --to: blue; } knob { animation: ink 1s linear forwards; } \
                   @keyframes ink { from { color: red; } 50%, to { color: var(--to); opacity: .5; } }";
        let sheet = Sheet::parse(css).unwrap_or_else(|e| panic!("{e}"));
        assert!(sheet.notices.is_empty(), "{:?}", sheet.notices);
        let styles =
            compute(&tree(), viewport(800.0), &[AppliedSheet { sheet: &sheet, layer: Layer::Theme, root: None }])
                .unwrap_or_else(|e| panic!("{e}"));
        let animations = styles[2].animations();
        assert_eq!(animations.len(), 1);
        let frames = &animations[0].1;
        assert_eq!(frames.iter().map(|f| f.0).collect::<Vec<_>>(), [0.0, 0.5, 1.0]);
        assert_eq!(frames[2].1[0], (Property::Color, Value::Color(Color::Rgba([0, 0, 255, 255]))));
        assert!(styles[2].authored(Property::Opacity), "a keyframe-only property counts as written");
        assert!(styles[1].animations().is_empty());
        // An unknown name animates nothing; a bad offset is a notice.
        let sheet = Sheet::parse("knob { animation: missing 1s; } @keyframes x { 200% { color: red; } }").unwrap();
        assert_eq!(sheet.notices.len(), 1);
    }
}
