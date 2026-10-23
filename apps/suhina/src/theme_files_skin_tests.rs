// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#[test]
fn editor_save_preserves_authored_css_comments_and_installed_guide() {
    let root = folder("authored-css-preservation");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    let mut theme = Theme::default();
    files.write("authored", &theme).unwrap();
    let css_path = root.join("themes/authored/theme.css");
    let source = format!("/* Revontuli — keep my notes */\n{}", std::fs::read_to_string(&css_path).unwrap());
    std::fs::write(&css_path, &source).unwrap();
    let guide = root.join("themes/authored/README.md");
    std::fs::write(&guide, "Author's own guide").unwrap();
    let (_, warnings) = files.read_all();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(std::fs::read_to_string(&css_path).unwrap(), source);
    let before = theme::hex_rgba(theme.rgba("accent", theme.colors.accent));
    theme.colors.accent = eframe::egui::Color32::from_rgb(18, 52, 86);
    files.write("authored", &theme).unwrap();
    assert_eq!(
        std::fs::read_to_string(&css_path).unwrap(),
        source.replacen(&format!("--accent: {before}"), "--accent: #123456", 1)
    );
    assert_eq!(std::fs::read_to_string(&guide).unwrap(), "Author's own guide");
    let _clean = std::fs::remove_dir_all(&root);
}

