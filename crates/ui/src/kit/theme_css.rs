// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A documented, bounded CSS subset for native controls. No browser or executable content.
use super::{skin, theme::Theme};
use serde_json::json;

pub use suhina_style::edit as source;

pub const MAX_CSS: usize = 128 * 1024;

/// The file an `@import "name.css"` statement names, when it is a relative `.css` path
/// inside the theme's folder; no URLs, absolute paths or `..`.
pub fn import(statement: &str) -> Option<String> {
    let rest = statement.trim().strip_prefix("@import")?.trim().trim_end_matches(';').trim();
    let rest = rest.strip_prefix("url(").and_then(|r| r.strip_suffix(')')).unwrap_or(rest).trim();
    let path = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| rest.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))?;
    (path.ends_with(".css")
        && path.len() <= 128
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|p| !p.is_empty() && p != "." && p != ".."))
    .then(|| path.to_owned())
}

/// The relative stylesheets `css` imports before its first rule, in order.
pub fn imports(css: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = css;
    loop {
        let trimmed = rest.trim_start();
        if let Some(comment) = trimmed.strip_prefix("/*") {
            let Some(end) = comment.find("*/") else { break };
            rest = &comment[end + 2..];
            continue;
        }
        let Some(statement) = trimmed.strip_prefix('@') else { break };
        let (Some(end), brace) = (statement.find(';'), statement.find('{')) else { break };
        if brace.is_some_and(|brace| brace < end) {
            break;
        }
        if let Some(path) = import(&format!("@{}", &statement[..end])) {
            result.push(path);
        }
        rest = &statement[end + 1..];
        if result.len() > 8 {
            break;
        }
    }
    result
}

