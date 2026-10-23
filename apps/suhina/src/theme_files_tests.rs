// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("suhina-theme-files-{name}-{}", std::process::id()));
    let _fresh = std::fs::remove_dir_all(&dir);
    dir
}

fn night() -> Theme {
    let mut theme = Theme { name: "Night".into(), ..Theme::default() };
    theme.colors.accent = suhina_ui::kit::theme::parse_hex("#123456").unwrap_or_default();
    theme
}

#[test]
fn themes_are_written_read_and_seen_when_they_change() {
    let dir = folder("round-trip");
    let mut files = ThemeFiles::new(Some(dir.join("themes")));
    assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
    assert_eq!(files.write("night", &night()), Ok(()));
    assert!(files.write("../night", &night()).is_err());
    // Written here, so not read back as a change.
    assert!(files.changed().0.is_empty());

    let themes = dir.join("themes");
    std::fs::write(themes.join("b-bad.json"), "{ \"colors\": { \"accent\": \"red\" } }").unwrap_or_default();
    std::fs::write(themes.join("notes.txt"), "not a theme").unwrap_or_default();
    let (entries, warnings) = files.read_all();
    let ids: Vec<_> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["b-bad", "night"]);
    assert!(entries.get(1).is_some_and(|e| same_look(&e.theme, &night())));
    // The theme with a colour it cannot read, and that it stays JSON until it reads whole.
    assert_eq!(warnings.len(), 2, "{warnings:?}");

    files.checked = None;
    std::fs::write(themes.join("c-new.json"), "{ \"name\": \"New\" }").unwrap_or_default();
    let (changed, _) = files.changed();
    assert_eq!(changed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["c-new"]);
    let _clean = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_renamed_theme_keeps_its_file() {
    let dir = folder("rename");
    let mut files = ThemeFiles::new(Some(dir.join("themes")));
    assert_eq!(files.rename("never-saved", "other"), Ok(()));
    assert_eq!(files.write("night", &night()), Ok(()));
    assert_eq!(files.write("day", &Theme::default()), Ok(()));
    assert!(files.rename("night", "day").is_err());
    assert_eq!(files.rename("night", "deep-sea"), Ok(()));
    let ids: Vec<_> = files.read_all().0.into_iter().map(|e| e.id).collect();
    assert_eq!(ids, ["day", "deep-sea"]);
    let _clean = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_backup_brings_the_themes_back() {
    let dir = folder("backup");
    let mut files = ThemeFiles::new(Some(dir.join("themes")));
    assert!(files.backup().is_err(), "nothing to back up yet");
    assert_eq!(files.write("night", &night()), Ok(()));
    let backup = files.backup().unwrap_or_default();
    assert!(backup.starts_with(dir.join("exports")), "{}", backup.display());

    // A backup holds each theme's folder as it is.
    let mut other = ThemeFiles::new(Some(dir.join("other")));
    assert_eq!(other.import(&backup), Ok((vec!["night".to_string()], Vec::new())));
    let (restored, _) = other.read_all();
    assert_eq!(restored.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["night"]);
    assert!(same_look(&restored[0].theme, &night()));
    // The same themes again are not doubled; a different one with a taken id gets its own.
    assert_eq!(other.import(&backup), Ok((Vec::new(), Vec::new())));
    let changed = Theme { name: "Night 2".into(), ..night() };
    let file = dir.join("night.json");
    std::fs::write(&file, changed.to_json()).unwrap_or_default();
    assert_eq!(other.import(&file).map(|r| r.0), Ok(vec!["night-2".to_string()]));
    let _clean = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_template_holds_a_theme_a_readme_and_examples() {
    let dir = folder("template");
    let files = ThemeFiles::new(Some(dir.join("themes")));
    let zip = files.export_template().unwrap_or_default();
    let file = std::fs::File::open(&zip).unwrap_or_else(|e| panic!("{e}"));
    let mut archive = zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("{e}"));
    let mut names: Vec<String> = archive.file_names().filter_map(Result::ok).map(|n| n.to_string()).collect();
    names.sort();
    for name in ["README.md", "my-theme/theme.css", "my-theme/variants/warm.json", "examples/nord/theme.css"] {
        assert!(names.contains(&name.to_string()), "{name}: {names:?}");
    }
    assert!(!names.iter().any(|n| n.ends_with(".json") && !n.contains("/variants/")), "{names:?}");
    let mut template = String::new();
    archive
        .by_name("my-theme/theme.css")
        .unwrap_or_else(|e| panic!("{e}"))
        .read_to_string(&mut template)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(theme::from_sheet(&template, "").1, Vec::<String>::new());
    // The editable example is importable; reference palettes remain examples only.
    let mut into = ThemeFiles::new(Some(dir.join("into")));
    let (ids, warnings) = into.import(&zip).unwrap();
    assert_eq!(ids, ["my-theme"]);
    assert!(warnings.is_empty());
    let (entries, warnings) = into.read_all();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["my-theme", "my-theme@warm"]);
    let _clean = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_is_not_a_theme_is_refused() {
    let dir = folder("refused");
    std::fs::create_dir_all(&dir).unwrap_or_default();
    let mut files = ThemeFiles::new(Some(dir.join("themes")));
    let huge = dir.join("huge.json");
    std::fs::write(&huge, vec![b' '; MAX_FILE as usize + 1]).unwrap_or_default();
    assert!(files.import(&huge).is_err());
    let list = dir.join("list.json");
    std::fs::write(&list, "[1, 2]").unwrap_or_default();
    assert!(files.import(&list).is_err());
    let fake = dir.join("fake.zip");
    std::fs::write(&fake, "not a zip").unwrap_or_default();
    assert!(files.import(&fake).is_err());
    assert!(files.read_all().0.is_empty());
    let _clean = std::fs::remove_dir_all(&dir);
}

#[test]
fn huge_files_in_the_folder_are_skipped() {
    let dir = folder("huge");
    std::fs::create_dir_all(&dir).unwrap_or_default();
    std::fs::write(dir.join("huge.json"), vec![b' '; MAX_FILE as usize + 1]).unwrap_or_default();
    let (entries, warnings) = ThemeFiles::new(Some(dir.clone())).read_all();
    assert!(entries.is_empty());
    assert_eq!(warnings.len(), 1);
    let _clean = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_folder_nothing_is_read_or_written() {
    let mut files = ThemeFiles::new(None);
    assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
    assert!(files.write("night", &Theme::default()).is_err());
    assert!(files.backup().is_err() && files.export_template().is_err() && files.open_folder().is_err());
    assert_eq!(files.folder_text(), None);
}

#[test]
fn dates_are_written_year_first() {
    let day = |d: u64| date(SystemTime::UNIX_EPOCH + Duration::from_secs(d * 86_400));
    assert_eq!(day(0), "1970-01-01");
    assert_eq!(day(11_016), "2000-02-29");
    assert_eq!(day(20_738), "2026-10-12");
}

#[test]
fn a_folder_without_theme_css_and_two_themes_with_one_name_are_named() {
    let dir = folder("not-themes");
    let themes = dir.join("themes");
    for (id, file, text) in [
        ("hidden-extension", "theme.css.txt", "theme { name: \"Mine\"; }"),
        ("first", "theme.css", "theme { name: \"Twin\"; }"),
        ("second", "theme.css", "theme { name: \"twin\"; }"),
    ] {
        std::fs::create_dir_all(themes.join(id)).unwrap();
        std::fs::write(themes.join(id).join(file), text).unwrap();
    }
    let mut files = ThemeFiles::new(Some(themes));
    let (entries, warnings) = files.read_all();
    let mut ids: Vec<_> = entries.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["first", "second"]);
    assert!(warnings.iter().any(|w| w.contains("hidden-extension") && w.contains("theme.css.txt")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("two themes are named")), "{warnings:?}");
    let _clean = std::fs::remove_dir_all(&dir);
}
