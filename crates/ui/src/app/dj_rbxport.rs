// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ mode is rbxport (Chris Le's DJ library app, vendored in `third_party/rbxport-app`):
//! where the copy shipped inside YOBUKO is, how it is started, and how a track rendered here
//! reaches its library (the inbox, read by `third_party/rbxport-app/src-tauri/src/yobuko.rs`).

use std::path::{Path, PathBuf};

/// Where to get rbxport when no copy is found (a build of YOBUKO made without it).
pub(crate) const DOWNLOAD_PAGE: &str = "https://github.com/chrisle/rbxport/releases/latest";

/// The rbxport shipped with YOBUKO, relative to YOBUKO's own program `exe`.
/// macOS: `YOBUKO.app/Contents/MacOS/suhina` → `YOBUKO.app/Contents/Helpers/rbxport.app`.
/// Windows and Linux: an `rbxport` folder beside the program.
pub(crate) fn bundled(exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new("."));
    if cfg!(target_os = "macos") {
        dir.parent().unwrap_or(dir).join("Helpers").join("rbxport.app")
    } else if cfg!(windows) {
        dir.join("rbxport").join("rbxport.exe")
    } else {
        dir.join("rbxport").join("rbxport")
    }
}

/// The places rbxport's own installers put it, for this system, most likely first.
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

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// The rbxport to start: the one shipped with YOBUKO first, else one installed on its own.
pub(crate) fn find() -> Option<PathBuf> {
    let shipped = std::env::current_exe().ok().map(|exe| bundled(&exe)).filter(|p| p.exists());
    shipped.or_else(|| candidates(home().as_deref(), &|name| std::env::var(name).ok()).into_iter().find(|p| p.exists()))
}

/// Starts rbxport, detached; YOBUKO keeps running. Started again, it is only brought forward.
pub(crate) fn launch(app: &Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(app);
        c
    } else {
        let mut c = std::process::Command::new(app);
        if let Some(dir) = app.parent() {
            c.current_dir(dir);
        }
        c
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

/// The folder rbxport imports requests from: the system's per-user data folder (what the
/// `dirs` crate calls `data_dir`) › YOBUKO › rbxport-inbox. Keep in step with `yobuko.rs`.
pub(crate) fn inbox(home: Option<&Path>, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let data = if cfg!(target_os = "macos") {
        home.map(|home| home.join("Library").join("Application Support"))
    } else if cfg!(windows) {
        env("APPDATA").map(PathBuf::from)
    } else {
        env("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|home| home.join(".local").join("share")))
    }?;
    Some(data.join("YOBUKO").join("rbxport-inbox"))
}

/// Asks rbxport to import `tracks`: a `.paths` request in `inbox`, written whole before it is
/// named so (rbxport reads only `.paths`). Returns the request.
pub(crate) fn send_to(inbox: &Path, tracks: &[PathBuf]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(inbox)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let text: String = tracks.iter().map(|t| format!("{}\n", t.display())).collect();
    let partial = inbox.join(format!("{stamp}.tmp"));
    let request = inbox.join(format!("{stamp}.paths"));
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, &request)?;
    Ok(request)
}

/// [`send_to`] the inbox of this user.
pub(crate) fn send(tracks: &[PathBuf]) -> std::io::Result<PathBuf> {
    let inbox = inbox(home().as_deref(), &|name| std::env::var(name).ok())
        .ok_or_else(|| std::io::Error::other("no folder for rbxport's inbox"))?;
    send_to(&inbox, tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_rbxport_sits_where_the_packages_put_it() {
        if cfg!(target_os = "macos") {
            let exe = Path::new("/Applications/YOBUKO.app/Contents/MacOS/suhina");
            assert_eq!(bundled(exe), Path::new("/Applications/YOBUKO.app/Contents/Helpers/rbxport.app"));
        } else if cfg!(windows) {
            let exe = Path::new(r"C:\Program Files\YOBUKO\suhina.exe");
            assert_eq!(bundled(exe), Path::new(r"C:\Program Files\YOBUKO\rbxport\rbxport.exe"));
        } else {
            assert_eq!(bundled(Path::new("/opt/yobuko/suhina")), Path::new("/opt/yobuko/rbxport/rbxport"));
        }
    }

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

    #[test]
    fn the_inbox_is_in_the_same_data_folder_rbxport_reads() {
        let home = Path::new("/home/dj");
        let env = |name: &str| match name {
            "APPDATA" => Some(r"C:\Users\dj\AppData\Roaming".to_owned()),
            _ => None,
        };
        let inbox = inbox(Some(home), &env).unwrap();
        assert!(inbox.ends_with(Path::new("YOBUKO").join("rbxport-inbox")));
        if cfg!(target_os = "macos") {
            assert!(inbox.starts_with("/home/dj/Library/Application Support"));
        } else if cfg!(windows) {
            assert!(inbox.starts_with(r"C:\Users\dj\AppData\Roaming"));
        } else {
            assert!(inbox.starts_with("/home/dj/.local/share"));
        }
    }

    #[test]
    fn a_sent_track_is_one_finished_request_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let track = dir.path().join("My song.wav");
        let request = send_to(&dir.path().join("inbox"), std::slice::from_ref(&track)).unwrap();
        assert_eq!(request.extension().unwrap(), "paths");
        assert_eq!(std::fs::read_to_string(&request).unwrap(), format!("{}\n", track.display()));
        let names: Vec<_> = std::fs::read_dir(dir.path().join("inbox")).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(names, vec![request]);
    }
}
