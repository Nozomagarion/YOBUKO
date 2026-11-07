// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mixxx, YOBUKO's DJ mode, and the one music library it shares with rbxport.
//!
//! YOBUKO starts Mixxx (free DJ software, GPL-2.0-or-later, mixxx.org) to mix live with a
//! controller such as the Pioneer DDJ-FLX4, and rbxport to prepare USB sticks for CDJ / XDJ
//! players. Neither app knows the other, so YOBUKO carries the library between them each
//! time one of them is opened:
//!
//! - **rbxport → Mixxx.** rbxport (YOBUKO's patched copy) publishes its library as JSON,
//!   [`Mirror`], at [`mirror_path`]. [`import_into_mixxx`] adds the tracks Mixxx does not
//!   have yet, with their hot cues, and keeps one Mixxx playlist per rbxport playlist,
//!   named [`RBXPORT_PREFIX`]` Folder / Name`. Only while Mixxx is closed: Mixxx keeps its
//!   library in memory and would not see, or would overwrite, a change made under it.
//! - **Mixxx → rbxport.** [`read_mixxx`] reads Mixxx's tracks, playlists and crates, and
//!   [`to_rbxport`] turns what rbxport lacks into requests for its inbox: loose tracks as a
//!   `.paths` file, and each playlist or crate as a `.playlist` file that rbxport files in
//!   its [`FROM_MIXXX`] folder. A playlist already there with the same tracks is not sent.
//!
//! What came from one side is never sent back to it, so nothing goes round in circles.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Mixxx's library database, in its profile folder.
pub const DATABASE: &str = "mixxxdb.sqlite";
/// Mixxx's settings file, in its profile folder.
pub const CONFIG: &str = "mixxx.cfg";
/// The start of the name of every Mixxx playlist that mirrors an rbxport one.
pub const RBXPORT_PREFIX: &str = "rbxport · ";
/// The rbxport folder that holds what came from Mixxx.
pub const FROM_MIXXX: &str = "Mixxx";
/// What a Mixxx crate is called once it is an rbxport playlist.
pub const CRATE_PREFIX: &str = "Crate · ";
/// The start of the name of the requests [`write_requests`] makes, which rbxport imports
/// without coming forward. Keep in step with `third_party/rbxport-app/src-tauri/src/yobuko.rs`.
pub const QUIET: &str = "sync-";
/// The most tracks one sync adds to Mixxx, so a huge first sync cannot stall YOBUKO.
pub const MAX_IMPORT: usize = 200_000;

/// YOBUKO's own folder inside the system's per-user data folder.
pub fn yobuko_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("YOBUKO")
}

/// Where rbxport publishes its library. Keep in step with `yobuko.rs` (`mirror_path`).
pub fn mirror_path(data_dir: &Path) -> PathBuf {
    yobuko_dir(data_dir).join("library").join("rbxport.json")
}

/// The Mixxx profile YOBUKO starts its own Mixxx with (`--settings-path`), so its settings,
/// controller mapping and library are YOBUKO's and never touch another Mixxx's.
pub fn profile(data_dir: &Path) -> PathBuf {
    yobuko_dir(data_dir).join("Mixxx")
}

/// The profile of the Mixxx from mixxx.org installed on its own on macOS: sandboxed, so it
/// can use no other folder than this one.
pub fn sandboxed_profile(home: &Path) -> PathBuf {
    home.join("Library/Containers/org.mixxx.mixxx/Data/Library/Application Support/Mixxx")
}

// ---------------------------------------------------------------------------------------------
// rbxport's published library
// ---------------------------------------------------------------------------------------------

/// rbxport's library as it publishes it. Keep in step with `yobuko.rs` (`Mirror`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Mirror {
    pub format: u32,
    pub tracks: Vec<MirrorTrack>,
    pub playlists: Vec<MirrorPlaylist>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MirrorCue {
    /// 0 for hot cue A, 1 for B, and so on.
    pub slot: u8,
    pub ms: u32,
    /// Where a hot loop ends, or 0 for a plain hot cue.
    pub loop_end_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MirrorPlaylist {
    /// The folders it is in, outermost first.
    pub folders: Vec<String>,
    pub name: String,
    /// Indices into [`Mirror::tracks`].
    pub tracks: Vec<u32>,
}

impl MirrorPlaylist {
    /// Whether it is one of the playlists that came from Mixxx.
    pub fn from_mixxx(&self) -> bool {
        self.folders.first().map(String::as_str) == Some(FROM_MIXXX)
    }
}

/// The library rbxport published last, if it has published one this format can read.
pub fn read_mirror(path: &Path) -> Option<Mirror> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<Mirror>(&bytes).ok().filter(|mirror| mirror.format == 1)
}

