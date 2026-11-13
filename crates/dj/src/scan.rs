// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Music folders: the audio files in them and their tags, for the library. Reading a file's
//! tags opens its header only (never its audio), so a large collection comes in quickly;
//! rbxport's analysis (tempo and key) runs on each track afterwards.

use crate::Track;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, RawValue, StandardTag, Tag};

/// The most music folders a library scans.
pub const MAX_FOLDERS: usize = 32;
/// The most files and folders one scan looks at, so a scan of a whole disk ends.
pub const MAX_VISITED: usize = 1_000_000;
/// The file types the library takes: what the decoder plays on every platform.
const EXTENSIONS: &[&str] =
    &["m4a", "mp4", "aac", "alac", "mp3", "flac", "wav", "wave", "aif", "aiff", "aifc", "ogg", "oga"];

/// Whether `path` is an audio file the library takes, by its extension.
pub fn is_dj_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(e)))
}

/// Now, in seconds since 1970.
pub(crate) fn now() -> Option<u64> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// An audio file found in a music folder, with what its tags say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub comment: String,
    pub year: Option<u16>,
    /// The length in seconds when the header says; 0 otherwise.
    pub seconds: f64,
    /// A tempo and key from the tags (a DJ program's earlier analysis), when there are.
    pub bpm: Option<f64>,
    pub key: Option<String>,
}

impl Found {
    /// The library track it makes (id 0 until the catalog gives one).
    pub fn track(&self) -> Track {
        Track {
            id: 0,
            path: self.path.clone(),
            title: self.title.clone(),
            source_project: None,
            seconds: self.seconds,
            bpm: self.bpm,
            key: self.key.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            genre: self.genre.clone(),
            comment: self.comment.clone(),
            year: self.year,
            added: now(),
            ..Track::default()
        }
    }
}

/// A tag's text, trimmed, at most 512 bytes, without control characters; empty for none.
fn clean(text: &str) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    let mut text = text.trim().to_owned();
    if text.len() > 512 {
        let mut end = 512;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// The first four-digit year in a date such as `2024`, `2024-05-01` or `2024/05`.
fn year_of(text: &str) -> Option<u16> {
    let digits: String = text.trim().chars().take(4).collect();
    digits.parse::<u16>().ok().filter(|y| (1000..=9999).contains(y))
}

/// Reads the header and tags of the audio file at `path` (not its audio).
pub fn read(path: &Path) -> Result<Found, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, stream, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("unsupported or unreadable audio: {e}"))?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled");
    let mut found = Found { path: path.to_path_buf(), title: clean(stem), ..Found::default() };
    if found.title.is_empty() {
        found.title = "Untitled".into();
    }
    let audio = format.default_track(TrackType::Audio).ok_or("no audio in the file")?;
    let rate = audio.codec_params.as_ref().and_then(|p| p.audio()).and_then(|p| p.sample_rate);
    if let (Some(frames), Some(rate)) = (audio.num_frames, rate.filter(|r| *r > 0)) {
        let seconds = frames as f64 / f64::from(rate);
        if seconds.is_finite() && seconds > 0.0 && seconds <= crate::MAX_SECONDS {
            found.seconds = seconds;
        }
    }
    let mut tags: Vec<Tag> = Vec::new();
    if let Some(revision) = format.metadata().skip_to_latest() {
        tags.extend(revision.media.tags.iter().cloned());
        for track in &revision.per_track {
            tags.extend(track.metadata.tags.iter().cloned());
        }
    }
    let mut title = None;
    for tag in &tags {
        let Some(std) = &tag.std else { continue };
        let set = |slot: &mut String, text: &str| {
            if slot.is_empty() {
                *slot = clean(text);
            }
        };
        match std {
            StandardTag::TrackTitle(t) if title.is_none() => title = Some(clean(t)),
            StandardTag::Artist(t) => set(&mut found.artist, t),
            StandardTag::Album(t) => set(&mut found.album, t),
            StandardTag::Genre(t) => set(&mut found.genre, t),
            StandardTag::Comment(t) => set(&mut found.comment, t),
            StandardTag::ReleaseYear(y) | StandardTag::RecordingYear(y) | StandardTag::OriginalReleaseYear(y) => {
                found.year = found.year.or(Some(*y).filter(|y| (1000..=9999).contains(y)));
            }
            StandardTag::ReleaseDate(t) | StandardTag::RecordingDate(t) | StandardTag::OriginalReleaseDate(t) => {
                found.year = found.year.or_else(|| year_of(t));
            }
            StandardTag::Bpm(bpm) if found.bpm.is_none() => {
                found.bpm = Some(*bpm as f64).filter(|b| (20.0..=400.0).contains(b));
            }
            _ => {}
        }
        if found.key.is_none()
            && matches!(tag.raw.key.to_ascii_uppercase().as_str(), "KEY" | "INITIALKEY" | "TKEY")
            && let RawValue::String(s) = &tag.raw.value
        {
            found.key = Some(clean(s)).filter(|k| !k.is_empty() && k.len() <= 32);
        }
    }
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        found.title = title;
    }
    Ok(found)
}