#[test]
fn palette_edits_do_not_rewrite_unchanged_package_resources() {
    let root = folder("palette-resources");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    let mut theme = theme::built_in_themes().into_iter().find(|(id, _)| *id == "suhina-sheet").unwrap().1;
    files.write("sheet", &theme).unwrap();
    let paths = ["themes/sheet/assets/button.svg", "themes/sheet/README.md", "themes/sheet/advanced.css"];
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(946_684_800);
    for path in paths {
        std::fs::File::options()
            .write(true)
            .open(root.join(path))
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    theme.colors.accent = eframe::egui::Color32::RED;
    files.write("sheet", &theme).unwrap();
    for path in paths {
        assert_eq!(std::fs::metadata(root.join(path)).unwrap().modified().unwrap(), old, "rewrote {path}");
    }
    assert_eq!(files.read_all().0[0].theme.colors.accent, eframe::egui::Color32::RED);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "manual disk and parsing timing for the theme slider reproduction"]
fn measure_theme_slider_saves() {
    let root = folder("slider-timing");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    for (id, mut theme) in theme::built_in_themes()
        .into_iter()
        .filter(|(id, _)| ["suhina-dark", "suhina-sheet", "suhina-blocks", "suhina-meadow"].contains(id))
    {
        files.write(id, &theme).unwrap();
        for value in 30..35 {
            theme.colors.accent = eframe::egui::Color32::from_rgb(value, 60, 90);
            let start = Instant::now();
            files.write(id, &theme).unwrap();
            eprintln!("slider-save {id} {value}: {:.3} ms", start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNQ6nD5DwADrAHuJHH6EQAAAABJRU5ErkJggg==";

#[test]
fn png_control_artwork_round_trips_and_live_corruption_preserves_saved_theme() {
    let root = folder("png-artwork");
    let mut theme = styled();
    theme.skin.assets.insert("track.name.color-00ff00.frame-0".into(), PNG.into());
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    files.write("portable", &theme).unwrap();
    let png = root.join("themes/portable/assets/track.name.color-00ff00.frame-0.png");
    assert_eq!(std::fs::read(&png).unwrap(), skin::decode_png(PNG).unwrap());
    assert_eq!(files.read_all().0[0].theme, theme);
    let package = portable_package_files("portable", &theme).unwrap();
    assert!(package.iter().any(|(name, _)| name.ends_with("frame-0.png")));
    let zip = root.join("portable.zip");
    write_zip(&zip, &package).unwrap();
    let mut imported = ThemeFiles::new(Some(root.join("imported")));
    imported.import(&zip).unwrap();
    assert_eq!(imported.read_all().0[0].theme.skin.assets["track.name.color-00ff00.frame-0"], PNG);
    let replacement = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4H6r0HwAGQwJ299oQdAAAAABJRU5ErkJggg==";
    std::fs::write(&png, skin::decode_png(replacement).unwrap()).unwrap();
    files.checked = None;
    let (changed, warnings) = files.changed();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(changed[0].theme.skin.assets["track.name.color-00ff00.frame-0"], replacement);
    let css = root.join("themes/portable/theme.css");
    let saved = std::fs::read(&css).unwrap();
    std::fs::write(&png, b"\x89PNG\r\n\x1fcorrupt").unwrap();
    files.checked = None;
    let (changed, warnings) = files.changed();
    assert!(changed.is_empty());
    assert!(!warnings.is_empty());
    assert_eq!(std::fs::read(css).unwrap(), saved);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn portable_animation_assets_leave_room_for_inherited_base_roles() {
    use super::*;
    let mut theme = Theme::default();
    let source = suhina_ui::kit::skin::default_assets()["button"].clone();
    for role in ["track.name", "track.body"] {
        for frame in 0..32 {
            theme.skin.assets.insert(format!("{role}.frame-{frame}"), source.clone());
        }
    }
    let files = portable_package_files("frames", &theme).unwrap();
    let (_, css) = files.iter().find(|(name, _)| name == "frames/theme.css").unwrap();
    let (restored, warnings) = resolve_css(std::str::from_utf8(css).unwrap(), |relative, _| {
        files
            .iter()
            .find(|(name, _)| name == &format!("frames/{relative}"))
            .map(|(_, data)| data.clone())
            .ok_or_else(|| format!("missing {relative}"))
    })
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(restored.skin.assets.len(), suhina_ui::kit::skin::MAX_PACKAGE_ASSETS);
}
use super::*;
use suhina_ui::kit::{skin, theme_css};
fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("suhina-theme-sync-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}
fn styled() -> Theme {
    let mut theme = Theme { name: "Portable".into(), ..Default::default() };
    theme.skin.assets = skin::example_assets();
    theme.skin.toolbar = Some(skin::Toolbar::default());
    theme.skin.toolbar_offsets.insert("transport".into(), 80);
    theme.skin.meter = skin::MeterStyle::Segmented;
    assert!(theme_css::apply(&mut theme, ".browser { color: #123456; background-color: #111222; }").is_empty());
    theme
}
#[test]
fn deleted_default_library_and_backup_are_recreated_from_embedded_svg() {
    let root = folder("default-delete");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
    let library = root.join("themes/_defaults");
    assert!(library.join("assets/fader.cap.svg").is_file());
    let backup = read_zip(&library.join("suhina-default.zip")).unwrap();
    assert_eq!(backup.len(), 1);
    assert_eq!(backup[0].1.skin.assets, skin::default_assets());
    std::fs::remove_dir_all(&library).unwrap();
    assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
    assert!(library.join("suhina-default.zip").is_file());
    assert_eq!(std::fs::read_to_string(library.join("assets/play.svg")).unwrap(), skin::default_assets()["play"]);
    std::fs::write(library.join("suhina-default.zip"), "damaged backup").unwrap();
    let (_, warnings) = files.read_all();
    assert!(warnings.iter().any(|w| w.contains("pristine backup restored")));
    assert_eq!(std::fs::read(library.join("suhina-default-backup-recovery-1.zip")).unwrap(), b"damaged backup");
    assert_eq!(read_zip(&library.join("suhina-default.zip")).unwrap()[0].1.skin.assets, skin::default_assets());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn default_edits_fork_numbered_themes_before_restoring_original_artwork() {
    let root = folder("default-fork");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    files.read_all();
    let play = root.join("themes/_defaults/assets/play.svg");
    let svg = |color| {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><rect width=\"20\" height=\"20\" fill=\"{color}\"/></svg>"
        )
    };
    for (number, color) in [(1, "#123456"), (2, "#abcdef")] {
        std::fs::write(&play, svg(color)).unwrap();
        files.checked = None;
        let (entries, warnings) = files.changed();
        assert_eq!(entries.len(), 1, "{warnings:?}");
        assert_eq!(entries[0].id, format!("suhina-default-modified-{number}"));
        assert_eq!(entries[0].theme.name, format!("Suhina Default (modified {number})"));
        assert_eq!(entries[0].theme.skin.assets["play"], svg(color));
        assert_eq!(std::fs::read_to_string(&play).unwrap(), skin::default_assets()["play"]);
        assert!(warnings.iter().any(|w| w.contains("original defaults restored")));
    }
    files.checked = None;
    assert!(files.changed().0.is_empty(), "restoration must not create reload loops");
    assert_eq!(files.read_all().0.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_default_svg_is_backed_up_before_restoring_the_embedded_source() {
    let root = folder("default-invalid");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    files.read_all();
    let play = root.join("themes/_defaults/assets/play.svg");
    std::fs::write(&play, "unfinished SVG artwork").unwrap();
    files.checked = None;
    let (entries, warnings) = files.changed();
    assert!(entries.is_empty());
    assert!(warnings.iter().any(|w| w.contains("preserved in")));
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(root.join("exports/suhina-default-recovery-1.zip")).unwrap()).unwrap();
    let mut saved = String::new();
    archive.by_name("assets/play.svg").unwrap().read_to_string(&mut saved).unwrap();
    assert_eq!(saved, "unfinished SVG artwork");
    assert_eq!(std::fs::read_to_string(play).unwrap(), skin::default_assets()["play"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn portable_packages_include_the_shared_ai_author_guide_and_effective_svg() {
    let theme = Theme::default();
    let files = portable_package_files("plain", &theme).unwrap();
    let guide = files.iter().find(|(name, _)| name == "plain/README.md").unwrap();
    assert_eq!(guide.1, theme::template_readme().as_bytes());
    let text = std::str::from_utf8(&guide.1).unwrap();
    for heading in [
        "What this package is",
        "why they are separate",
        "How and when",
        "What's possible now",
        "Limits",
        "For AI-assisted edits",
    ] {
        assert!(text.contains(heading), "{heading}");
    }
    assert!(files.iter().any(|(name, _)| name == "plain/assets/fader.cap.svg"));
    assert!(
        files
            .iter()
            .any(|(name, data)| name == "plain/examples/dark-gloss.css" && data == theme::GLOSS_EXAMPLE.as_bytes())
    );
    assert!(theme.skin.assets.is_empty(), "export must not modify sparse local overrides");
    let css = files.iter().find(|(name, _)| name == "plain/theme.css").unwrap();
    assert!(std::str::from_utf8(&css.1).unwrap().contains("play { image: url(\"assets/play.svg\"); }"));
    assert!(!files.iter().any(|(name, _)| name.ends_with(".json")), "packages are CSS");
}
#[test]
fn advanced_css_source_is_imported_verbatim_and_survives_packages() {
    let root = folder("advanced");
    let mut theme = styled();
    let source = "/* Keep my comment */\n@layer custom { :root { --ink: oklch(70% .1 240); } .browser { color:var(--ink); } }\n@media (width < 800px) { #performance { display:none; } }\n";
    theme.skin.css = Some(std::sync::Arc::from(source));
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    files.write("portable", &theme).unwrap();
    let css = root.join("themes/portable/advanced.css");
    assert_eq!(std::fs::read_to_string(&css).unwrap(), source);
    let main = std::fs::read_to_string(root.join("themes/portable/theme.css")).unwrap();
    assert!(main.starts_with("@import \"advanced.css\";\n"), "{main}");
    assert!(!root.join("themes/portable.json").exists(), "no JSON is written");
    let edited = source.replace("70%", "80%");
    std::fs::write(&css, &edited).unwrap();
    files.checked = None;
    let (changed, warnings) = files.changed();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(changed[0].theme.skin.css.as_deref(), Some(edited.as_str()));
    files.checked = None;
    assert!(files.changed().0.is_empty(), "reading must not trigger its own reload");
    let zip = files.backup().unwrap();
    let mut target = ThemeFiles::new(Some(root.join("imported")));
    assert!(target.import(&zip).unwrap().1.is_empty());
    assert_eq!(target.read_all().0[0].theme.skin.css.as_deref(), Some(edited.as_str()));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn external_theme_css_edits_load_and_editor_saves_keep_them_without_reload_loops() {
    let root = folder("css-source");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    let original = styled();
    files.write("portable", &original).unwrap();
    let css = root.join("themes/portable/theme.css");
    let css_text = format!(
        "/* My notes stay. */\n{}",
        std::fs::read_to_string(&css)
            .unwrap()
            .replace("--accent: #A6A3FF;", "--accent: #12AB34;")
            .replace("intensity: 100%", "intensity: 65%")
            .replace("offset-transport: 80px", "offset-transport: 96px")
    );
    assert!(css_text.contains("--accent: #12AB34;"));
    std::fs::write(&css, &css_text).unwrap();
    files.checked = None;
    let (changed, warnings) = files.changed();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].theme.colors.accent, theme::parse_hex("#12AB34").unwrap());
    assert_eq!(changed[0].theme.skin.meter_intensity, 65);
    assert_eq!(changed[0].theme.skin.toolbar_offsets["transport"], 96);
    files.checked = None;
    assert!(files.changed().0.is_empty(), "reading must not trigger its own reload");
    let mut edited = changed[0].theme.clone();
    edited.colors.accent = theme::parse_hex("#112233").unwrap();
    edited.skin.meter_intensity = 80;
    files.write("portable", &edited).unwrap();
    let saved = std::fs::read_to_string(&css).unwrap();
    assert!(saved.starts_with("/* My notes stay. */\n"), "{saved}");
    assert!(saved.contains("--accent: #112233;") && saved.contains("intensity: 80%"));
    assert!(saved.contains("offset-transport: 96px"));
    assert_eq!(files.read_all().0[0].theme, edited);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_save_over_unread_external_edits_is_refused_and_keeps_the_files() {
    let root = folder("conflict");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    let original = styled();
    files.write("portable", &original).unwrap();
    let css = root.join("themes/portable/theme.css");
    let external = std::fs::read_to_string(&css).unwrap().replace("--accent: #A6A3FF;", "--accent: #445566;");
    std::fs::write(&css, &external).unwrap();
    let mut ours = original.clone();
    ours.colors.accent = theme::parse_hex("#112233").unwrap();
    let error = files.write("portable", &ours).unwrap_err();
    assert!(error.contains("external edits"), "{error}");
    assert_eq!(std::fs::read_to_string(&css).unwrap(), external);
    // Once read, the external edit is the theme, and saving works again.
    let (entries, _) = files.read_all();
    assert_eq!(entries[0].theme.colors.accent, theme::parse_hex("#445566").unwrap());
    assert!(files.write("portable", &ours).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_json_theme_migrates_to_css_keeping_its_comments_and_its_originals() {
    let root = folder("migrate");
    let themes = root.join("themes");
    let mut theme = styled();
    theme.skin.css = Some(std::sync::Arc::from(".browser { color: #abcdef; }\n"));
    theme.liquid = Some(suhina_ui::kit::liquid::Options { panels: 30, tracks: 0, clips: 10 });
    // A theme as earlier versions wrote it: JSON beside a folder with style.css and artwork.
    std::fs::create_dir_all(themes.join("old/assets")).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&theme.to_json()).unwrap();
    json["stylesheet"] = serde_json::json!("old/style.css");
    json["advanced-stylesheet"] = serde_json::json!("old/advanced.css");
    if let Some(skin) = json["skin"].as_object_mut() {
        skin.remove("css");
    }
    for (role, source) in &theme.skin.assets {
        std::fs::write(themes.join(format!("old/assets/{role}.svg")), source).unwrap();
        json["skin"]["assets"][role] = serde_json::json!(format!("old/assets/{role}.svg"));
    }
    let json = serde_json::to_vec_pretty(&json).unwrap();
    std::fs::write(themes.join("old.json"), &json).unwrap();
    let style = format!("/* Revontuli notes */\n{}", theme_css::export(&theme));
    std::fs::write(themes.join("old/style.css"), &style).unwrap();
    std::fs::write(themes.join("old/advanced.css"), ".browser { color: #abcdef; }\n").unwrap();
    let mut files = ThemeFiles::new(Some(themes.clone()));
    let (entries, warnings) = files.read_all();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].theme, theme);
    let css = std::fs::read_to_string(themes.join("old/theme.css")).unwrap();
    assert!(css.starts_with("@import \"advanced.css\";\n/* Revontuli notes */"), "{css}");
    assert!(!themes.join("old.json").exists() && !themes.join("old/style.css").exists());
    assert_eq!(std::fs::read(themes.join("old/legacy/old.json")).unwrap(), json);
    assert_eq!(std::fs::read_to_string(themes.join("old/legacy/style.css")).unwrap(), style);
    // Read again: the CSS theme is the same, and nothing migrates twice.
    let (again, warnings) = ThemeFiles::new(Some(themes.clone())).read_all();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(again[0].theme, theme);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn the_user_sheet_is_read_when_it_changes_and_a_broken_one_is_an_error() {
    let root = folder("user-css");
    let themes = root.join("themes");
    std::fs::create_dir_all(&themes).unwrap();
    let mut files = ThemeFiles::new(Some(themes.clone()));
    assert_eq!(files.user_css(false), Some(Ok(None)), "no user.css yet");
    assert_eq!(files.user_css(false), None, "unchanged");
    std::fs::write(themes.join("user.css"), "#cutoff { color: red; }").unwrap();
    assert_eq!(files.user_css(false), Some(Ok(Some("#cutoff { color: red; }".into()))));
    assert_eq!(files.user_css(false), None);
    assert!(files.user_css(true).is_some(), "a full reload reads it again");
    std::fs::write(
        themes.join("user.css"),
        format!("{}a{{}}{}", "@media (min-width:1px) {".repeat(70), "}".repeat(70)),
    )
    .unwrap();
    assert!(matches!(files.user_css(false), Some(Err(_))));
    std::fs::remove_file(themes.join("user.css")).unwrap();
    assert_eq!(files.user_css(false), Some(Ok(None)));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_json_theme_that_cannot_round_trip_stays_json() {
    let root = folder("migrate-refused");
    let themes = root.join("themes");
    std::fs::create_dir_all(&themes).unwrap();
    // A JSON theme whose stylesheet has an invalid rule: CSS would lose it, so it is kept.
    let mut json: serde_json::Value = serde_json::from_str(&styled().to_json()).unwrap();
    json["stylesheet"] = serde_json::json!("kept/style.css");
    std::fs::create_dir_all(themes.join("kept")).unwrap();
    std::fs::write(themes.join("kept/style.css"), "meter { segments: 999; }").unwrap();
    std::fs::write(themes.join("kept.json"), serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    let (entries, warnings) = ThemeFiles::new(Some(themes.clone())).read_all();
    assert_eq!(entries.len(), 1, "the theme still loads from JSON");
    assert!(warnings.iter().any(|w| w.contains("kept as JSON")), "{warnings:?}");
    assert!(themes.join("kept.json").exists() && !themes.join("kept/theme.css").exists());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn example_builtins_export_their_css_svg_controls_and_background() {
    for id in ["suhina-sheet", "suhina-meadow", "suhina-blocks"] {
        let root = folder(id);
        let (_, theme) = theme::built_in_themes().into_iter().find(|(key, _)| *key == id).unwrap();
        let mut source = ThemeFiles::new(Some(root.join("source")));
        source.write("sheet-copy", &theme).unwrap();
        let zip = source.backup().unwrap();
        let mut target = ThemeFiles::new(Some(root.join("target")));
        let (ids, warnings) = target.import(&zip).unwrap();
        assert_eq!(ids, ["sheet-copy"]);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(target.read_all().0[0].theme, theme);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        for role in theme.skin.assets.keys() {
            assert!(archive.by_name(&format!("sheet-copy/assets/{role}.svg")).is_ok(), "{role}");
        }
        assert!(archive.by_name("sheet-copy/advanced.css").is_ok());
        assert!(archive.by_name("sheet-copy/theme.css").is_ok());
        assert!(archive.by_name("sheet-copy/README.md").is_ok());
        assert_eq!(archive.by_name("sheet-copy/background.png").is_ok(), id == "suhina-meadow");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn package_round_trip_contains_svg_css_and_png() {
    let root = folder("package");
    let mut theme = styled();
    let image = suhina_ui::kit::skin::encode_png(b"\x89PNG\r\n\x1a\nfake-data-for-portability-only").unwrap();
    theme.skin.background = Some(image);
    // Package storage validates the PNG signature; the image decoder separately validates its full contents.
    let mut source = ThemeFiles::new(Some(root.join("source")));
    source.write("portable", &theme).unwrap();
    let zip = source.backup().unwrap();
    let mut target = ThemeFiles::new(Some(root.join("target")));
    let (ids, warnings) = target.import(&zip).unwrap();
    assert_eq!(ids, ["portable"]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(target.read_all().0[0].theme, theme);
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
    assert!(archive.by_name("portable/theme.css").is_ok());
    assert!(archive.by_name("portable/assets/fader.cap.svg").is_ok());
    assert!(archive.by_name("portable/background.png").is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_package_does_not_write_any_themes() {
    let root = folder("invalid");
    let zip = root.join("bad.zip");
    write_zip(
        &zip,
        &[("good/good.json".into(), styled().to_json().into_bytes()), ("bad/bad.json".into(), b"{".to_vec())],
    )
    .unwrap();
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    assert!(files.import(&zip).is_err());
    assert!(files.read_all().0.is_empty());
    let zip = root.join("traversal.zip");
    write_zip(&zip, &[("../escape.json".into(), styled().to_json().into_bytes())]).unwrap();
    assert!(files.import(&zip).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn live_svg_edit_is_loaded_and_missing_svg_does_not_overwrite_the_theme() {
    let root = folder("svg");
    let mut files = ThemeFiles::new(Some(root.join("themes")));
    files.write("portable", &styled()).unwrap();
    let svg = root.join("themes/portable/assets/fader.cap.svg");
    let replacement = skin::example_assets()["stop"].clone();
    std::fs::write(&svg, &replacement).unwrap();
    files.checked = None;
    let (changed, warnings) = files.changed();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(changed[0].theme.skin.assets["fader.cap"], replacement);
    let css = root.join("themes/portable/theme.css");
    let saved = std::fs::read(&css).unwrap();
    std::fs::remove_file(svg).unwrap();
    files.checked = None;
    let (changed, warnings) = files.changed();
    assert!(changed.is_empty());
    assert!(!warnings.is_empty());
    assert_eq!(std::fs::read(&css).unwrap(), saved);
    std::fs::remove_dir_all(root).unwrap();
}
