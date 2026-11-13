// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fork's separate DJ catalog. Files stay in place; analysis never runs on the audio thread.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT: u32 = 4;
/// The most tracks a library holds: a large collection's, as music folders bring in.
pub const MAX_TRACKS: usize = 100_000;
const MAX_BYTES: u64 = 128 * 1024 * 1024;
/// The longest track kept, in seconds (4 hours: a recorded set).
pub const MAX_SECONDS: f64 = 4.0 * 3600.0;
/// The longest track rbxport's analysis decodes, in seconds.
pub const MAX_ANALYSED_SECONDS: f64 = 20.0 * 60.0;
pub const MAX_PLAYLISTS: usize = 128;
pub const MAX_PLAYLIST_ENTRIES: usize = 16000;

mod grid;
pub use grid::{BeatGrid, GridLine};
mod cue;
pub use cue::{Cue, CueEdit, CueSet};
mod playlist;
pub mod scan;
mod storage;
pub use playlist::{Playlist, PlaylistEdit, PlaylistKind};
pub use scan::{Analysed, Found, MAX_FOLDERS, is_dj_audio};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: u32,
    pub path: PathBuf,
    pub title: String,
    pub source_project: Option<PathBuf>,
    /// The length in seconds; 0 while unknown (a file whose header does not say, until it is
    /// analysed).
    pub seconds: f64,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    /// The file's tags. Each is left out of the file when empty, so a library written before
    /// them reads and writes unchanged.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub album: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub genre: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    /// When the track came into the library, in seconds since 1970.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u64>,
    /// rbxport's analysis has run on it (its tempo and key, or none found).
    #[serde(default, skip_serializing_if = "is_false")]
    pub analysed: bool,
    /// Its file was not found at the last scan of the music folders.
    #[serde(default, skip_serializing_if = "is_false")]
    pub missing: bool,
    /// 0 to 5 stars.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rating: u8,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &u8) -> bool {
    *n == 0
}

impl Track {
    /// Whether rbxport's analysis still has to run on it: a track a music folder brought in
    /// and not analysed yet. (A track imported one by one was analysed as it came in.)
    pub fn needs_analysis(&self) -> bool {
        !self.analysed && self.added.is_some() && !self.missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub revision: u64,
    pub next_playlist_id: u64,
    pub cues: Vec<CueSet>,
    pub grids: Vec<BeatGrid>,
    /// The music folders scanned into the library, like Mixxx's library folders.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<PathBuf>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            format: FORMAT,
            tracks: Vec::new(),
            playlists: Vec::new(),
            revision: 0,
            next_playlist_id: 1,
            cues: Vec::new(),
            grids: Vec::new(),
            folders: Vec::new(),
        }
    }
}

impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.tracks.len() > MAX_TRACKS {
            return Err("unsupported DJ catalog format or track limit".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        for t in &self.tracks {
            if t.id == 0
                || !ids.insert(t.id)
                || !paths.insert(&t.path)
                || !t.path.is_absolute()
                || t.path.as_os_str().len() > 4096
                || t.title.is_empty()
                || t.title.len() > 512
                || !t.seconds.is_finite()
                || t.seconds < 0.0
                || t.seconds > MAX_SECONDS
                || [&t.artist, &t.album, &t.genre, &t.comment].iter().any(|s| s.len() > 512)
                || t.rating > 5
                || t.bpm.is_some_and(|b| !b.is_finite() || !(20.0..=400.0).contains(&b))
                || t.key.as_ref().is_some_and(|k| k.len() > 32)
                || t.source_project.as_ref().is_some_and(|p| !p.is_absolute() || p.as_os_str().len() > 4096)
            {
                return Err("invalid DJ catalog track".into());
            }
        }
        if self.folders.len() > MAX_FOLDERS
            || self.folders.iter().any(|f| !f.is_absolute() || f.as_os_str().len() > 4096)
        {
            return Err("invalid DJ music folders".into());
        }
        self.validate_playlists(&ids)?;
        self.validate_cues()?;
        self.validate_grids()?;
        Ok(())
    }

    /// Analyse on a worker, then insert or refresh one file without duplicating it.
    pub fn import(&mut self, path: &Path, source_project: Option<PathBuf>) -> Result<u32, String> {
        let path = path.canonicalize().map_err(|e| format!("audio file: {e}"))?;
        if self.tracks.len() >= MAX_TRACKS && !self.tracks.iter().any(|t| t.path == path) {
            return Err("the DJ catalog is full".into());
        }
        if !is_dj_audio(&path) {
            return Err("choose an M4A, AAC, MP3, FLAC, WAV, AIFF or Ogg audio file".into());
        }
        let found = scan::read(&path)?;
        let info = suhina_media::info::sample_info(&path)?;
        if !info.duration_seconds.is_some_and(|s| s > 0.0 && s <= MAX_ANALYSED_SECONDS)
            || info.sample_rate.is_none_or(|r| !(1000..=96000).contains(&r))
            || !info.channels.is_some_and(|c| (1..=2).contains(&c))
        {
            return Err("DJ analysis accepts up to 20 minutes of mono/stereo audio at up to 96 kHz".into());
        }
        let before = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        let (result, seconds) = analyse(&path)?;
        let after = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            return Err("the audio file changed during analysis; import it again".into());
        }
        let existing = self.tracks.iter().position(|t| t.path == path);
        let id = match existing.and_then(|i| self.tracks.get(i)) {
            Some(t) => t.id,
            None => {
                self.tracks.iter().map(|t| t.id).max().unwrap_or(0).checked_add(1).ok_or("DJ track ids exhausted")?
            }
        };
        let old = existing.and_then(|i| self.tracks.get(i));
        let track = Track {
            id,
            path,
            source_project: source_project.or_else(|| old.and_then(|t| t.source_project.clone())),
            seconds,
            bpm: (result.tempo.bpm.is_finite() && (20.0..=400.0).contains(&result.tempo.bpm))
                .then_some(result.tempo.bpm),
            key: result.key.map(|k| k.name),
            analysed: true,
            added: old.and_then(|t| t.added).or_else(scan::now),
            rating: old.map_or(0, |t| t.rating),
            ..found.track()
        };
        let mut next = self.clone();
        if let Some(t) = existing.and_then(|i| next.tracks.get_mut(i)) {
            *t = track;
        } else {
            next.tracks.push(track);
        }
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(id)
    }
}

/// Decodes one file and runs rbxport's analysis on it: the result and the length in seconds.
pub(crate) fn analyse(path: &Path) -> Result<(rbl_analysis::Analysis, f64), String> {
    let audio = suhina_media::decode_file(path)?;
    let channels = usize::from(audio.channels);
    if !(1..=2).contains(&channels) || !(1000..=96000).contains(&audio.sample_rate) {
        return Err("unsupported decoded audio format".into());
    }
    let mono: Vec<f32> = audio
        .samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
        .collect();
    if mono.is_empty() || mono.len() as u64 > u64::from(audio.sample_rate) * 1200 || mono.iter().any(|s| !s.is_finite())
    {
        return Err("invalid or oversized audio".into());
    }
    let result = rbl_analysis::analyse_with(&mono, audio.sample_rate, rbl_analysis::AnalysisPreset::Rbxport.options());
    Ok((result, mono.len() as f64 / f64::from(audio.sample_rate)))
}

pub mod look;
pub mod mixxx;
pub mod mixxx_skin;
pub mod usb;
pub use usb::{UsbProgress, UsbSummary, export_usb};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod playlist_tests;

#[cfg(test)]
mod cue_tests;

#[cfg(test)]
mod grid_tests;

#[cfg(test)]
mod scan_tests;
