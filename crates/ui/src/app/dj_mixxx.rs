// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ mode is Mixxx (free DJ software, mixxx.org), shipped inside YOBUKO: where that copy is,
//! which profile it runs with, how it is started, and the library sync done each time it is
//! (`suhina_dj::mixxx`). rbxport stays for USB sticks (`dj_rbxport.rs`).

use std::path::{Path, PathBuf};

use suhina_dj::mixxx;

/// Where to get Mixxx when no copy is found (a Linux system without it, say).
pub(crate) const DOWNLOAD_PAGE: &str = "https://mixxx.org/download/";

/// The Mixxx shipped with YOBUKO, relative to YOBUKO's own program `exe`.
/// macOS: `YOBUKO.app/Contents/MacOS/suhina` → `YOBUKO.app/Contents/Helpers/Mixxx.app`.
/// Windows and Linux: a `mixxx` folder beside the program.
pub(crate) fn bundled(exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new("."));
    if cfg!(target_os = "macos") {
        dir.parent().unwrap_or(dir).join("Helpers").join("Mixxx.app")
    } else if cfg!(windows) {
        dir.join("mixxx").join("mixxx.exe")
    } else {
        dir.join("mixxx").join("mixxx")
    }
}

/// The places Mixxx's own installers and the Linux distributions put it, most likely first.
pub(crate) fn candidates(home: Option<&Path>, env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.push(PathBuf::from("/Applications/Mixxx.app"));
        if let Some(home) = home {
            out.push(home.join("Applications/Mixxx.app"));
        }
    } else if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
            if let Some(base) = env(var) {
                out.push(Path::new(&base).join("Mixxx").join("mixxx.exe"));
            }
        }
    } else {
        if let Some(path) = env("PATH") {
            out.extend(std::env::split_paths(&path).map(|dir| dir.join("mixxx")));
        }
        out.push(PathBuf::from("/usr/bin/mixxx"));
        out.push(PathBuf::from("/usr/local/bin/mixxx"));
    }
    out
}

/// A Mixxx YOBUKO can start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mixxx {
    pub path: PathBuf,
    /// The copy shipped inside YOBUKO, which runs with YOBUKO's own profile.
    pub shipped: bool,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// The Mixxx to start: the one shipped with YOBUKO first, else one installed on its own.
/// Tests never find one, so they never start it nor touch a real library.
pub(crate) fn find() -> Option<Mixxx> {
    if cfg!(test) {
        return None;
    }
    // The Mixxx shipped for macOS is its Apple Silicon build: an Intel Mac (the Intel half of
    // YOBUKO's universal program) looks for one installed on its own instead.
    let usable = !cfg!(all(target_os = "macos", target_arch = "x86_64"));
    let shipped = std::env::current_exe().ok().map(|exe| bundled(&exe)).filter(|p| usable && p.exists());
    if let Some(path) = shipped {
        return Some(Mixxx { path, shipped: true });
    }
    candidates(home().as_deref(), &|name| std::env::var(name).ok())
        .into_iter()
        .find(|p| p.is_file() || p.is_dir())
        .map(|path| Mixxx { path, shipped: false })
}

/// Its version: the `VERSION` note YOBUKO's packages write beside the Mixxx they ship, or a
/// macOS app's `Info.plist`.
pub(crate) fn version(mixxx: &Mixxx) -> Option<(u32, u32)> {
    if mixxx.path.extension().is_some_and(|e| e == "app") {
        let plist = std::fs::read_to_string(mixxx.path.join("Contents").join("Info.plist")).unwrap_or_default();
        return mixxx::plist_version(&plist).or_else(|| {
            let note = std::fs::read_to_string(mixxx.path.with_file_name("Mixxx.version")).ok()?;
            mixxx::parse_version(&note)
        });
    }
    let note = std::fs::read_to_string(mixxx.path.parent()?.join("VERSION")).ok()?;
    mixxx::parse_version(&note)
}

