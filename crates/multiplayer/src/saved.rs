// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sessions kept on this computer, to come back to them later with friends
//! (`docs/spec/saved-sessions.md`).
//!
//! Each session lives in `<Shared projects>/Sessions/<folder>/`: `session.json` says what it is
//! and `session.suhina` is the project as it was last saved. The folder is the session's id for
//! the version this computer keeps as its own (the main version when this computer created the
//! session, else this computer's backup copy); a creator who joins a friend's backup copy keeps
//! that copy in `<id>-copy`, so the main version is never overwritten by it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{MAX_SESSION_TEXT, SessionInfo};

/// The project file in a saved session's folder.
pub const PROJECT_FILE: &str = "session.suhina";
const META_FILE: &str = "session.json";
const PROFILE_FILE: &str = "me.json";
/// Most sessions listed, newest first.
pub const MAX_LISTED: usize = 64;

/// One session kept on this computer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SavedSession {
    pub info: SessionInfo,
    /// Whether this computer keeps the main version: it created the session.
    pub mine: bool,
    /// Seconds since 1970 when it was last saved.
    pub saved_at: u64,
    /// Where it is kept; not written in the file.
    #[serde(skip)]
    pub folder: PathBuf,
}

impl SavedSession {
    /// The project file to open.
    pub fn project(&self) -> PathBuf {
        self.folder.join(PROJECT_FILE)
    }
}

/// The folder saved sessions live in, under the multiplayer storage root.
pub fn folder(root: &Path) -> PathBuf {
    root.join("Sessions")
}

/// Where this computer keeps session `info`: its own folder, or `<id>-copy` for a backup copy
/// of a session whose main version this computer holds.
pub fn place(root: &Path, info: &SessionInfo) -> Result<PathBuf, String> {
    info.validate()?;
    let own = folder(root).join(&info.id);
    let main_here = read(&own).is_ok_and(|saved| saved.mine);
    if main_here && !info.main {
        return Ok(folder(root).join(format!("{}-copy", info.id)));
    }
    Ok(own)
}

/// Writes what `folder` keeps: the session `info`, whether it is this computer's main version
/// and when. The project file is written beside it by the caller (`project.save`).
pub fn write(folder: &Path, info: &SessionInfo, mine: bool, saved_at: u64) -> Result<(), String> {
    info.validate()?;
    std::fs::create_dir_all(folder).map_err(|e| format!("could not create the session folder: {e}"))?;
    let saved = SavedSession { info: info.clone(), mine, saved_at, folder: PathBuf::new() };
    let json = serde_json::to_vec_pretty(&saved).map_err(|_| "could not encode the session")?;
    // Written beside it first, then renamed over it, so a cut never leaves half a file.
    let partial = folder.join(format!("{META_FILE}.part"));
    std::fs::write(&partial, json).map_err(|e| format!("could not save the session: {e}"))?;
    std::fs::rename(&partial, folder.join(META_FILE)).map_err(|e| format!("could not save the session: {e}"))
}

/// Reads the session kept in `folder`.
pub fn read(folder: &Path) -> Result<SavedSession, String> {
    let bytes = read_small(&folder.join(META_FILE))?;
    let mut saved: SavedSession = serde_json::from_slice(&bytes).map_err(|_| "unreadable session file")?;
    saved.info.validate()?;
    saved.folder = folder.to_path_buf();
    Ok(saved)
}

/// The sessions kept on this computer that still have their project file, newest first.
pub fn list(root: &Path) -> Vec<SavedSession> {
    let Ok(entries) = std::fs::read_dir(folder(root)) else { return Vec::new() };
    let mut sessions: Vec<SavedSession> = entries
        .flatten()
        .take(4 * MAX_LISTED)
        .filter_map(|entry| read(&entry.path()).ok())
        .filter(|saved| saved.project().is_file())
        .collect();
    sessions.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| a.info.name.cmp(&b.info.name)));
    sessions.truncate(MAX_LISTED);
    sessions
}

/// The name this computer's person goes by in sessions, as last typed.
pub fn my_name(root: &Path) -> Option<String> {
    let bytes = read_small(&folder(root).join(PROFILE_FILE)).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let name = value.get("name")?.as_str()?.trim();
    (!name.is_empty() && name.len() <= MAX_SESSION_TEXT && !name.chars().any(char::is_control)).then(|| name.to_owned())
}

/// Keeps the name this computer's person goes by in sessions.
pub fn set_my_name(root: &Path, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_SESSION_TEXT || name.chars().any(char::is_control) {
        return Err(format!("your name takes 1 to {MAX_SESSION_TEXT} bytes"));
    }
    let dir = folder(root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create the sessions folder: {e}"))?;
    let json = serde_json::json!({ "name": name }).to_string();
    std::fs::write(dir.join(PROFILE_FILE), json).map_err(|e| format!("could not save your name: {e}"))
}

fn read_small(path: &Path) -> Result<Vec<u8>, String> {
    const MOST: u64 = 64 * 1024;
    let size = std::fs::metadata(path).map_err(|_| "no session file")?.len();
    if size > MOST {
        return Err("session file too large".into());
    }
    std::fs::read(path).map_err(|_| "unreadable session file".into())
}

#[cfg(test)]
#[path = "saved_tests.rs"]
mod tests;
