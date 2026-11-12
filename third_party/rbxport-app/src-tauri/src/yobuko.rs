//! YOBUKO patch (not in upstream rbxport): what this copy of rbxport does
//! differently because it ships inside YOBUKO, the music app it is the USB
//! export side of. YOBUKO mixes with Mixxx; rbxport prepares the sticks for
//! CDJ / XDJ players, and both share one music library through YOBUKO.
//!
//! - Tracks arrive through an inbox folder both apps know: YOBUKO drops a
//!   `.paths` file (one audio file path per line) there, and rbxport imports
//!   those files into its library and deletes the request. A folder rather
//!   than a command line, because rbxport is usually already running when a
//!   track is sent, and a second launch only brings it forward.
//! - Playlists made in Mixxx arrive the same way, as a `.playlist` file (JSON:
//!   the folder to put it in, its name, its tracks). A playlist of that name
//!   already in that folder is replaced, so sending one again updates it.
//! - What this library holds is published for YOBUKO to give to Mixxx: every
//!   track with its tags and hot cues, and every playlist, in
//!   `<data dir>/YOBUKO/library/rbxport.json` (see [`Mirror`]). Rewritten
//!   whenever the library changes.
//! - It looks like the rest of YOBUKO: YOBUKO's studio writes the colours of
//!   the theme in use to `<data dir>/YOBUKO/look.json`, and the window takes
//!   them (`src/lib/yobukoLook.ts`, through [`yobuko_look`]).
//! - Updates come with YOBUKO's own releases, so the built-in updater, which
//!   would replace this copy with the upstream one, stays off.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::state::AppState;

/// Every copy of rbxport built from this tree ships inside YOBUKO.
pub const BUNDLED: bool = true;

/// How often the inbox is looked at.
const INTERVAL: Duration = Duration::from_secs(2);

/// How often the library is looked at for changes to publish.
const PUBLISH_INTERVAL: Duration = Duration::from_secs(5);

/// The extension of a finished track request; YOBUKO writes `.tmp` and renames.
const REQUEST: &str = "paths";

/// The extension of a finished playlist request.
const PLAYLIST: &str = "playlist";

/// A request whose name starts so is YOBUKO keeping the libraries in step
/// (`sync-`) or adding a track it rendered (`render-`) while DJ mode is in
/// Mixxx: it is imported without bringing rbxport forward.
const QUIET: [&str; 2] = ["sync-", "render-"];

/// The folder YOBUKO drops requests in. Keep in step with YOBUKO's
/// `crates/ui/src/app/dj_rbxport.rs` (`inbox`).
pub fn inbox() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("YOBUKO").join("rbxport-inbox"))
}

/// Where this library is published for YOBUKO. Keep in step with YOBUKO's
/// `crates/dj/src/mixxx.rs` (`mirror_path`).
pub fn mirror_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("YOBUKO").join("library").join("rbxport.json"))
}

/// Where YOBUKO's studio writes the colours of its theme. Keep in step with
/// YOBUKO's `crates/dj/src/look.rs` (`path`).
pub fn look_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("YOBUKO").join("look.json"))
}

/// The look YOBUKO's studio wrote, as its JSON text, or `None` when there is
/// none yet. The window reads it at start and every few seconds after.
#[tauri::command]
pub fn yobuko_look() -> Option<String> {
    std::fs::read_to_string(look_path()?).ok().filter(|text| text.len() < 64 * 1024)
}

/// The finished requests in `dir`, oldest name first.
pub fn requests(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == REQUEST || ext == PLAYLIST))
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

/// A playlist YOBUKO asks for: `name` inside the root folder `folder`,
/// holding `tracks` in order. Keep in step with YOBUKO's
/// `crates/dj/src/mixxx.rs` (`PlaylistRequest`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistRequest {
    pub folder: String,
    pub name: String,
    pub tracks: Vec<String>,
}