/// A path as both apps can compare it: forward slashes, and on the systems whose disks
/// ignore case, lower case.
pub fn same_file_key(path: &str) -> String {
    let path = path.replace('\\', "/");
    if cfg!(any(windows, target_os = "macos")) { path.to_lowercase() } else { path }
}

/// A path the way Mixxx stores it: forward slashes on every system.
fn mixxx_location(path: &str) -> String {
    path.replace('\\', "/")
}

/// The Mixxx playlist that mirrors `playlist`.
pub fn mixxx_playlist_name(playlist: &MirrorPlaylist) -> String {
    let mut parts: Vec<&str> = playlist.folders.iter().map(String::as_str).collect();
    parts.push(&playlist.name);
    format!("{RBXPORT_PREFIX}{}", parts.join(" / "))
}

// ---------------------------------------------------------------------------------------------
// Mixxx's library
// ---------------------------------------------------------------------------------------------

/// What Mixxx's library holds, as rbxport needs it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MixxxLibrary {
    pub tracks: Vec<String>,
    pub lists: Vec<MixxxList>,
}

/// A Mixxx playlist or crate, its tracks in order.
#[derive(Debug, Clone, PartialEq)]
pub struct MixxxList {
    pub name: String,
    pub is_crate: bool,
    pub tracks: Vec<String>,
}

