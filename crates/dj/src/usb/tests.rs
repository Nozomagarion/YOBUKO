// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{Cue, CueSet, Playlist};

/// Four seconds of clicks at 120 BPM, as the float WAV a render writes.
fn clicks(path: &Path) {
    let rate = 44_100u32;
    let samples: Vec<f32> = (0..rate * 4)
        .map(|i| {
            let in_beat = i % (rate / 2);
            if in_beat < 400 { (in_beat as f32 * 0.3).sin() * 0.8 } else { 0.0 }
        })
        .collect();
    let stereo: Vec<f32> = samples.iter().flat_map(|s| [*s, *s]).collect();
    suhina_media::write::write_audio_file(
        path,
        &stereo,
        &suhina_media::write::AudioFileSpec::float_wav(rate),
        suhina_media::write::WriteMode::Export { overwrite: false },
    )
    .unwrap();
}

#[test]
fn a_confirmed_grid_becomes_numbered_beats_to_the_end_of_the_track() {
    let grid = BeatGrid { track: 1, bpm: 120.0, first_beat: 0.25 };
    let beats = grid_beats(&grid, 2.0);
    assert_eq!(beats.len(), 4);
    assert_eq!(beats.iter().map(|b| b.beat_number).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(beats.iter().map(|b| b.time_ms).collect::<Vec<_>>(), [250, 750, 1250, 1750]);
    assert!(beats.iter().all(|b| b.tempo_x100 == 12_000));
    assert!(grid_beats(&BeatGrid { track: 1, bpm: 120.0, first_beat: 5.0 }, 2.0).is_empty());
}

#[test]
fn the_first_eight_cues_are_hot_cues_and_every_cue_is_a_memory_cue() {
    let points = (0..10).map(|i| Cue { id: i + 1, name: format!("Cue {i}"), seconds: f64::from(10 - i) }).collect();
    let catalog = Catalog { cues: vec![CueSet { track: 1, next_id: 11, points }], ..Catalog::default() };
    let cues = cues_of(&catalog, 1);
    let hot: Vec<_> = cues.iter().filter(|c| c.kind != 0).collect();
    assert_eq!(hot.iter().map(|c| c.kind).collect::<Vec<_>>(), HOT_CUE_KINDS);
    assert_eq!(hot[0].time_ms, 1000, "sorted by time: A is the earliest");
    assert_eq!(cues.iter().filter(|c| c.kind == 0).count(), 10);
    assert!(cues_of(&catalog, 2).is_empty());
}

#[test]
fn the_library_is_written_as_a_rekordbox_stick_and_a_second_export_reuses_it() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("My Track.wav");
    clicks(&audio);
    let mut catalog = Catalog::default();
    let id = catalog.import(&audio, None).unwrap();
    catalog.playlists.push(Playlist { id: 1, name: "Saturday".into(), tracks: vec![id] });
    catalog.cues.push(CueSet { track: id, next_id: 2, points: vec![Cue { id: 1, name: "Drop".into(), seconds: 1.0 }] });

    let stick = dir.path().join("stick");
    std::fs::create_dir(&stick).unwrap();
    let mut stages = Vec::new();
    let summary = export_usb(&catalog, &stick, &mut |p| stages.push(p.stage.clone())).unwrap();
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert_eq!((summary.tracks, summary.playlists), (1, 1));
    assert_eq!(summary.converted, 1, "a float WAV becomes a 16-bit WAV a CDJ plays");
    assert!(stages.first().is_some_and(|s| s == "analysing"));
    assert!(stick.join("PIONEER/rekordbox/export.pdb").is_file());
    let anlz: Vec<_> = walk(&stick.join("PIONEER/USBANLZ"));
    for ext in ["DAT", "EXT", "2EX"] {
        assert!(anlz.iter().any(|p| p.extension().is_some_and(|e| e == ext)), "missing ANLZ0000.{ext}");
    }
    assert!(walk(&stick.join("Contents")).iter().any(|p| p.extension().is_some_and(|e| e == "wav")));
    assert!(rbl_export::verify(&stick).is_ok());
    assert!(audio.is_file(), "the source is left where it was");

    let again = export_usb(&catalog, &stick, &mut |_| {}).unwrap();
    assert_eq!(again.tracks, 1);
    assert_eq!(again.copied, 0, "nothing changed, nothing is copied again");
}

#[test]
fn an_empty_library_or_a_missing_folder_is_refused_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(export_usb(&Catalog::default(), dir.path(), &mut |_| {}).is_err());
    let catalog = Catalog { tracks: vec![], ..Catalog::default() };
    assert!(export_usb(&catalog, &dir.path().join("nowhere"), &mut |_| {}).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() { out.extend(walk(&path)) } else { out.push(path) }
    }
    out
}