/// The library as YOBUKO reads it. Keep in step with YOBUKO's
/// `crates/dj/src/mixxx.rs` (`Mirror`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mirror {
    pub format: u32,
    pub tracks: Vec<MirrorTrack>,
    pub playlists: Vec<MirrorPlaylist>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub key: String,
    /// Beats per minute, 0 when not analysed.
    pub bpm: f64,
    pub seconds: u32,
    pub sample_rate: u32,
    /// 0 to 5 stars.
    pub rating: u8,
    pub hot_cues: Vec<MirrorCue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorCue {
    /// 0 for hot cue A, 1 for B, and so on.
    pub slot: u8,
    pub ms: u32,
    /// Where a hot loop ends, or 0 for a plain hot cue.
    pub loop_end_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorPlaylist {
    /// The folders it is in, outermost first.
    pub folders: Vec<String>,
    pub name: String,
    /// Indices into [`Mirror::tracks`].
    pub tracks: Vec<u32>,
}

/// What YOBUKO's playlists from Mixxx go in. Published like every other
/// playlist, so YOBUKO can tell what is already here; YOBUKO itself leaves
/// them out of Mixxx, so a playlist does not go round in circles.
pub const FROM_MIXXX: &str = "Mixxx";

/// The library, for YOBUKO: tracks, hot cues and playlists (not folders,
/// not intelligent playlists).
pub fn mirror(library: &rbl_index::Library) -> Mirror {
    let count = library.len();
    let cues = library.cues();
    let mut tracks = Vec::with_capacity(count);
    for row in 0..count {
        let r = u32::try_from(row).unwrap_or(u32::MAX);
        let hot_cues = cues
            .of(r)
            .iter()
            .filter_map(|cue| {
                let letter = cue.hot_letter()?;
                let slot = u8::try_from(u32::from(letter).checked_sub(u32::from('A'))?).ok()?;
                Some(MirrorCue { slot, ms: cue.position_ms, loop_end_ms: cue.out_ms })
            })
            .collect();
        tracks.push(MirrorTrack {
            path: library.folder_path.get(row).to_owned(),
            title: library.title.get(row).to_owned(),
            artist: library.artist_name(r).to_owned(),
            album: library.album_name(r).to_owned(),
            genre: library.genre_name(r).to_owned(),
            key: library.key_name(r).to_owned(),
            bpm: f64::from(library.bpm_x100.get(row).copied().unwrap_or(0)) / 100.0,
            seconds: library.length_sec.get(row).copied().unwrap_or(0),
            sample_rate: library.sample_rate.get(row).copied().unwrap_or(0),
            rating: library.rating.get(row).copied().unwrap_or(0).min(5),
            hot_cues,
        });
    }
    drop(cues);

    let lists = library.playlists();
    let folders_of = |index: usize| -> Vec<String> {
        let mut out = Vec::new();
        let mut parent = lists.parent.get(index).copied().unwrap_or(rbl_index::NO_ID);
        let mut hops = 0;
        while parent != rbl_index::NO_ID && hops < 64 {
            let p = parent as usize;
            out.push(lists.name(p).to_owned());
            parent = lists.parent.get(p).copied().unwrap_or(rbl_index::NO_ID);
            hops += 1;
        }
        out.reverse();
        out
    };
    let mut playlists = Vec::new();
    for index in 0..lists.len() {
        if lists.is_folder(index) || lists.is_smart(index) {
            continue;
        }
        let folders = folders_of(index);
        let members = lists.members.get(index).cloned().unwrap_or_default();
        playlists.push(MirrorPlaylist { folders, name: lists.name(index).to_owned(), tracks: members });
    }
    Mirror { format: 1, tracks, playlists }
}

/// Writes `mirror` to `path` whole: a reader never sees half a file.
pub fn publish(path: &Path, mirror: &Mirror) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("json.tmp");
    std::fs::write(&partial, serde_json::to_vec(mirror).map_err(std::io::Error::other)?)?;
    std::fs::rename(&partial, path)
}

/// What changes when the library does: the snapshot, the cues, the playlists.
fn signature(state: &AppState) -> Option<(usize, usize, usize, usize)> {
    let library = state.library().ok()?;
    let cues = library.cues().len();
    let lists = library.playlists();
    let members = lists.members.iter().map(Vec::len).sum::<usize>();
    Some((Arc::as_ptr(&library) as usize, cues, lists.len(), members))
}