fn open(db: &Path, flags: OpenFlags) -> Result<Connection, String> {
    if !db.is_file() {
        return Err(format!("Mixxx has no library yet at {}", db.display()));
    }
    let conn = Connection::open_with_flags(db, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("Could not open Mixxx's library: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
    // As Mixxx itself opens it: its schema declares `library.location` a reference to
    // `track_locations(location)` but stores that row's id, so enforced foreign keys refuse
    // every track. (The SQLCipher build YOBUKO links turns them on by default.)
    conn.pragma_update(None, "foreign_keys", false).map_err(|e| e.to_string())?;
    let ready: bool = conn
        .query_row(
            "SELECT COUNT(*) = 2 FROM sqlite_master WHERE type = 'table' AND name IN ('library', 'track_locations')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if ready { Ok(conn) } else { Err("Mixxx's library is not set up yet: open Mixxx once first".into()) }
}

/// Reads Mixxx's tracks, visible playlists and visible crates. Mixxx's own lists that mirror
/// rbxport, its Auto DJ queue and its history are left out.
pub fn read_mixxx(db: &Path) -> Result<MixxxLibrary, String> {
    let conn = open(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let fail = |e: rusqlite::Error| format!("Could not read Mixxx's library: {e}");
    let mut out = MixxxLibrary::default();
    let mut tracks = conn
        .prepare(
            "SELECT tl.location FROM library l JOIN track_locations tl ON l.location = tl.id
             WHERE COALESCE(l.mixxx_deleted, 0) = 0 AND COALESCE(tl.fs_deleted, 0) = 0
             ORDER BY l.id",
        )
        .map_err(fail)?;
    out.tracks = tracks.query_map([], |row| row.get::<_, String>(0)).map_err(fail)?.filter_map(Result::ok).collect();

    let mut playlists =
        conn.prepare("SELECT id, name FROM Playlists WHERE hidden = 0 ORDER BY position, id").map_err(fail)?;
    let found: Vec<(i64, String)> =
        playlists.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).map_err(fail)?.filter_map(Result::ok).collect();
    let mut members = conn
        .prepare(
            "SELECT tl.location FROM PlaylistTracks pt
             JOIN library l ON pt.track_id = l.id JOIN track_locations tl ON l.location = tl.id
             WHERE pt.playlist_id = ?1 AND COALESCE(l.mixxx_deleted, 0) = 0 ORDER BY pt.position",
        )
        .map_err(fail)?;
    for (id, name) in found {
        if name.starts_with(RBXPORT_PREFIX) {
            continue;
        }
        let tracks =
            members.query_map([id], |row| row.get::<_, String>(0)).map_err(fail)?.filter_map(Result::ok).collect();
        out.lists.push(MixxxList { name, is_crate: false, tracks });
    }

    let has_crates: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'crates'", [], |r| r.get(0))
        .map_err(fail)?;
    if has_crates {
        let mut crates = conn.prepare("SELECT id, name FROM crates WHERE show = 1 ORDER BY name").map_err(fail)?;
        let found: Vec<(i64, String)> =
            crates.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).map_err(fail)?.filter_map(Result::ok).collect();
        let mut members = conn
            .prepare(
                "SELECT tl.location FROM crate_tracks ct
                 JOIN library l ON ct.track_id = l.id JOIN track_locations tl ON l.location = tl.id
                 WHERE ct.crate_id = ?1 AND COALESCE(l.mixxx_deleted, 0) = 0 ORDER BY tl.location",
            )
            .map_err(fail)?;
        for (id, name) in found {
            let tracks =
                members.query_map([id], |row| row.get::<_, String>(0)).map_err(fail)?.filter_map(Result::ok).collect();
            out.lists.push(MixxxList { name, is_crate: true, tracks });
        }
    }
    Ok(out)
}

/// What [`import_into_mixxx`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Imported {
    pub tracks: usize,
    pub cues: usize,
    pub playlists: usize,
}

/// Mixxx's hot cue colours (its default palette), by slot.
const CUE_COLOURS: [i64; 8] = [0xC5_0A08, 0x32_BE44, 0x00_44FF, 0xF8_D200, 0x42_D4F4, 0xAF_00CC, 0xFC_A6D7, 0xF2_F2FF];

/// Mixxx's cue types.
const HOT_CUE: i64 = 1;
const LOOP: i64 = 4;

/// A position in milliseconds as Mixxx stores cues: in samples of both channels.
fn engine_samples(ms: u32, sample_rate: u32) -> f64 {
    f64::from(ms) / 1000.0 * f64::from(sample_rate) * 2.0
}

/// Brings rbxport's library into Mixxx's: the tracks Mixxx lacks (hot cues included), and
/// one playlist per rbxport playlist, refilled every time. A track Mixxx already has keeps
/// its own cues and tags. Tracks whose file is gone are left out. Mixxx must be closed.
pub fn import_into_mixxx(db: &Path, mirror: &Mirror) -> Result<Imported, String> {
    import(db, mirror, true)
}

/// Adds `tracks` to Mixxx's library (a track YOBUKO just rendered, say), leaving its
/// playlists alone. Mixxx must be closed.
pub fn add_to_mixxx(db: &Path, tracks: &[MirrorTrack]) -> Result<Imported, String> {
    let mirror = Mirror { format: 1, tracks: tracks.to_vec(), playlists: Vec::new() };
    import(db, &mirror, false)
}

fn import(db: &Path, mirror: &Mirror, playlists: bool) -> Result<Imported, String> {
    let mut conn = open(db, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let fail = |e: rusqlite::Error| format!("Could not write to Mixxx's library: {e}");
    let tx = conn.transaction().map_err(fail)?;
    let mut done = Imported::default();

    let mut known: HashMap<String, i64> = HashMap::new();
    {
        let mut rows = tx
            .prepare("SELECT l.id, tl.location FROM library l JOIN track_locations tl ON l.location = tl.id")
            .map_err(fail)?;
        for row in rows.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(fail)? {
            let (id, location) = row.map_err(fail)?;
            known.insert(same_file_key(&location), id);
        }
    }

    // Each rbxport track's id in Mixxx, by its index in the mirror.
    let mut ids: Vec<Option<i64>> = Vec::with_capacity(mirror.tracks.len());
    for track in &mirror.tracks {
        let key = same_file_key(&track.path);
        if let Some(id) = known.get(&key) {
            ids.push(Some(*id));
            continue;
        }
        let file = Path::new(&track.path);
        if done.tracks >= MAX_IMPORT || track.path.is_empty() || !file.is_file() {
            ids.push(None);
            continue;
        }
        let location = mixxx_location(&track.path);
        let filename = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let directory = file.parent().map(|d| mixxx_location(&d.to_string_lossy())).unwrap_or_default();
        let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        tx.execute(
            "INSERT OR IGNORE INTO track_locations (location, filename, directory, filesize, fs_deleted, needs_verification)
             VALUES (?1, ?2, ?3, ?4, 0, 1)",
            params![location, filename, directory, i64::try_from(size).unwrap_or(i64::MAX)],
        )
        .map_err(fail)?;
        let location_id: i64 = tx
            .query_row("SELECT id FROM track_locations WHERE location = ?1", [&location], |r| r.get(0))
            .map_err(fail)?;
        let filetype = file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let title = if track.title.is_empty() {
            file.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            track.title.clone()
        };
        tx.execute(
            "INSERT INTO library (artist, title, album, genre, location, duration, bpm, samplerate, channels,
                                  mixxx_deleted, played, timesplayed, rating, key, header_parsed, filetype)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 2, 0, 0, 0, ?9, ?10, 0, ?11)",
            params![
                track.artist,
                title,
                track.album,
                track.genre,
                location_id,
                f64::from(track.seconds),
                track.bpm,
                track.sample_rate,
                i64::from(track.rating.min(5)),
                track.key,
                filetype
            ],
        )
        .map_err(fail)?;
        let id = tx.last_insert_rowid();
        known.insert(key, id);
        ids.push(Some(id));
        done.tracks += 1;
        if track.sample_rate > 0 {
            for cue in &track.hot_cues {
                let (kind, length) = if cue.loop_end_ms > cue.ms {
                    (LOOP, engine_samples(cue.loop_end_ms - cue.ms, track.sample_rate))
                } else {
                    (HOT_CUE, 0.0)
                };
                tx.execute(
                    "INSERT INTO cues (track_id, type, position, length, hotcue, label, color)
                     VALUES (?1, ?2, ?3, ?4, ?5, '', ?6)",
                    params![
                        id,
                        kind,
                        engine_samples(cue.ms, track.sample_rate),
                        length,
                        i64::from(cue.slot),
                        CUE_COLOURS[usize::from(cue.slot) % CUE_COLOURS.len()]
                    ],
                )
                .map_err(fail)?;
                done.cues += 1;
            }
        }
    }

    if !playlists {
        tx.commit().map_err(fail)?;
        return Ok(done);
    }
    // Playlists: one per rbxport playlist, refilled; ones rbxport no longer has go.
    let mut wanted = HashSet::new();
    for playlist in mirror.playlists.iter().filter(|p| !p.from_mixxx()) {
        let name = mixxx_playlist_name(playlist);
        if !wanted.insert(name.clone()) {
            continue;
        }
        let existing: Option<i64> = tx
            .query_row("SELECT id FROM Playlists WHERE name = ?1 AND hidden = 0", [&name], |r| r.get(0))
            .optional()
            .map_err(fail)?;
        let id = match existing {
            Some(id) => {
                tx.execute("DELETE FROM PlaylistTracks WHERE playlist_id = ?1", [id]).map_err(fail)?;
                tx.execute("UPDATE Playlists SET date_modified = datetime('now') WHERE id = ?1", [id]).map_err(fail)?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO Playlists (name, position, hidden, date_created, date_modified)
                     VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM Playlists), 0, datetime('now'), datetime('now'))",
                    [&name],
                )
                .map_err(fail)?;
                tx.last_insert_rowid()
            }
        };
        let mut position = 0_i64;
        for index in &playlist.tracks {
            let Some(Some(track)) = ids.get(*index as usize) else { continue };
            position += 1;
            tx.execute(
                "INSERT INTO PlaylistTracks (playlist_id, track_id, position, pl_datetime_added)
                 VALUES (?1, ?2, ?3, datetime('now'))",
                params![id, track, position],
            )
            .map_err(fail)?;
        }
        done.playlists += 1;
    }
    let stale: Vec<(i64, String)> = {
        let mut rows =
            tx.prepare("SELECT id, name FROM Playlists WHERE hidden = 0 AND substr(name, 1, ?1) = ?2").map_err(fail)?;
        let prefix_len = i64::try_from(RBXPORT_PREFIX.chars().count()).unwrap_or(0);
        rows.query_map(params![prefix_len, RBXPORT_PREFIX], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(fail)?
            .filter_map(Result::ok)
            .collect()
    };
    for (id, name) in stale {
        if !wanted.contains(&name) {
            tx.execute("DELETE FROM PlaylistTracks WHERE playlist_id = ?1", [id]).map_err(fail)?;
            tx.execute("DELETE FROM Playlists WHERE id = ?1", [id]).map_err(fail)?;
        }
    }
    tx.commit().map_err(fail)?;
    Ok(done)
}