/// Apply declarations in source order. Unsupported selectors/properties are visible warnings.
pub fn apply(theme: &mut Theme, css: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    if css.len() > MAX_CSS {
        return vec!["stylesheet exceeds 128 KiB".into()];
    }
    let mut source = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        source.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find("*/") else {
            return vec!["unterminated CSS comment".into()];
        };
        rest = &rest[start + 2 + end + 2..];
    }
    source.push_str(rest);
    // `@import "advanced.css";` and other statements are read by the theme loader, not here.
    let mut rest = source.as_str();
    while let Some(statement) = rest.trim_start().strip_prefix('@') {
        let (Some(end), brace) = (statement.find(';'), statement.find('{')) else { break };
        if brace.is_some_and(|brace| brace < end) {
            break;
        }
        if import(&format!("@{}", &statement[..end])).is_none() {
            warnings.push(format!("@{}: only @import of a .css file in the theme's folder is read", &statement[..end]));
        }
        rest = &statement[end + 1..];
    }
    let mut rules = 0;
    while !rest.trim().is_empty() {
        rules += 1;
        if rules > 128 {
            warnings.push("stylesheet exceeds 128 rules".into());
            break;
        }
        let Some((selector, after)) = rest.split_once('{') else {
            warnings.push("CSS rule needs {".into());
            break;
        };
        let Some((body, after)) = after.split_once('}') else {
            warnings.push("CSS rule needs }".into());
            break;
        };
        rest = after;
        let selector = selector.trim();
        let control_selector = selector.trim_start_matches('.');
        for declaration in body.split(';').filter(|s| !s.trim().is_empty()) {
            let Some((property, value)) = declaration.split_once(':') else {
                warnings.push(format!("{selector}: expected property: value"));
                continue;
            };
            let property = property.trim();
            let value = value.trim();
            let result = match selector {
                "session" | ".session" if super::session_geometry::SessionGeometry::limits(property).is_some() => {
                    theme.skin.session.set(property, value)
                }
                "theme" if property == "name" => {
                    let name = value.trim_matches(['"', '\'']);
                    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
                        Err("theme name must be 1..64 characters")
                    } else {
                        theme.name = name.into();
                        Ok(())
                    }
                }
                "liquid" if matches!(property, "panels" | "tracks" | "clips") => {
                    match value.trim_end_matches('%').parse::<u8>().ok().filter(|n| *n <= 100) {
                        Some(n) => {
                            let options =
                                theme.liquid.get_or_insert(super::liquid::Options { panels: 0, tracks: 0, clips: 0 });
                            match property {
                                "panels" => options.panels = n,
                                "tracks" => options.tracks = n,
                                _ => options.clips = n,
                            }
                            Ok(())
                        }
                        None => Err("liquid strength must be 0..100"),
                    }
                }
                "gradient-panels" | "gradient-tracks" | "gradient-clips" if matches!(property, "angle" | "stops") => {
                    let slot = match selector {
                        "gradient-panels" => &mut theme.gradients.panels,
                        "gradient-tracks" => &mut theme.gradients.tracks,
                        _ => &mut theme.gradients.clips,
                    };
                    let mut gradient = slot.clone().unwrap_or_default();
                    let result = if property == "angle" {
                        match value.trim_end_matches("deg").parse::<u16>().ok().filter(|a| *a <= 360) {
                            Some(angle) => {
                                gradient.angle = angle;
                                Ok(())
                            }
                            None => Err("gradient angle must be 0..360"),
                        }
                    } else {
                        let mut stops = Vec::new();
                        let mut valid = true;
                        for item in value.split(',') {
                            let mut words = item.split_whitespace();
                            let (Some(color), Some(position), None) = (words.next(), words.next(), words.next()) else {
                                valid = false;
                                break;
                            };
                            let rgba = super::theme::parse_rgba(color);
                            let position = position.trim_end_matches('%').parse::<u8>().ok().filter(|p| *p <= 100);
                            let (Some(rgba), Some(position)) = (rgba, position) else {
                                valid = false;
                                break;
                            };
                            stops.push(super::gradient::Stop {
                                position,
                                color: egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
                                transparent_rgb: (rgba[3] == 0).then_some([rgba[0], rgba[1], rgba[2]]),
                            });
                        }
                        if valid && (2..=8).contains(&stops.len()) {
                            gradient.stops = stops;
                            Ok(())
                        } else {
                            Err("gradient stops are 2..8 of `#colour position%`")
                        }
                    };
                    if result.is_ok() {
                        *slot = Some(gradient);
                    }
                    result
                }
                "artwork" if property == "fps" => match value.parse::<u8>().ok().filter(|fps| (1..=30).contains(fps)) {
                    Some(fps) => {
                        theme.skin.artwork_fps = fps;
                        Ok(())
                    }
                    None => Err("artwork fps must be 1..30"),
                },
                _ if selector.starts_with(".control-") => {
                    let name = selector.trim_start_matches(".control-");
                    if !super::control_layout::valid_name(name)
                        || (!theme.skin.controls.contains_key(name) && theme.skin.controls.len() >= 256)
                    {
                        Err("invalid or excessive native control placement")
                    } else {
                        let mut next = theme.skin.controls.get(name).copied().unwrap_or_default();
                        match next.set(property, value) {
                            Ok(()) => {
                                theme.skin.controls.insert(name.into(), next);
                                Ok(())
                            }
                            Err(error) => Err(error),
                        }
                    }
                }
                ":root" if property.starts_with("--") => {
                    let key = property.trim_start_matches("--").replace('-', ".");
                    match (theme.colors.token_mut(&key), super::theme::parse_rgba(value)) {
                        (Some(slot), Some(rgba)) => {
                            *slot = egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
                            theme.transparent_rgb.remove(&key);
                            if rgba[3] == 0 && rgba[..3] != [0, 0, 0] {
                                theme.transparent_rgb.insert(key, [rgba[0], rgba[1], rgba[2]]);
                            }
                            Ok(())
                        }
                        _ => Err("unknown colour token or invalid hex colour"),
                    }
                }
                ".window" | "window"
                    if matches!(property, "background-image" | "background-opacity" | "background-blur") =>
                {
                    match property {
                        "background-image" if value == "none" => {
                            theme.skin.background = None;
                            Ok(())
                        }
                        "background-image" => {
                            if let Some(path) = value.strip_prefix("url(").and_then(|s| s.strip_suffix(')')) {
                                let path = path.trim().trim_matches(['\'', '"']);
                                if path.ends_with(".png") && skin::asset_path(&path.replace(".png", ".svg")) {
                                    theme.skin.background = Some(std::sync::Arc::from(path));
                                    Ok(())
                                } else {
                                    Err("background-image must name a relative PNG")
                                }
                            } else {
                                Err("background-image requires url()")
                            }
                        }
                        _ => {
                            if let Some(n) = value.trim_end_matches('%').parse::<u8>().ok().filter(|n| *n <= 100) {
                                if property == "background-opacity" {
                                    theme.skin.background_opacity = n;
                                } else {
                                    theme.skin.background_blur = n;
                                }
                                Ok(())
                            } else {
                                Err("background opacity/blur must be 0..100")
                            }
                        }
                    }
                }
                "meter" | ".meter" if matches!(property, "style" | "segments" | "intensity") => match property {
                    "intensity" => {
                        match value.strip_suffix('%').unwrap_or(value).parse::<u8>().ok().filter(|n| *n <= 100) {
                            Some(n) => {
                                theme.skin.meter_intensity = n;
                                Ok(())
                            }
                            None => Err("meter intensity must be 0..100%"),
                        }
                    }
                    "style" => match value {
                        "continuous" => {
                            theme.skin.meter = skin::MeterStyle::Continuous;
                            Ok(())
                        }
                        "segmented" => {
                            theme.skin.meter = skin::MeterStyle::Segmented;
                            Ok(())
                        }
                        "needle" => {
                            theme.skin.meter = skin::MeterStyle::Needle;
                            Ok(())
                        }
                        "frames" => {
                            theme.skin.meter = skin::MeterStyle::Frames;
                            Ok(())
                        }
                        _ => Err("unknown meter style"),
                    },
                    "segments" => match value.parse::<u8>().ok().filter(|n| (4..=64).contains(n)) {
                        Some(n) => {
                            theme.skin.segments = n;
                            Ok(())
                        }
                        None => Err("segments must be 4..64"),
                    },
                    _ => Err("unsupported meter property"),
                },
                "toolbar" | ".toolbar" if property.starts_with("offset-") => {
                    let module = property.trim_start_matches("offset-");
                    match value.trim_end_matches("px").parse::<i32>() {
                        Ok(offset) if skin::MODULES.contains(&module) && (-16384..=16384).contains(&offset) => {
                            theme.skin.toolbar_offsets.insert(module.into(), offset);
                            Ok(())
                        }
                        _ => Err("expected a known module offset from -16384 to 16384"),
                    }
                }
                "toolbar" | ".toolbar" if matches!(property, "order" | "hidden") => {
                    let list: Vec<_> =
                        value.split(',').map(str::trim).filter(|s| !s.is_empty() && *s != "none").collect();
                    if list.len() > skin::MODULES.len() || list.iter().any(|s| !skin::MODULES.contains(s)) {
                        Err("unknown toolbar module")
                    } else {
                        let current = theme.skin.toolbar.get_or_insert_with(Default::default);
                        let mut v = json!({"order":current.order,"hidden":current.hidden});
                        v[property] = json!(list);
                        let parsed = skin::Skin::from_json(&json!({"toolbar":v}), &mut warnings);
                        theme.skin.toolbar = parsed.toolbar;
                        Ok(())
                    }
                }
                _ if property == "image" => {
                    let role = control_selector.replace(':', ".").replace(" .", ".");
                    if !skin::valid_role(&role) {
                        Err("unsupported control selector")
                    } else if value == "none" {
                        theme.skin.assets.remove(&role);
                        Ok(())
                    } else if let Some(path) = value.strip_prefix("url(").and_then(|v| v.strip_suffix(')')) {
                        let path = path.trim().trim_matches(['\'', '"']);
                        if skin::asset_path(path) {
                            theme.skin.assets.insert(role, path.into());
                            Ok(())
                        } else {
                            Err("image must name a relative SVG file")
                        }
                    } else {
                        Err("image requires url(relative.svg) or none")
                    }
                }
                _ if super::style_class::valid_selector(selector)
                    && super::style_class::valid_property(property, value) =>
                {
                    theme
                        .skin
                        .styles
                        .entry(selector.to_string())
                        .or_default()
                        .insert(property.to_string(), value.to_string());
                    Ok(())
                }
                _ => Err("unsupported selector or property"),
            };
            if let Err(reason) = result {
                warnings.push(format!("CSS {selector} {{ {property}: {value} }}: {reason}"));
            }
        }
    }
    super::control_layout::validate_anchors(&mut theme.skin.controls, &mut warnings);
    warnings
}