/// Calls `visit` with each audio file under `folder` (hidden files and folders left out,
/// links not followed), until `visit` returns false, `stop` is set or [`MAX_VISITED`]
/// entries have been looked at. Folders that cannot be read are skipped.
pub fn walk(folder: &Path, stop: &AtomicBool, visit: &mut dyn FnMut(PathBuf) -> bool) {
    let mut folders = vec![folder.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files = Vec::new();
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_VISITED || stop.load(Ordering::Relaxed) {
                return;
            }
            let hidden = entry.file_name().to_str().is_none_or(|n| n.starts_with('.'));
            let Ok(kind) = entry.file_type() else { continue };
            if hidden || kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                folders.push(entry.path());
            } else if kind.is_file() && is_dj_audio(&entry.path()) {
                files.push(entry.path());
            }
        }
        // Each folder's files in name order, so an album comes in as it is numbered.
        files.sort();
        for file in files {
            if !visit(file) {
                return;
            }
        }
    }
}

/// What rbxport's analysis found in one track: its tempo, key and length.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysed {
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub seconds: f64,
}

/// Runs rbxport's analysis on the track at `path` (decoding it): its tempo and key. A track
/// longer than [`crate::MAX_ANALYSED_SECONDS`] is not decoded.
pub fn analyse(path: &Path, seconds: f64) -> Result<Analysed, String> {
    if seconds > crate::MAX_ANALYSED_SECONDS {
        return Err("too long to analyse".into());
    }
    let (result, seconds) = crate::analyse(path)?;
    Ok(Analysed {
        bpm: Some(result.tempo.bpm).filter(|b| b.is_finite() && (20.0..=400.0).contains(b)),
        key: result.key.map(|k| k.name),
        seconds,
    })
}

impl crate::Catalog {
    /// Sets the music folders the library scans (absolute, at most [`MAX_FOLDERS`]).
    pub fn set_folders(&mut self, folders: Vec<PathBuf>) -> Result<(), String> {
        let mut next = self.clone();
        next.folders = folders;
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Adds the files a scan found that the library does not hold yet, and brings back the
    /// ones it had lost. Returns how many came in; stops at [`crate::MAX_TRACKS`].
    pub fn add_found(&mut self, found: Vec<Found>) -> Result<usize, String> {
        let mut next = self.clone();
        let mut known: std::collections::HashMap<PathBuf, usize> =
            next.tracks.iter().enumerate().map(|(i, t)| (t.path.clone(), i)).collect();
        let mut id = next.tracks.iter().map(|t| t.id).max().unwrap_or(0);
        let mut added = 0;
        for file in found {
            if !file.path.is_absolute() {
                continue;
            }
            if let Some(track) = known.get(&file.path).and_then(|&i| next.tracks.get_mut(i)) {
                track.missing = false;
                continue;
            }
            if next.tracks.len() >= crate::MAX_TRACKS {
                break;
            }
            id = id.checked_add(1).ok_or("DJ track ids exhausted")?;
            known.insert(file.path.clone(), next.tracks.len());
            next.tracks.push(Track { id, ..file.track() });
            added += 1;
        }
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(added)
    }

    /// After a whole scan of the music folders: the tracks inside them that the scan did not
    /// see are marked missing (kept, with their cues, in case the drive comes back).
    pub fn mark_missing(&mut self, seen: &std::collections::HashSet<PathBuf>) -> Result<usize, String> {
        let mut next = self.clone();
        let mut lost = 0;
        for track in &mut next.tracks {
            let inside = next.folders.iter().any(|f| track.path.starts_with(f));
            let missing = inside && !seen.contains(&track.path) && !track.path.exists();
            if missing && !track.missing {
                lost += 1;
            }
            if inside {
                track.missing = missing;
            }
        }
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(lost)
    }

    /// Keeps what rbxport's analysis found for each track (`Err` marks a track that could not
    /// be analysed, so it is not tried again).
    pub fn set_analysis(&mut self, results: Vec<(u32, Result<Analysed, String>)>) -> Result<(), String> {
        let mut next = self.clone();
        for (id, result) in results {
            let Some(track) = next.tracks.iter_mut().find(|t| t.id == id) else { continue };
            track.analysed = true;
            if let Ok(found) = result {
                track.bpm = track.bpm.or(found.bpm);
                track.key = track.key.take().or(found.key);
                if found.seconds.is_finite() && found.seconds > 0.0 && found.seconds <= crate::MAX_SECONDS {
                    track.seconds = found.seconds;
                }
            }
        }
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Sets a track's rating, 0 to 5 stars.
    pub fn set_rating(&mut self, revision: u64, track: u32, stars: u8) -> Result<(), String> {
        if revision != self.revision {
            return Err("the DJ library changed; try again".into());
        }
        if stars > 5 {
            return Err("a rating is 0 to 5 stars".into());
        }
        let mut next = self.clone();
        next.tracks.iter_mut().find(|t| t.id == track).ok_or("unknown DJ track")?.rating = stars;
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(())
    }
}
