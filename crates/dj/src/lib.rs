// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fork's separate DJ catalog. Files stay in place; analysis never runs on the audio thread.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT: u32 = 2;
pub const MAX_TRACKS: usize = 2000;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_PLAYLISTS: usize = 128;
pub const MAX_PLAYLIST_ENTRIES: usize = 16000;

mod playlist;
mod storage;
pub use playlist::{Playlist, PlaylistEdit};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: u32,
    pub path: PathBuf,
    pub title: String,
    pub source_project: Option<PathBuf>,
    pub seconds: f64,
    pub bpm: Option<f64>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub revision: u64,
    pub next_playlist_id: u64,
}

impl Default for Catalog {
    fn default() -> Self {
        Self { format: FORMAT, tracks: Vec::new(), playlists: Vec::new(), revision: 0, next_playlist_id: 1 }
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
                || t.seconds <= 0.0
                || t.seconds > 1200.0
                || t.bpm.is_some_and(|b| !b.is_finite() || !(20.0..=400.0).contains(&b))
                || t.key.as_ref().is_some_and(|k| k.len() > 32)
                || t.source_project.as_ref().is_some_and(|p| !p.is_absolute() || p.as_os_str().len() > 4096)
            {
                return Err("invalid DJ catalog track".into());
            }
        }
        self.validate_playlists(&ids)?;
        Ok(())
    }

    /// Analyse on a worker, then insert or refresh one file without duplicating it.
    pub fn import(&mut self, path: &Path, source_project: Option<PathBuf>) -> Result<u32, String> {
        let path = path.canonicalize().map_err(|e| format!("audio file: {e}"))?;
        if self.tracks.len() >= MAX_TRACKS && !self.tracks.iter().any(|t| t.path == path) {
            return Err("the DJ catalog is full".into());
        }
        if suhina_media::info::kind_of(&path) != Some(suhina_media::info::FileKind::Sample) {
            return Err("choose a WAV, AIFF, FLAC, MP3 or Ogg audio file".into());
        }
        let info = suhina_media::info::sample_info(&path)?;
        if !info.duration_seconds.is_some_and(|s| s > 0.0 && s <= 1200.0)
            || info.sample_rate.is_none_or(|r| !(1000..=96000).contains(&r))
            || !info.channels.is_some_and(|c| (1..=2).contains(&c))
        {
            return Err("DJ analysis accepts up to 20 minutes of mono/stereo audio at up to 96 kHz".into());
        }
        let before = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        let audio = suhina_media::decode_file(&path)?;
        let channels = usize::from(audio.channels);
        if !(1..=2).contains(&channels) || !(1000..=96000).contains(&audio.sample_rate) {
            return Err("unsupported decoded audio format".into());
        }
        let mono: Vec<f32> = audio
            .samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
            .collect();
        if mono.is_empty()
            || mono.len() as u64 > u64::from(audio.sample_rate) * 1200
            || mono.iter().any(|s| !s.is_finite())
        {
            return Err("invalid or oversized audio".into());
        }
        let result =
            rbl_analysis::analyse_with(&mono, audio.sample_rate, rbl_analysis::AnalysisPreset::Rbxport.options());
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
        let track = Track {
            id,
            title: path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_owned(),
            path,
            source_project: source_project
                .or_else(|| existing.and_then(|i| self.tracks.get(i)).and_then(|t| t.source_project.clone())),
            seconds: mono.len() as f64 / f64::from(audio.sample_rate),
            bpm: (result.tempo.bpm.is_finite() && (20.0..=400.0).contains(&result.tempo.bpm))
                .then_some(result.tempo.bpm),
            key: result.key.map(|k| k.name),
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod playlist_tests;
