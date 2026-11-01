//! YOBUKO patch (not in upstream rbxport): what this copy of rbxport does
//! differently because it ships inside YOBUKO, the music app it is DJ mode of.
//!
//! - Tracks YOBUKO renders arrive through an inbox folder both apps know:
//!   YOBUKO drops a `.paths` file (one audio file path per line) there, and
//!   rbxport imports those files into its library and deletes the request.
//!   A folder rather than a command line, because rbxport is usually already
//!   running when a track is sent, and a second launch only brings it forward.
//! - Updates come with YOBUKO's own releases, so the built-in updater, which
//!   would replace this copy with the upstream one, stays off.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::Manager;

/// Every copy of rbxport built from this tree ships inside YOBUKO.
pub const BUNDLED: bool = true;

/// How often the inbox is looked at.
const INTERVAL: Duration = Duration::from_secs(2);

/// The extension of a finished request; YOBUKO writes `.tmp` and renames.
const REQUEST: &str = "paths";

/// The folder YOBUKO drops requests in. Keep in step with YOBUKO's
/// `crates/ui/src/app/dj_rbxport.rs` (`inbox`).
pub fn inbox() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("YOBUKO").join("rbxport-inbox"))
}

/// The finished requests in `dir`, oldest name first.
pub fn requests(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == REQUEST))
        .collect();
    out.sort();
    out
}

/// The audio files a request names: one path per line, blank lines and
/// files that are gone left out.
pub fn paths(request: &str) -> Vec<String> {
    request
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && Path::new(line).exists())
        .map(str::to_owned)
        .collect()
}

/// Watches the inbox for as long as the app runs.
pub fn start(app: tauri::AppHandle) {
    let Some(dir) = inbox() else { return };
    let _ = std::thread::Builder::new().name("yobuko-inbox".into()).spawn(move || {
        let mut refused: Option<PathBuf> = None;
        loop {
            std::thread::sleep(INTERVAL);
            for request in requests(&dir) {
                let wanted = std::fs::read_to_string(&request).map(|text| paths(&text)).unwrap_or_default();
                if wanted.is_empty() {
                    let _ = std::fs::remove_file(&request);
                    continue;
                }
                let imported = tauri::async_runtime::block_on(crate::commands::import_files_then(
                    app.clone(),
                    app.state(),
                    wanted,
                    true,
                ));
                match imported {
                    Ok(report) => {
                        tracing::info!(imported = report.imported, request = %request.display(), "tracks from YOBUKO");
                        let _ = std::fs::remove_file(&request);
                        if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
                            let _ = main.set_focus();
                        }
                    }
                    // Most likely no library is open yet, or it is read-only:
                    // the request waits for the next look. Said once per file.
                    Err(e) => {
                        if refused.as_ref() != Some(&request) {
                            tracing::warn!(error = ?e, request = %request.display(), "tracks from YOBUKO not imported yet");
                            refused = Some(request.clone());
                        }
                        break;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_finished_requests_are_read_and_only_files_that_exist_are_imported() {
        let dir = tempfile::tempdir().unwrap();
        let track = dir.path().join("mix.wav");
        std::fs::write(&track, b"RIFF").unwrap();
        std::fs::write(dir.path().join("b.paths"), "").unwrap();
        std::fs::write(dir.path().join("a.paths"), "").unwrap();
        std::fs::write(dir.path().join("c.tmp"), "").unwrap();
        let found = requests(dir.path());
        assert_eq!(found, vec![dir.path().join("a.paths"), dir.path().join("b.paths")]);
        let request = format!("{}\n\n{}\n", track.display(), dir.path().join("gone.wav").display());
        assert_eq!(paths(&request), vec![track.display().to_string()]);
    }
}
