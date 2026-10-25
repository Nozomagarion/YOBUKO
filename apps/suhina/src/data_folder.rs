// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where Suhina keeps its data when the person has chosen a folder of their own (Settings ›
//! General › Settings folder). The settings live in that folder, so the choice cannot be one of
//! them: a small file in the default folder, `data-location.txt`, names it.

use std::path::{Path, PathBuf};

/// The file in the default data folder that names the folder in use.
const POINTER: &str = "data-location.txt";
/// What is copied into a new folder: the settings, keys, recent projects and themes.
const FILES: &[&str] = &["settings.txt", "keymap.json", "recent.txt"];
const THEMES: &str = "themes";
/// The most files and folders a copy walks, so a wrong folder cannot keep it busy.
const MAX_ENTRIES: usize = 20_000;

/// The folder `default`'s pointer names, when it names one that is there.
pub fn pointed(default: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(default.join(POINTER)).ok()?;
    let folder = PathBuf::from(text.trim());
    (!text.trim().is_empty() && folder.is_absolute() && folder.is_dir()).then_some(folder)
}

/// Points `default` at `folder` from the next start, or back at itself with `None`.
pub fn point(default: &Path, folder: Option<&Path>) -> Result<(), String> {
    let pointer = default.join(POINTER);
    match folder {
        Some(folder) => {
            std::fs::create_dir_all(default).map_err(|e| format!("could not make {}: {e}", default.display()))?;
            std::fs::write(&pointer, folder.display().to_string())
                .map_err(|e| format!("could not write {}: {e}", pointer.display()))
        }
        None => match std::fs::remove_file(&pointer) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("could not remove {}: {e}", pointer.display()))
            }
            _ => Ok(()),
        },
    }
}

/// Copies the settings, keys, recent projects and themes from `from` into `to`, made first,
/// leaving every file `to` has already as it is. Links are not followed. Returns how many files
/// were copied.
pub fn copy_over(from: &Path, to: &Path) -> Result<usize, String> {
    if from == to {
        return Ok(0);
    }
    std::fs::create_dir_all(to).map_err(|e| format!("could not make {}: {e}", to.display()))?;
    let mut copied = 0;
    let mut seen = 0;
    for name in FILES {
        copied += copy_file(&from.join(name), &to.join(name))?;
    }
    copy_tree(&from.join(THEMES), &to.join(THEMES), &mut copied, &mut seen)?;
    Ok(copied)
}

fn copy_file(from: &Path, to: &Path) -> Result<usize, String> {
    let ordinary = std::fs::symlink_metadata(from).is_ok_and(|m| m.is_file());
    if !ordinary || std::fs::symlink_metadata(to).is_ok() {
        return Ok(0);
    }
    std::fs::copy(from, to).map(|_| 1).map_err(|e| format!("could not copy {}: {e}", from.display()))
}

fn copy_tree(from: &Path, to: &Path, copied: &mut usize, seen: &mut usize) -> Result<(), String> {
    if !std::fs::symlink_metadata(from).is_ok_and(|m| m.is_dir()) {
        return Ok(());
    }
    std::fs::create_dir_all(to).map_err(|e| format!("could not make {}: {e}", to.display()))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("could not read {}: {e}", from.display()))?;
    for entry in entries.filter_map(Result::ok) {
        *seen += 1;
        if *seen > MAX_ENTRIES {
            return Err(format!("{} holds too many files to copy", from.display()));
        }
        let path = entry.path();
        let target = to.join(entry.file_name());
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.is_dir() {
            copy_tree(&path, &target, copied, seen)?;
        } else if meta.is_file() {
            *copied += copy_file(&path, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_folder_is_named_copied_into_and_forgotten() {
        let base = std::env::temp_dir().join(format!("suhina-data-folder-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&base);
        let (default, chosen) = (base.join("default"), base.join("chosen"));
        std::fs::create_dir_all(default.join("themes/night")).unwrap();
        std::fs::write(default.join("settings.txt"), "theme = night\n").unwrap();
        std::fs::write(default.join("themes/night/theme.css"), ":root {}").unwrap();
        assert_eq!(pointed(&default), None);
        assert_eq!(copy_over(&default, &chosen), Ok(2));
        assert!(chosen.join("themes/night/theme.css").is_file());
        // What the chosen folder has already is kept.
        std::fs::write(chosen.join("settings.txt"), "theme = day\n").unwrap();
        assert_eq!(copy_over(&default, &chosen), Ok(0));
        assert_eq!(std::fs::read_to_string(chosen.join("settings.txt")).unwrap(), "theme = day\n");
        assert_eq!(point(&default, Some(&chosen)), Ok(()));
        assert_eq!(pointed(&default), Some(chosen.clone()));
        assert_eq!(point(&default, None), Ok(()));
        assert_eq!(pointed(&default), None);
        // A folder that is gone is not used.
        point(&default, Some(&base.join("gone"))).unwrap();
        assert_eq!(pointed(&default), None);
        let _clean = std::fs::remove_dir_all(&base);
    }
}
