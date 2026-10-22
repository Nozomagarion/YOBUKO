// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A documented, bounded CSS subset for native controls. No browser or executable content.
use super::{skin, theme::Theme};
use serde_json::json;

pub const MAX_CSS: usize = 128 * 1024;

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
    let mut rest = source.as_str();
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
                "meter" | ".meter" if matches!(property, "style" | "segments") => match property {
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
    warnings
}

/// Canonical CSS is generated from the same model the theme editor changes.
pub fn export(theme: &Theme) -> String {
    let mut css = String::from("/* Suhina native theme CSS. See README.md for supported properties. */\n:root {\n");
    for (key, _, color) in theme.colors.tokens() {
        css.push_str(&format!("  --{}: {};\n", key.replace('.', "-"), super::theme::hex_rgba(theme.rgba(key, color))));
    }
    css.push_str(&format!("}}\nmeter {{ style: {}; segments: {}; }}\n", theme.skin.meter.name(), theme.skin.segments));
    if let Some(toolbar) = &theme.skin.toolbar {
        css.push_str(&format!(
            "toolbar {{ order: {}; hidden: {}; }}\n",
            toolbar.order.join(", "),
            if toolbar.hidden.is_empty() { "none".into() } else { toolbar.hidden.join(", ") }
        ));
    }
    for (selector, properties) in &theme.skin.styles {
        css.push_str(&format!("{selector} {{"));
        for (key, value) in properties {
            css.push_str(&format!(" {key}: {value};"));
        }
        css.push_str(" }\n");
    }
    for role in theme.skin.assets.keys() {
        // Package export writes each inline SVG under this stable relative name.
        css.push_str(&format!(
            "{} {{ image: url(\"assets/{role}.svg\"); }}\n",
            role.replace(".hover", ":hover").replace(".active", ":active").replace(".disabled", ":disabled")
        ));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn css_changes_native_model_and_reports_unsupported_rules() {
        let mut theme = Theme::default();
        let warnings = apply(
            &mut theme,
            "/* own style */ :root { --accent: #123456; } meter {style: segmented; segments: 16;} toolbar {order: transport, tempo; hidden: performance;} fader.cap:hover {image: url('assets/cap.svg');} body {display: grid;}",
        );
        assert_eq!(theme.colors.accent, egui::Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!(theme.skin.meter, skin::MeterStyle::Segmented);
        assert_eq!(theme.skin.toolbar.as_ref().unwrap().order[0], "transport");
        assert_eq!(theme.skin.assets["fader.cap.hover"], "assets/cap.svg");
        assert_eq!(warnings.len(), 1);
        let mut restored = Theme::default();
        assert!(apply(&mut restored, &export(&theme)).is_empty());
        assert_eq!(restored, theme);
    }
    #[test]
    fn malformed_css_does_not_panic_or_load_resources() {
        for css in [
            "/*",
            "meter {",
            "@import 'https://example.test/style';",
            "play {image:url(../../x.svg);}",
            "meter {segments: 999999;}",
        ] {
            assert!(!apply(&mut Theme::default(), css).is_empty(), "{css}");
        }
    }
}