// ---------------------------------------------------------------------------------------------
// Requests for rbxport
// ---------------------------------------------------------------------------------------------

/// A playlist for rbxport to file in `folder`. Keep in step with `yobuko.rs` (`PlaylistRequest`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistRequest {
    pub folder: String,
    pub name: String,
    pub tracks: Vec<String>,
}

/// What rbxport is to be asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToRbxport {
    pub tracks: Vec<String>,
    pub playlists: Vec<PlaylistRequest>,
}

impl ToRbxport {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.playlists.is_empty()
    }
}

/// What of Mixxx's library rbxport does not have yet, by what it published last (`None`:
/// it has published nothing yet, so everything). Playlists that came from rbxport stay out.
pub fn to_rbxport(mixxx: &MixxxLibrary, mirror: Option<&Mirror>) -> ToRbxport {
    let known: HashSet<String> =
        mirror.map(|m| m.tracks.iter().map(|t| same_file_key(&t.path)).collect()).unwrap_or_default();
    let mut out = ToRbxport::default();
    let mut seen = HashSet::new();
    for track in &mixxx.tracks {
        let key = same_file_key(track);
        if !known.contains(&key) && seen.insert(key) {
            out.tracks.push(track.clone());
        }
    }
    let theirs: HashMap<&str, Vec<String>> = mirror
        .map(|m| {
            m.playlists
                .iter()
                .filter(|p| p.from_mixxx() && p.folders.len() == 1)
                .map(|p| {
                    let paths = p
                        .tracks
                        .iter()
                        .filter_map(|i| m.tracks.get(*i as usize))
                        .map(|t| same_file_key(&t.path))
                        .collect();
                    (p.name.as_str(), paths)
                })
                .collect()
        })
        .unwrap_or_default();
    for list in &mixxx.lists {
        if list.tracks.is_empty() {
            continue;
        }
        let name = if list.is_crate { format!("{CRATE_PREFIX}{}", list.name) } else { list.name.clone() };
        let ours: Vec<String> = list.tracks.iter().map(|t| same_file_key(t)).collect();
        if theirs.get(name.as_str()) == Some(&ours) {
            continue;
        }
        out.playlists.push(PlaylistRequest { folder: FROM_MIXXX.into(), name, tracks: list.tracks.clone() });
    }
    out
}

