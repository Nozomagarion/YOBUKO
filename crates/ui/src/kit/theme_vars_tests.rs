// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::kit::theme::{Theme, built_in_themes, parse_hex};
use crate::kit::theme_css;

/// A theme read back from the sheet written for it, as the theme files do it.
fn from_sheet(text: &str) -> (Theme, Vec<String>) {
    let (declared, base, mut warnings) = read(text).unwrap();
    let mut theme = Theme::default();
    theme.vars.declared = Arc::new(declared);
    theme.vars.base = Arc::new(base);
    warnings.extend(resolve(&mut theme));
    warnings.extend(theme_css::read_host_rules(&mut theme, text));
    theme.skin.css = Some(Arc::from(text));
    (theme, warnings)
}

#[test]
fn the_core_set_registers_every_token_and_setting_with_suhina_darks_values() {
    let core = core();
    for (key, _, _) in crate::kit::theme::Palette::DARK.tokens() {
        assert!(is_core(&token_property(key)), "{key}");
    }
    for name in ["--meter-style", "--liquid", "--gradient-panels", "--background-image", "--session-slot-height"] {
        assert!(is_core(name), "{name}");
    }
    assert!(core.iter().all(|p| p.label.is_none()), "core properties have their own editors");
    let mut theme = Theme::default();
    assert!(apply_core(&mut theme, &core_values(&Theme::default())).is_empty());
    assert_eq!(theme, Theme::default());
}

#[test]
fn every_built_in_theme_reads_back_from_its_own_sheet() {
    for (id, original) in built_in_themes() {
        let text = theme_css::sheet(&original, original.skin.css.as_deref());
        let (theme, warnings) = from_sheet(&text);
        assert!(warnings.is_empty(), "{id}: {warnings:?}");
        assert_eq!(core_values(&theme), core_values(&original), "{id}");
        assert_eq!(theme.name, original.name, "{id}");
    }
}

#[test]
fn a_variant_sets_values_over_the_theme_and_may_use_its_variables() {
    let text = "@property --glow { syntax: \"<number>\"; initial-value: 0.6; label: \"Glow\"; range: 0 1; }\n\
                :root { --nord-blue: #88C0D0; --accent: #5E81AC; }\n\
                .button:hover { opacity: var(--glow); }";
    let (theme, warnings) = from_sheet(text);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(theme.vars.declared.len(), 1);
    assert_eq!(theme.colors.accent, parse_hex("#5E81AC").unwrap());
    let (frost, warnings) = Variant::from_json(
        "frost",
        r##"{ "name": "Frost", "appearance": "light",
              "values": { "--accent": "var(--nord-blue)", "--glow": "0.2", "--meter-segments": "99" } }"##,
    )
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(frost.light, Some(true));
    let (chosen, warnings) = with_variant(&theme, Some(&frost));
    assert_eq!(chosen.colors.accent, parse_hex("#88C0D0").unwrap());
    assert_eq!(chosen.vars.variant.as_deref(), Some("frost"));
    // A value outside its property's range is left out, with a warning.
    assert!(warnings.iter().any(|w| w.contains("--meter-segments")), "{warnings:?}");
    assert_eq!(chosen.skin.segments, theme.skin.segments);
    let sheet = chosen.skin.variant.as_deref().unwrap();
    assert!(sheet.contains("--glow: 0.2") && !sheet.contains("--meter-segments"), "{sheet}");
    // Without a variant, the theme's own values.
    let (own, _) = with_variant(&chosen, None);
    assert_eq!(own.colors.accent, theme.colors.accent);
    assert!(own.skin.variant.is_none());
}

#[test]
fn a_variant_value_for_an_undeclared_property_is_refused() {
    let (theme, _) = from_sheet(":root { --accent: #123456; }");
    let (variant, _) =
        Variant::from_json("odd", r##"{ "name": "Odd", "values": { "--nobody": "1", "--accent": "#ABCDEF" } }"##)
            .unwrap();
    let (chosen, warnings) = with_variant(&theme, Some(&variant));
    assert!(warnings.iter().any(|w| w.contains("--nobody")), "{warnings:?}");
    assert_eq!(chosen.colors.accent, parse_hex("#ABCDEF").unwrap());
    assert!(!chosen.vars.values.contains_key("--nobody"));
}

#[test]
fn editing_the_model_writes_only_the_values_that_differ_from_the_theme() {
    let (mut theme, _) = from_sheet(":root { --accent: #123456; --meter-style: needle; }");
    sync(&mut theme);
    assert!(theme.vars.values.is_empty(), "{:?}", theme.vars.values);
    theme.colors.accent = parse_hex("#ABCDEF").unwrap();
    theme.skin.segments = 32;
    sync(&mut theme);
    assert_eq!(theme.vars.values.keys().map(String::as_str).collect::<Vec<_>>(), ["--accent", "--meter-segments"]);
    // Back to the theme's own value: the variant no longer sets it.
    theme.colors.accent = parse_hex("#123456").unwrap();
    sync(&mut theme);
    assert_eq!(theme.vars.values.keys().map(String::as_str).collect::<Vec<_>>(), ["--meter-segments"]);
}

#[test]
fn variant_files_round_trip_and_bad_ones_are_named() {
    let variant = Variant {
        id: "polar-night".into(),
        name: "Polar Night".into(),
        light: Some(false),
        values: [("--accent".to_string(), "#81A1C1".to_string())].into(),
    };
    let (read, warnings) = Variant::from_json("polar-night", &variant.to_json()).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(read, variant);
    assert!(Variant::from_json("Bad Id", "{}").is_err());
    assert!(Variant::from_json("x", "[1]").is_err());
    let (lenient, warnings) =
        Variant::from_json("x", r##"{ "name": "", "appearance": "dim", "values": { "accent": "#fff", "--a": "}" } }"##)
            .unwrap();
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    assert!(lenient.values.is_empty() && lenient.light.is_none());
}

#[test]
fn host_rules_are_read_from_a_sheet_and_the_rest_is_left_to_the_engine() {
    let text = "/* notes */ theme { name: \"Night\"; }\n\
                @media (width < 600px) { .control-mixer-pan { display: none; } }\n\
                @layer x { .browser { color: red; } }\n\
                knob.body { image: url(\"assets/knob.body.svg\"); }\n\
                .control-mixer-pan { left: 4px; top: 0px; }\n\
                .pan { --suhina-appearance: knob; color: #123456; }\n\
                .button:hover { opacity: .8; }";
    let mut theme = Theme::default();
    let warnings = theme_css::read_host_rules(&mut theme, text);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(theme.name, "Night");
    assert_eq!(theme.skin.assets.get("knob.body").map(String::as_str), Some("assets/knob.body.svg"));
    assert_eq!(theme.skin.controls["mixer-pan"].offset().x, 4.0);
    assert!(!theme.skin.controls["mixer-pan"].hidden, "a rule in @media is not a placement");
    assert_eq!(theme.skin.styles[".pan"].len(), 1, "only --suhina-* styles are the app's");
}
