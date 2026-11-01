// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rekordbox USB export of the DJ library, written by rbxport's exporter.
//!
//! The stick gets what a CDJ or XDJ browses: `Contents/` (the audio),
//! `PIONEER/rekordbox/export.pdb` and `exportLibrary.db` (the databases) and
//! `PIONEER/USBANLZ/` (beat grids, waveforms and cues). Every track is analysed
//! again here with rbxport's analysis, so the waveforms on the player match the
//! audio; a grid the person confirmed in the DJ library replaces the detected
//! one. The source files are only read.

use crate::{BeatGrid, Catalog, Track};
use std::path::Path;

/// How far an export has got: shown under the button while it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsbProgress {
    /// `analysing`, `checking`, `copying`, `database`, `verifying` or `publishing`.
    pub stage: String,
    pub done: usize,
    pub total: usize,
    pub title: String,
}

/// What the stick now holds, in a few numbers the panel can say back.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct UsbSummary {
    pub destination: String,
    pub tracks: usize,
    pub playlists: usize,
    pub copied: usize,
    pub reused: usize,
    pub converted: usize,
    pub removed: usize,
    pub verified: bool,
    /// Tracks left out, as "title: reason".
    pub failed: Vec<String>,
    pub warnings: usize,
}

/// Hot cues A to H, in the master database's numbering (memory = 0, A-C = 1-3, D-P = 5-17).
const HOT_CUE_KINDS: [u8; 8] = [1, 2, 3, 5, 6, 7, 8, 9];

