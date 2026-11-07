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

/// The arguments Mixxx is started with: full screen, so DJ mode fills the screen in YOBUKO's
/// look instead of opening one more window, and the YOBUKO profile when there is one.
pub(crate) fn arguments(settings_path: Option<&Path>) -> Vec<std::ffi::OsString> {
    let mut args = vec![std::ffi::OsString::from("--full-screen")];
    if let Some(profile) = settings_path {
        args.push("--settings-path".into());
        args.push(profile.as_os_str().to_owned());
    }
    args
}

/// Starts Mixxx full screen, detached; YOBUKO keeps running behind it.
pub(crate) fn launch(mixxx: &Mixxx, settings_path: Option<&Path>) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&mixxx.path).arg("--args").args(arguments(settings_path));
        c
    } else {
        let mut c = std::process::Command::new(&mixxx.path);
        c.args(arguments(settings_path));
        if let Some(dir) = mixxx.path.parent() {
            c.current_dir(dir);
        }
        c
    };
    quiet(&mut command).spawn().map(drop)
}

/// Brings the Mixxx that is already running in front of everything, without starting another.
/// Best effort: on Linux it needs `wmctrl`, which not every desktop has.
pub(crate) fn bring_forward(mixxx: &Mixxx) {
    if cfg!(test) {
        return;
    }
    let mut command = if cfg!(target_os = "macos") {
        // `open` on an app that runs only brings it forward.
        let mut c = std::process::Command::new("open");
        c.arg(&mixxx.path);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("powershell");
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$p = Get-Process mixxx -ErrorAction SilentlyContinue | Select-Object -First 1; \
             if ($p) { (New-Object -ComObject WScript.Shell).AppActivate($p.Id) | Out-Null }",
        ]);
        c
    } else {
        let mut c = std::process::Command::new("wmctrl");
        c.args(["-x", "-a", "mixxx"]);
        c
    };
    let _ = quiet(&mut command).spawn();
}

fn quiet(command: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up beside Mixxx.
        command.creation_flags(0x0800_0000);
    }
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
}

/// Watches the Mixxx YOBUKO started: once it has been seen running and then closes, `closed`
/// says so and YOBUKO's window is asked to repaint, so it can come back to the studio.
pub(crate) struct Watch {
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Watch {
    /// How long Mixxx may take to appear (a first start scans the library) before the watch
    /// gives up, and how often it looks.
    const APPEAR: std::time::Duration = std::time::Duration::from_secs(90);
    const EVERY: std::time::Duration = std::time::Duration::from_millis(if cfg!(test) { 5 } else { 1500 });

    pub(crate) fn start(ctx: egui::Context) -> Self {
        Self::start_with(ctx, is_running)
    }

    pub(crate) fn start_with(ctx: egui::Context, running: fn() -> bool) -> Self {
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = closed.clone();
        let spawned = std::thread::Builder::new().name("mixxx-watch".into()).spawn(move || {
            let since = std::time::Instant::now();
            while !running() {
                if since.elapsed() > Self::APPEAR {
                    return;
                }
                std::thread::sleep(Self::EVERY);
            }
            while running() {
                std::thread::sleep(Self::EVERY);
            }
            flag.store(true, std::sync::atomic::Ordering::Release);
            ctx.request_repaint();
        });
        if spawned.is_err() {
            eprintln!("YOBUKO: could not watch Mixxx, so the studio will not come back by itself when it closes");
        }
        Self { closed }
    }

    pub(crate) fn closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }
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
    fn mixxx_starts_full_screen_with_yobukos_profile() {
        assert_eq!(arguments(None), vec![std::ffi::OsString::from("--full-screen")]);
        let args = arguments(Some(Path::new("/data/YOBUKO/Mixxx")));
        assert_eq!(args, ["--full-screen", "--settings-path", "/data/YOBUKO/Mixxx"].map(std::ffi::OsString::from));
    }

    #[test]
    fn the_watch_tells_once_mixxx_has_opened_and_closed() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // Not yet running, running for a while, then closed.
        static LOOKS: AtomicUsize = AtomicUsize::new(0);
        fn running() -> bool {
            matches!(LOOKS.fetch_add(1, Ordering::SeqCst), 2..=5)
        }
        let watch = Watch::start_with(egui::Context::default(), running);
        let since = std::time::Instant::now();
        while !watch.closed() {
            assert!(since.elapsed() < std::time::Duration::from_secs(10), "the watch never saw Mixxx close");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(LOOKS.load(Ordering::SeqCst) >= 7);
    }

    #[test]
    fn a_mixxx_that_never_runs_is_not_taken_for_closed() {
        fn never() -> bool {
            false
        }
        let watch = Watch::start_with(egui::Context::default(), never);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!watch.closed());
    }

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
