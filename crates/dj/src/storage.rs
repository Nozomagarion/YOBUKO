// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Catalog, CueSet, FORMAT, MAX_BYTES, Playlist, Track};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCatalog {
    format: u32,
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaylistCatalog {
    format: u32,
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    revision: u64,
    next_playlist_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CueCatalog {
    format: u32,
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    revision: u64,
    next_playlist_id: u64,
    cues: Vec<CueSet>,
}

struct LegacyBackup {
    format: u32,
    bytes: Vec<u8>,
}

fn read(path: &Path) -> Result<(Catalog, Option<LegacyBackup>), String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Catalog::default(), None)),
        Err(e) => return Err(format!("could not read the DJ catalog: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("the DJ catalog is too large".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid DJ catalog: {e}"))?;
    let (catalog, legacy) = if value["format"].as_u64() == Some(1) {
        let old: LegacyCatalog = serde_json::from_slice(&bytes).map_err(|e| format!("invalid DJ catalog: {e}"))?;
        if old.format != 1 {
            return Err("unsupported DJ catalog format".into());
        }
        (Catalog { tracks: old.tracks, ..Catalog::default() }, Some(LegacyBackup { format: 1, bytes }))
    } else if value["format"].as_u64() == Some(2) {
        let old: PlaylistCatalog = serde_json::from_slice(&bytes).map_err(|e| format!("invalid DJ catalog: {e}"))?;
        if old.format != 2 {
            return Err("unsupported DJ catalog format".into());
        }
        (
            Catalog {
                tracks: old.tracks,
                playlists: old.playlists,
                revision: old.revision,
                next_playlist_id: old.next_playlist_id,
                ..Catalog::default()
            },
            Some(LegacyBackup { format: 2, bytes }),
        )
    } else if value["format"].as_u64() == Some(3) {
        let old: CueCatalog = serde_json::from_slice(&bytes).map_err(|e| format!("invalid DJ catalog: {e}"))?;
        if old.format != 3 {
            return Err("unsupported DJ catalog format".into());
        }
        (
            Catalog {
                tracks: old.tracks,
                playlists: old.playlists,
                revision: old.revision,
                next_playlist_id: old.next_playlist_id,
                cues: old.cues,
                ..Catalog::default()
            },
            Some(LegacyBackup { format: 3, bytes }),
        )
    } else {
        let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|e| format!("invalid DJ catalog: {e}"))?;
        (catalog, None)
    };
    catalog.validate()?;
    Ok((catalog, legacy))
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Self, String> {
        read(path).map(|(c, _)| c)
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("the DJ catalog is too large".into());
        }
        Ok(bytes)
    }

    /// Atomic publication. App writers use save_checked to detect concurrent edits as well.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = self.bytes()?;
        let parent = path.parent().ok_or("the DJ catalog needs a folder")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        temp.write_all(&bytes).and_then(|_| temp.as_file().sync_all()).map_err(|e| e.to_string())?;
        temp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Short cooperative lock; analysis happens before this, on a worker.
    pub fn save_checked(&self, expected: &Self, path: &Path) -> Result<(), String> {
        self.bytes()?;
        if self.format != FORMAT || expected.revision.checked_add(1) != Some(self.revision) {
            return Err("invalid DJ catalog revision".into());
        }
        let parent = path.parent().ok_or("the DJ catalog needs a folder")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|_| "another app is updating the DJ library; refresh and try again")?;
        let (disk, legacy) = read(path)?;
        if &disk != expected {
            return Err("the DJ library changed in another app; use Refresh DJ library".into());
        }
        if let Some(LegacyBackup { format, bytes }) = legacy {
            let mut backup = tempfile::Builder::new()
                .prefix(&format!("dj-library-v{format}-backup-"))
                .suffix(".json")
                .tempfile_in(parent)
                .map_err(|e| e.to_string())?;
            backup.write_all(&bytes).and_then(|_| backup.as_file().sync_all()).map_err(|e| e.to_string())?;
            backup.keep().map_err(|e| e.to_string())?;
        }
        self.save(path)
    }
}