fn ms(seconds: f64) -> u32 {
    (seconds * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// A confirmed constant grid as the beats ANLZ stores, every beat to the end of the track.
fn grid_beats(grid: &BeatGrid, seconds: f64) -> Vec<rbl_anlz::Beat> {
    if !grid.valid_for(seconds) {
        return Vec::new();
    }
    let period = 60.0 / grid.bpm;
    let tempo_x100 = (grid.bpm * 100.0).round().clamp(0.0, f64::from(u16::MAX)) as u16;
    let count = ((seconds - grid.first_beat) / period).ceil().clamp(0.0, 20_000.0) as usize;
    (0..count)
        .map(|i| (i, grid.first_beat + i as f64 * period))
        .filter(|(_, at)| *at < seconds)
        .map(|(i, at)| rbl_anlz::Beat { beat_number: (i % 4) as u16 + 1, tempo_x100, time_ms: ms(at) })
        .collect()
}

/// The library's cue points as rekordbox cues: the first eight are hot cues A to H, the
/// rest memory cues; each keeps its name as the comment.
fn cues_of(catalog: &Catalog, track: u32) -> Vec<rbl_anlz::cues::ExportCue> {
    let Some(set) = catalog.cues.iter().find(|c| c.track == track) else { return Vec::new() };
    let mut points: Vec<_> = set.points.iter().collect();
    points.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let mut out = Vec::new();
    for (i, cue) in points.iter().enumerate() {
        let base = rbl_anlz::cues::ExportCue {
            kind: 0,
            time_ms: ms(cue.seconds),
            loop_time_ms: None,
            color_id: 0,
            color_code: 0,
            comment: cue.name.clone(),
            active_loop: false,
            loop_numerator: 0,
            loop_denominator: 0,
        };
        if let Some(&kind) = HOT_CUE_KINDS.get(i) {
            out.push(rbl_anlz::cues::ExportCue { kind, ..base.clone() });
        }
        out.push(base);
    }
    out
}

/// One library track ready for the exporter, its analysis files authored.
fn source_track(catalog: &Catalog, track: &Track) -> Result<rbl_export::SourceTrack, String> {
    let (analysis, seconds) = crate::analyse(&track.path)?;
    let grid = catalog.grids.iter().find(|g| g.track == track.id);
    let beats = match grid.map(|g| grid_beats(g, seconds)) {
        Some(beats) if !beats.is_empty() => beats,
        _ => analysis
            .tempo
            .beats
            .iter()
            .map(|b| rbl_anlz::Beat { beat_number: b.beat_number, tempo_x100: b.tempo_x100, time_ms: b.time_ms })
            .collect(),
    };
    let columns: Vec<rbl_anlz::BandColumn> = analysis
        .waveform
        .columns
        .iter()
        .map(|c| rbl_anlz::BandColumn { low: c.low, mid: c.mid, high: c.high, peak: c.peak })
        .collect();
    let name = track.path.file_name().and_then(|n| n.to_str()).unwrap_or("track");
    let files = rbl_anlz::author_with_overview(
        &format!("/Contents/{name}"),
        &beats,
        &columns,
        analysis.waveform.overview.as_slice().try_into().ok(),
        rbl_anlz::Existing::default(),
    );
    let bpm = grid.map(|g| g.bpm).or(track.bpm).unwrap_or(analysis.tempo.bpm);
    let info = suhina_media::info::sample_info(&track.path).ok();
    let sample_rate = info.as_ref().and_then(|i| i.sample_rate).unwrap_or(0);
    let file_size = std::fs::metadata(&track.path).map(|m| m.len()).unwrap_or(0);
    let bitrate = if seconds > 0.0 { ((file_size as f64 * 8.0) / seconds / 1000.0).round() as u32 } else { 0 };
    let cues = cues_of(catalog, track.id);
    Ok(rbl_export::SourceTrack {
        metadata: rbl_core::ExportMetadata { analysed: 1, ..Default::default() },
        cues: Some(cues),
        id: u64::from(track.id),
        source_path: track.path.clone(),
        title: track.title.clone(),
        artist: if track.source_project.is_some() { "YOBUKO".into() } else { String::new() },
        key: track.key.clone().unwrap_or_default(),
        comment: "Exported from YOBUKO".into(),
        bpm_x100: (bpm * 100.0).round().clamp(0.0, f64::from(u32::MAX)) as u32,
        duration_sec: seconds.round().clamp(0.0, f64::from(u16::MAX)) as u16,
        sample_rate,
        bitrate,
        file_size,
        analysis: vec![("DAT".into(), files.dat), ("EXT".into(), files.ext), ("2EX".into(), files.two_ex)],
        ..Default::default()
    })
}

/// Writes (or brings up to date) a Rekordbox USB export of the whole DJ library and its
/// playlists in `destination`, normally the root of a FAT32 or exFAT stick.
///
/// A stick that already holds one of these exports is synced: only what changed is
/// copied, and tracks no longer in the library are taken off. Tracks that cannot be read
/// are left out and listed in [`UsbSummary::failed`]; the rest is still written.
pub fn export_usb(
    catalog: &Catalog,
    destination: &Path,
    progress: &mut dyn FnMut(&UsbProgress),
) -> Result<UsbSummary, String> {
    if !destination.is_dir() {
        return Err("choose a folder or the root of a USB stick".into());
    }
    if catalog.tracks.is_empty() {
        return Err("the DJ library is empty: import or render a track first".into());
    }
    let total = catalog.tracks.len();
    let mut tracks = Vec::with_capacity(total);
    let mut index_of = std::collections::HashMap::new();
    let mut failed = Vec::new();
    for (done, track) in catalog.tracks.iter().enumerate() {
        progress(&UsbProgress { stage: "analysing".into(), done, total, title: track.title.clone() });
        match source_track(catalog, track) {
            Ok(source) => {
                index_of.insert(track.id, tracks.len());
                tracks.push(source);
            }
            Err(e) => failed.push(format!("{}: {e}", track.title)),
        }
    }
    if tracks.is_empty() {
        return Err(format!("no track could be read: {}", failed.join("; ")));
    }
    let playlists: Vec<rbl_export::SourcePlaylist> = catalog
        .playlists
        .iter()
        .map(|p| rbl_export::SourcePlaylist {
            id: u64::from(p.id),
            name: p.name.clone(),
            track_indices: p.tracks.iter().filter_map(|t| index_of.get(t).copied()).collect(),
            ..Default::default()
        })
        .collect();
    let options = rbl_export::ExportOptions {
        // CDJs read 16/24-bit WAV and AIFF and MP3; anything else becomes a 16-bit WAV copy.
        compatibility: Some(rbl_export::CompatibilityFormat::Wav),
        ..Default::default()
    };
    let report = rbl_export::export_with_options(destination, &tracks, &playlists, &[], &options, &mut |p| {
        progress(&UsbProgress { stage: p.stage.into(), done: p.done, total: p.total, title: p.title.clone() });
    })
    .map_err(|e| format!("USB export: {e}"))?;
    failed.extend(report.failed.iter().cloned());
    failed.extend(report.skipped.iter().map(|t| format!("{t}: audio missing")));
    Ok(UsbSummary {
        destination: destination.display().to_string(),
        tracks: report.tracks,
        playlists: report.playlists,
        copied: report.tracks_added + report.tracks_updated,
        reused: report.reused,
        converted: report.converted,
        removed: report.removed,
        verified: report.verified,
        failed,
        warnings: report.warnings.len(),
    })
}

#[cfg(test)]
mod tests;