/// Canonical CSS is generated from the same model the theme editor changes.
pub fn export(theme: &Theme) -> String {
    let mut css = String::from("/* Suhina native theme CSS. See README.md for supported properties. */\n:root {\n");
    for (key, _, color) in theme.colors.tokens() {
        css.push_str(&format!("  --{}: {};\n", key.replace('.', "-"), super::theme::hex_rgba(theme.rgba(key, color))));
    }
    css.push_str(&format!(
        "}}\nmeter {{ style: {}; segments: {}; intensity: {}%; }}\n",
        theme.skin.meter.name(),
        theme.skin.segments,
        theme.skin.meter_intensity
    ));
    css.push_str(&format!("artwork {{ fps: {}; }}\n", theme.skin.artwork_fps));
    css.push_str(&format!("theme {{ name: \"{}\"; }}\n", theme.name.replace(['"', '\\'], "")));
    if let Some(liquid) = theme.liquid {
        css.push_str(&format!(
            "liquid {{ panels: {}; tracks: {}; clips: {}; }}\n",
            liquid.panels, liquid.tracks, liquid.clips
        ));
    }
    for (name, gradient) in
        [("panels", &theme.gradients.panels), ("tracks", &theme.gradients.tracks), ("clips", &theme.gradients.clips)]
    {
        if let Some(gradient) = gradient {
            let stops: Vec<String> = gradient
                .stops
                .iter()
                .map(|stop| format!("{} {}%", super::theme::hex_rgba(stop.rgba()), stop.position))
                .collect();
            css.push_str(&format!("gradient-{name} {{ angle: {}; stops: {}; }}\n", gradient.angle, stops.join(", ")));
        }
    }
    for (name, value) in theme.skin.session.entries() {
        if let Some(value) = value {
            css.push_str(&format!("session {{ {name}: {value}px; }}\n"));
        }
    }
    for (name, placement) in &theme.skin.controls {
        css.push_str(&format!(".control-{name} {{ left: {}px; top: {}px;", placement.offset().x, placement.offset().y));
        css.push_str(&format!(
            " bottom: {}; display: {};",
            placement.bottom.map_or("auto".into(), |v| format!("{v}px")),
            if placement.hidden { "none" } else { "block" }
        ));
        css.push_str(&format!(" right: {};", placement.right.map_or("auto".into(), |v| format!("{v}px"))));
        for (key, value) in [("width", placement.width), ("height", placement.height)] {
            if key == "width" && placement.full_width {
                css.push_str(" width: 100%;");
                continue;
            }
            if key == "height" && placement.full_height {
                css.push_str(" height: 100%;");
                continue;
            }
            css.push_str(&format!(" {key}: {};", value.map_or("auto".into(), |v| format!("{v}px"))));
        }
        css.push_str(&format!(
            " --suhina-direction: {};",
            if placement.direction == super::control_layout::Direction::Horizontal { "horizontal" } else { "vertical" }
        ));
        for (edge, anchor) in placement.anchors.iter() {
            css.push_str(&format!(" {}: {anchor};", super::control_layout::anchor_property(edge)));
        }
        css.push_str(" }\n");
    }
    if let Some(toolbar) = &theme.skin.toolbar {
        css.push_str(&format!(
            "toolbar {{ order: {}; hidden: {}; }}\n",
            toolbar.order.join(", "),
            if toolbar.hidden.is_empty() { "none".into() } else { toolbar.hidden.join(", ") }
        ));
    }
    for (module, offset) in &theme.skin.toolbar_offsets {
        css.push_str(&format!("toolbar {{ offset-{module}: {offset}px; }}\n"));
    }
    if theme.skin.background.is_some() {
        css.push_str(&format!(
            ".window {{ background-image: url(\"background.png\"); background-opacity: {}; background-blur: {}; }}\n",
            theme.skin.background_opacity, theme.skin.background_blur
        ));
    }
    for (selector, properties) in &theme.skin.styles {
        css.push_str(&format!("{selector} {{"));
        for (key, value) in properties {
            css.push_str(&format!(" {key}: {value};"));
        }
        css.push_str(" }\n");
    }
    for (role, asset) in &theme.skin.assets {
        // Package export writes each inline SVG under this stable relative name.
        let path = if asset.trim_start().starts_with('<') || skin::png_artwork(asset) {
            format!("assets/{role}.{}", skin::artwork_extension(asset))
        } else {
            asset.clone()
        };
        css.push_str(&format!(
            "{} {{ image: url(\"{path}\"); }}\n",
            role.replace(".hover", ":hover").replace(".active", ":active").replace(".disabled", ":disabled"),
        ));
    }
    css
}

#[cfg(test)]
#[path = "theme_css_tests.rs"]
mod tests;