/// The profile it runs with, and whether it must be told so (`--settings-path`): YOBUKO's own
/// for the shipped copy; for one installed on its own, that Mixxx's usual profile, so the
/// DJ's library is the one they already have.
pub(crate) fn profile(
    mixxx: &Mixxx,
    data: &Path,
    home: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<(PathBuf, bool)> {
    if mixxx.shipped {
        return Some((mixxx::profile(data), true));
    }
    let usual = if cfg!(target_os = "macos") {
        mixxx::sandboxed_profile(home?)
    } else if cfg!(windows) {
        Path::new(&env("LOCALAPPDATA")?).join("Mixxx")
    } else {
        home?.join(".mixxx")
    };
    Some((usual, false))
}

/// Whether a Mixxx is running: its library must not be written under it, and a second one
/// must not be started on the same library.
pub(crate) fn is_running() -> bool {
    if cfg!(test) {
        return false;
    }
    let output = if cfg!(windows) {
        std::process::Command::new("tasklist").args(["/FI", "IMAGENAME eq mixxx.exe", "/NH"]).output()
    } else {
        std::process::Command::new("pgrep").args(["-x", "-i", "mixxx"]).output()
    };
    match output {
        Ok(out) if cfg!(windows) => String::from_utf8_lossy(&out.stdout).to_lowercase().contains("mixxx.exe"),
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Starts Mixxx, detached; YOBUKO keeps running.
pub(crate) fn launch(mixxx: &Mixxx, settings_path: Option<&Path>) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&mixxx.path);
        if let Some(profile) = settings_path {
            c.arg("--args").arg("--settings-path").arg(profile);
        }
        c
    } else {
        let mut c = std::process::Command::new(&mixxx.path);
        if let Some(profile) = settings_path {
            c.arg("--settings-path").arg(profile);
        }
        if let Some(dir) = mixxx.path.parent() {
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

/// What one library sync did, said on the DJ page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Synced {
    /// Tracks and playlists brought from rbxport into Mixxx.
    pub into_mixxx: Option<mixxx::Imported>,
    /// Tracks and playlists asked of rbxport.
    pub to_rbxport: (usize, usize),
    pub problems: Vec<String>,
}

impl Synced {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(done) = self.into_mixxx
            && (done.tracks > 0 || done.playlists > 0)
        {
            parts.push(format!("{} tracks and {} playlists from rbxport in Mixxx", done.tracks, done.playlists));
        }
        if self.to_rbxport != (0, 0) {
            parts.push(format!(
                "{} tracks and {} playlists from Mixxx on their way to rbxport",
                self.to_rbxport.0, self.to_rbxport.1
            ));
        }
        if parts.is_empty() {
            "Your music library is the same in Mixxx and rbxport.".into()
        } else {
            format!("Library shared: {}.", parts.join("; "))
        }
    }
}

/// Shares the library between Mixxx (its database in `profile`) and rbxport (what it
/// published under `data`, and its `inbox`). rbxport → Mixxx only while `mixxx_closed`.
pub(crate) fn sync(profile: &Path, data: &Path, inbox: &Path, mixxx_closed: bool) -> Synced {
    let db = profile.join(mixxx::DATABASE);
    let mirror = mixxx::read_mirror(&mixxx::mirror_path(data));
    let mut out = Synced::default();
    if !db.is_file() {
        // Mixxx has not been opened yet: it makes its library on first start.
        return out;
    }
    if mixxx_closed && let Some(mirror) = &mirror {
        match mixxx::import_into_mixxx(&db, mirror) {
            Ok(done) => out.into_mixxx = Some(done),
            Err(e) => out.problems.push(e),
        }
    }
    match mixxx::read_mixxx(&db) {
        Ok(library) => {
            let asks = mixxx::to_rbxport(&library, mirror.as_ref());
            if !asks.is_empty() {
                match mixxx::write_requests(inbox, &asks) {
                    Ok(_) => out.to_rbxport = (asks.tracks.len(), asks.playlists.len()),
                    Err(e) => out.problems.push(format!("Could not reach rbxport's inbox: {e}")),
                }
            }
        }
        Err(e) => out.problems.push(e),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_mixxx_sits_where_the_packages_put_it() {
        if cfg!(target_os = "macos") {
            let exe = Path::new("/Applications/YOBUKO.app/Contents/MacOS/suhina");
            assert_eq!(bundled(exe), Path::new("/Applications/YOBUKO.app/Contents/Helpers/Mixxx.app"));
        } else if cfg!(windows) {
            let exe = Path::new(r"C:\Program Files\YOBUKO\suhina.exe");
            assert_eq!(bundled(exe), Path::new(r"C:\Program Files\YOBUKO\mixxx\mixxx.exe"));
        } else {
            assert_eq!(bundled(Path::new("/opt/yobuko/suhina")), Path::new("/opt/yobuko/mixxx/mixxx"));
        }
    }

    #[test]
    fn every_system_looks_where_mixxx_is_installed() {
        let env = |name: &str| match name {
            "ProgramFiles" => Some(r"C:\Program Files".to_owned()),
            "PATH" => Some("/usr/local/bin".to_owned()),
            _ => None,
        };
        let found = candidates(Some(Path::new("/home/dj")), &env);
        if cfg!(target_os = "macos") {
            assert_eq!(found[0], PathBuf::from("/Applications/Mixxx.app"));
        } else if cfg!(windows) {
            assert_eq!(found[0], Path::new(r"C:\Program Files").join("Mixxx").join("mixxx.exe"));
        } else {
            assert_eq!(found[0], PathBuf::from("/usr/local/bin/mixxx"));
            assert!(found.contains(&PathBuf::from("/usr/bin/mixxx")));
        }
        assert!(DOWNLOAD_PAGE.starts_with("https://mixxx.org"));
    }

    #[test]
    fn the_shipped_mixxx_uses_yobukos_profile_and_another_keeps_its_own() {
        let data = Path::new("/data");
        let home = Path::new("/home/dj");
        let env = |name: &str| (name == "LOCALAPPDATA").then(|| r"C:\Users\dj\AppData\Local".to_owned());
        let shipped = Mixxx { path: PathBuf::from("/x/mixxx"), shipped: true };
        assert_eq!(profile(&shipped, data, Some(home), &env), Some((mixxx::profile(data), true)));
        let own = Mixxx { path: PathBuf::from("/usr/bin/mixxx"), shipped: false };
        let (path, told) = profile(&own, data, Some(home), &env).unwrap();
        assert!(!told);
        if cfg!(target_os = "macos") {
            assert!(path.ends_with("Containers/org.mixxx.mixxx/Data/Library/Application Support/Mixxx"));
        } else if cfg!(windows) {
            assert!(path.ends_with("Mixxx"));
        } else {
            assert_eq!(path, home.join(".mixxx"));
        }
    }

    #[test]
    fn a_shipped_version_note_picks_the_flx4_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("mixxx");
        std::fs::write(&exe, b"").unwrap();
        let shipped = Mixxx { path: exe, shipped: true };
        assert_eq!(version(&shipped), None);
        std::fs::write(dir.path().join("VERSION"), "2.6.0\n").unwrap();
        assert_eq!(version(&shipped), Some((2, 6)));
        assert_eq!(mixxx::Mapping::for_version(version(&shipped)), mixxx::Mapping::Community);
    }

    #[test]
    fn before_mixxx_first_runs_nothing_is_synced() {
        let dir = tempfile::tempdir().unwrap();
        let synced = sync(&dir.path().join("Mixxx"), dir.path(), &dir.path().join("inbox"), true);
        assert_eq!(synced, Synced::default());
        assert_eq!(synced.summary(), "Your music library is the same in Mixxx and rbxport.");
    }
}
