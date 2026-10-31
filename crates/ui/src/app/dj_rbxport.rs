// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The "Open rbxport" button: hands over to the rbxport app itself (library management,
//! PRO DJ LINK, its own USB sync) when it is installed, or opens its download page.

use std::path::{Path, PathBuf};

/// Where to get rbxport when it is not installed.
pub(crate) const DOWNLOAD_PAGE: &str = "https://github.com/chrisle/rbxport/releases/latest";

/// The places rbxport's installers put it, for this system, most likely first.
pub(crate) fn candidates(home: Option<&Path>, env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.push(PathBuf::from("/Applications/rbxport.app"));
        if let Some(home) = home {
            out.push(home.join("Applications/rbxport.app"));
        }
    } else if cfg!(windows) {
        for var in ["ProgramFiles", "LOCALAPPDATA", "ProgramFiles(x86)"] {
            if let Some(base) = env(var) {
                out.push(Path::new(&base).join("rbxport").join("rbxport.exe"));
            }
        }
    } else {
        if let Some(path) = env("PATH") {
            out.extend(std::env::split_paths(&path).map(|dir| dir.join("rbxport")));
        }
        out.push(PathBuf::from("/usr/bin/rbxport"));
        out.push(PathBuf::from("/opt/rbxport/rbxport"));
        if let Some(home) = home {
            out.push(home.join(".local/bin/rbxport"));
            out.push(home.join("Applications/rbxport.AppImage"));
        }
    }
    out
}

/// The installed rbxport, if any.
pub(crate) fn installed() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    candidates(home.as_deref(), &|name| std::env::var(name).ok()).into_iter().find(|p| p.exists())
}

/// Starts rbxport, detached; YOBUKO keeps running.
pub(crate) fn launch(app: &Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-a").arg(app);
        c
    } else {
        std::process::Command::new(app)
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_looks_where_its_installer_puts_rbxport() {
        let home = Path::new("/home/dj");
        let env = |name: &str| match name {
            "ProgramFiles" => Some(r"C:\Program Files".to_owned()),
            "PATH" => Some("/usr/local/bin".to_owned()),
            _ => None,
        };
        let found = candidates(Some(home), &env);
        assert!(!found.is_empty());
        if cfg!(target_os = "macos") {
            assert_eq!(found[0], PathBuf::from("/Applications/rbxport.app"));
        } else if cfg!(windows) {
            assert_eq!(found[0], Path::new(r"C:\Program Files").join("rbxport").join("rbxport.exe"));
        } else {
            assert_eq!(found[0], PathBuf::from("/usr/local/bin/rbxport"));
            assert!(found.contains(&home.join(".local/bin/rbxport")));
        }
        assert!(DOWNLOAD_PAGE.starts_with("https://github.com/chrisle/rbxport"));
    }
}