/// Imports one playlist request: into the root folder it names (made when
/// missing), replacing a playlist of the same name there.
fn import_playlist<R: tauri::Runtime>(app: &tauri::AppHandle<R>, request: &PlaylistRequest) -> Result<(), String> {
    let state = Arc::clone(app.state::<Arc<AppState>>().inner());
    let library = state.library().map_err(|e| format!("{e:?}"))?;
    let folder = {
        let lists = library.playlists();
        (0..lists.len())
            .find(|&i| {
                lists.is_folder(i)
                    && lists.parent.get(i).copied() == Some(rbl_index::NO_ID)
                    && lists.name(i) == request.folder
            })
            .map(|i| lists.ids[i].to_string())
    };
    let files: Vec<PathBuf> =
        request.tracks.iter().map(PathBuf::from).filter(|path| path.exists()).collect();
    if files.is_empty() {
        return Ok(());
    }
    {
        let _gate = state.edit_gate.lock();
        state
            .write(|writer| {
                let parent = match &folder {
                    Some(id) => id.clone(),
                    None => writer.create_folder(&request.folder, rbl_db::write::ROOT)?,
                };
                let first = writer.import_folder_as_playlist(
                    &request.name,
                    &parent,
                    &files,
                    rbl_db::write::FolderClash::Ask,
                    None,
                )?;
                if let Some(existing) = first.conflict {
                    writer.import_folder_as_playlist(
                        &request.name,
                        &parent,
                        &files,
                        rbl_db::write::FolderClash::Replace(&existing),
                        None,
                    )?;
                }
                Ok(())
            })
            .map_err(|e| format!("{e:?}"))?;
        state.edit_history.lock().clear_redo();
    }
    tauri::async_runtime::block_on(crate::commands::reload(app.clone(), state)).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// Watches the inbox, and publishes the library, for as long as the app runs.
pub fn start(app: tauri::AppHandle) {
    if let Some(out) = mirror_path() {
        let publisher = app.clone();
        let _ = std::thread::Builder::new().name("yobuko-library".into()).spawn(move || {
            let mut last = None;
            loop {
                std::thread::sleep(PUBLISH_INTERVAL);
                let state = publisher.state::<Arc<AppState>>();
                let now = signature(&state);
                if now.is_none() || now == last {
                    continue;
                }
                let Ok(library) = state.library() else { continue };
                match publish(&out, &mirror(&library)) {
                    Ok(()) => last = now,
                    Err(e) => tracing::warn!(error = ?e, "library not published for YOBUKO"),
                }
            }
        });
    }
    let Some(dir) = inbox() else { return };
    let _ = std::thread::Builder::new().name("yobuko-inbox".into()).spawn(move || {
        let mut refused: Option<PathBuf> = None;
        loop {
            std::thread::sleep(INTERVAL);
            for request in requests(&dir) {
                let quiet =
                    request.file_name().is_some_and(|name| QUIET.iter().any(|q| name.to_string_lossy().starts_with(q)));
                if request.extension().is_some_and(|ext| ext == PLAYLIST) {
                    let parsed = std::fs::read(&request)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<PlaylistRequest>(&bytes).ok());
                    let Some(playlist) = parsed else {
                        let _ = std::fs::remove_file(&request);
                        continue;
                    };
                    match import_playlist(&app, &playlist) {
                        Ok(()) => {
                            tracing::info!(name = %playlist.name, "playlist from YOBUKO");
                            let _ = std::fs::remove_file(&request);
                        }
                        Err(e) => {
                            if refused.as_ref() != Some(&request) {
                                tracing::warn!(error = %e, request = %request.display(), "playlist from YOBUKO not imported yet");
                                refused = Some(request.clone());
                            }
                            break;
                        }
                    }
                    continue;
                }
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
                        if !quiet && !crate::yobuko_embed::embedded() {
                            if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
                                let _ = main.set_focus();
                            }
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
        std::fs::write(dir.path().join("d.playlist"), "").unwrap();
        let found = requests(dir.path());
        assert_eq!(
            found,
            vec![dir.path().join("a.paths"), dir.path().join("b.paths"), dir.path().join("d.playlist")]
        );
        let request = format!("{}\n\n{}\n", track.display(), dir.path().join("gone.wav").display());
        assert_eq!(paths(&request), vec![track.display().to_string()]);
    }

    #[test]
    fn a_playlist_request_reads_as_yobuko_writes_it() {
        let text = r#"{"folder":"Mixxx","name":"Friday","tracks":["/m/a.mp3","/m/b.mp3"]}"#;
        let request: PlaylistRequest = serde_json::from_str(text).unwrap();
        assert_eq!(request.folder, FROM_MIXXX);
        assert_eq!(request.name, "Friday");
        assert_eq!(request.tracks.len(), 2);
    }

    #[test]
    fn the_published_library_is_written_whole() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("library").join("rbxport.json");
        let mirror = Mirror {
            format: 1,
            tracks: vec![MirrorTrack {
                path: "/m/a.mp3".into(),
                title: "A".into(),
                artist: String::new(),
                album: String::new(),
                genre: String::new(),
                key: "8A".into(),
                bpm: 128.0,
                seconds: 300,
                sample_rate: 44100,
                rating: 3,
                hot_cues: vec![MirrorCue { slot: 0, ms: 1500, loop_end_ms: 0 }],
            }],
            playlists: vec![MirrorPlaylist { folders: vec!["Club".into()], name: "Peak".into(), tracks: vec![0] }],
        };
        publish(&out, &mirror).unwrap();
        let back: Mirror = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(back, mirror);
        assert!(!out.with_extension("json.tmp").exists());
    }
}