/// Puts `requests` in rbxport's inbox, each file written whole before it gets the name
/// rbxport reads. Earlier sync requests rbxport has not read yet are dropped first: these
/// replace them. Returns how many files were written.
pub fn write_requests(inbox: &Path, requests: &ToRbxport) -> std::io::Result<usize> {
    std::fs::create_dir_all(inbox)?;
    for entry in std::fs::read_dir(inbox)?.filter_map(Result::ok) {
        if entry.file_name().to_string_lossy().starts_with(QUIET) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mut written = 0;
    let mut put = |name: String, bytes: Vec<u8>| -> std::io::Result<()> {
        let partial = inbox.join(format!("{name}.tmp"));
        std::fs::write(&partial, bytes)?;
        let finished = inbox.join(&name);
        std::fs::rename(&partial, finished)?;
        written += 1;
        Ok(())
    };
    if !requests.tracks.is_empty() {
        let text: String = requests.tracks.iter().map(|t| format!("{t}\n")).collect();
        put(format!("{QUIET}{stamp}-0.paths"), text.into_bytes())?;
    }
    for (n, playlist) in requests.playlists.iter().enumerate() {
        let bytes = serde_json::to_vec(playlist).map_err(std::io::Error::other)?;
        put(format!("{QUIET}{stamp}-{:05}.playlist", n + 1), bytes)?;
    }
    Ok(written)
}

// ---------------------------------------------------------------------------------------------
// The DDJ-FLX4
// ---------------------------------------------------------------------------------------------

/// Which DDJ-FLX4 mapping Mixxx loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapping {
    /// The one that comes with Mixxx (2.4 and later): decks, jogs, hot cues, beat jump,
    /// beat loop, sampler, Beat FX.
    BuiltIn,
    /// ElHanko's community mapping, Extended preset (Mixxx 2.6 and later): adds Pad FX,
    /// Key Shift, stems, Smart CFX. Vendored in `third_party/mixxx-ddj-flx4-mapping`.
    Community,
}

impl Mapping {
    /// The mapping for a Mixxx of this version, `(major, minor)`; unknown means built-in.
    pub fn for_version(version: Option<(u32, u32)>) -> Self {
        match version {
            Some(v) if v >= (2, 6) => Self::Community,
            _ => Self::BuiltIn,
        }
    }

    /// The mapping file Mixxx is told to load (it looks in the profile's `controllers`
    /// folder, then in its own).
    pub fn file(self) -> &'static str {
        match self {
            Self::BuiltIn => "Pioneer-DDJ-FLX4.midi.xml",
            Self::Community => EXTENDED,
        }
    }
}

/// The names Mixxx gives a DDJ-FLX4 on the different systems, as its settings write them
/// (spaces and slashes made underscores).
pub const FLX4_NAMES: [&str; 4] = ["DDJ-FLX4", "DDJ-FLX4_MIDI_1", "DDJ-FLX4_1", "DDJ-FLX4_MIDI_1_1"];

const EXTENDED: &str = "Pioneer-DDJ-FLX4-2.0-Extended.midi.xml";
const BASIC: &str = "Pioneer-DDJ-FLX4-2.0.midi.xml";

macro_rules! vendored {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_bytes!(concat!("../../../third_party/mixxx-ddj-flx4-mapping/", $path)).as_slice())),*]
    };
}

/// ElHanko's mapping files, by their place in a Mixxx profile.
const COMMUNITY_FILES: [(&str, &[u8]); 19] = vendored!(
    "controllers/Pioneer-DDJ-FLX4-2.0.midi.xml",
    "controllers/Pioneer-DDJ-FLX4-2.0.script.js",
    "controllers/Pioneer-DDJ-FLX4-extended-scripts.js",
    "effects/chains/01_ECHO_1_4.xml",
    "effects/chains/02_ECHO_1_2.xml",
    "effects/chains/03_ECHO_1.xml",
    "effects/chains/04_ECHO_2.xml",
    "effects/chains/05_ECHO_4.xml",
    "effects/chains/06_REVERB_DJ.xml",
    "effects/chains/07_REVERB_WASH.xml",
    "effects/chains/08_TRANS_1_4.xml",
    "effects/chains/09_TRANS_1_2.xml",
    "effects/chains/10_TRANS_1.xml",
    "effects/chains/11_TRANS_2.xml",
    "effects/chains/12_TRANS_4.xml",
    "effects/chains/13_FLANGER_DJ.xml",
    "effects/chains/14_PHASER_DJ.xml",
    "effects/chains/C_01_FILTER_DJ.xml",
    "effects/chains/C_02_Noise.xml",
);

/// The Extended preset, made from the Basic one as the mapping's README says: its own name,
/// and the Extended script loaded after the Basic one.
fn extended_preset(basic: &str) -> Option<String> {
    let anchor = "filename=\"Pioneer-DDJ-FLX4-2.0.script.js\" />";
    let at = basic.find(anchor)? + anchor.len();
    let mut out = String::with_capacity(basic.len() + 128);
    out.push_str(&basic[..at]);
    out.push_str("\n      <file filename=\"Pioneer-DDJ-FLX4-extended-scripts.js\" functionprefix=\"\" />");
    out.push_str(&basic[at..]);
    let renamed = out.replacen("<name>Pioneer DDJ-FLX4 (Basic)</name>", "<name>Pioneer DDJ-FLX4 (Extended)</name>", 1);
    (renamed != out).then_some(renamed)
}

/// Copies ElHanko's mapping and effect chains into `profile`, with the Extended preset.
pub fn install_community_mapping(profile: &Path) -> std::io::Result<()> {
    for (place, bytes) in COMMUNITY_FILES {
        let target = profile.join(place);
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::read(&target).ok().as_deref() != Some(bytes) {
            std::fs::write(&target, bytes)?;
        }
    }
    let basic = COMMUNITY_FILES
        .iter()
        .find(|(place, _)| place.ends_with(BASIC))
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let extended = extended_preset(&basic).ok_or_else(|| std::io::Error::other("unexpected DDJ-FLX4 mapping"))?;
    std::fs::write(profile.join("controllers").join(EXTENDED), extended)
}

/// Mixxx's settings file: `[Group]` lines, each followed by `key value` lines.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    groups: Vec<(String, Vec<(String, String)>)>,
}

impl Settings {
    pub fn parse(text: &str) -> Self {
        let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                continue;
            }
            if line.starts_with('[') && line.trim_end().ends_with(']') {
                groups.push((line.trim().to_owned(), Vec::new()));
                continue;
            }
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            if groups.is_empty() {
                groups.push((String::new(), Vec::new()));
            }
            if let Some((_, entries)) = groups.last_mut() {
                entries.push((key.to_owned(), value.to_owned()));
            }
        }
        Self { groups }
    }

    pub fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.groups
            .iter()
            .filter(|(g, _)| g == group)
            .flat_map(|(_, entries)| entries.iter())
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, group: &str, key: &str, value: &str) {
        let index = match self.groups.iter().position(|(g, _)| g == group) {
            Some(i) => i,
            None => {
                self.groups.push((group.to_owned(), Vec::new()));
                self.groups.len() - 1
            }
        };
        let entries = &mut self.groups[index].1;
        match entries.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_owned(),
            None => entries.push((key.to_owned(), value.to_owned())),
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (group, entries) in &self.groups {
            if !group.is_empty() {
                out.push_str(group);
                out.push('\n');
            }
            for (key, value) in entries {
                out.push_str(key);
                out.push(' ');
                out.push_str(value);
                out.push('\n');
            }
            out.push('\n');
        }
        out
    }
}

/// Every mapping file YOBUKO has chosen at some point: a device set to one of these was set
/// by YOBUKO, so YOBUKO may change it; one set to anything else is the DJ's own choice.
const OURS: [&str; 3] = ["Pioneer-DDJ-FLX4.midi.xml", EXTENDED, BASIC];

/// Sets `profile` up for a DDJ-FLX4: the mapping files (for [`Mapping::Community`]), and the
/// controller enabled with that mapping in Mixxx's settings, unless the DJ chose another
/// mapping for it in Mixxx. A settings file without a version stays without one, so Mixxx
/// still runs its first-start setup (sound card, music folder) and keeps these lines.
pub fn configure(profile: &Path, mapping: Mapping) -> std::io::Result<()> {
    std::fs::create_dir_all(profile)?;
    if mapping == Mapping::Community {
        install_community_mapping(profile)?;
    }
    let path = profile.join(CONFIG);
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    let mut settings = Settings::parse(&before);
    for name in FLX4_NAMES {
        let chosen = settings.get("[ControllerPreset]", name).map(str::to_owned);
        let ours = chosen.as_deref().is_none_or(|file| {
            let file_name = Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            OURS.contains(&file_name.as_str())
        });
        if ours {
            settings.set("[ControllerPreset]", name, mapping.file());
            if settings.get("[Controller]", name).is_none() {
                settings.set("[Controller]", name, "1");
            }
        }
    }
    let after = settings.render();
    if after != before {
        let partial = path.with_extension("cfg.tmp");
        std::fs::write(&partial, after)?;
        std::fs::rename(&partial, &path)?;
    }
    Ok(())
}

/// The version of a Mixxx from its `VERSION` note (YOBUKO's packages write one beside the
/// Mixxx they ship) or, on macOS, its app's `Info.plist`. `(major, minor)`.
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let mut parts = text[start..].split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// The `CFBundleShortVersionString` of an app's `Info.plist` (XML form).
pub fn plist_version(plist: &str) -> Option<(u32, u32)> {
    let at = plist.find("<key>CFBundleShortVersionString</key>")?;
    let rest = &plist[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")?;
    parse_version(&rest[open..open + close])
}

#[cfg(test)]
#[path = "mixxx_tests.rs"]
mod tests;
