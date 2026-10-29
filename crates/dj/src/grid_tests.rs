// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::*;
fn catalog() -> Catalog {
    Catalog {
        tracks: vec![Track {
            id: 1,
            path: std::env::temp_dir().join("grid.wav"),
            title: "Song".into(),
            source_project: None,
            seconds: 2.0,
            bpm: Some(121.0),
            key: None,
        }],
        ..Catalog::default()
    }
}
#[test]
fn grid_positions_follow_authored_impulses_not_the_analysis_bpm() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pulses.wav");
    let mut samples = vec![0.0; 32000];
    for frame in [2000, 6000, 10000, 14000] {
        samples[frame * 2] = 0.8;
        samples[frame * 2 + 1] = -0.6;
    }
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(&file, &samples, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    let audio = suhina_media::decode_file(&file).unwrap();
    let mut c = catalog();
    c.set_grid(0, 1, Some((120.0, 0.25))).unwrap();
    let grid = &c.grids[0];
    let lines = grid.lines(2.0, 100);
    assert_eq!(lines.iter().map(|l| l.seconds).collect::<Vec<_>>(), [0.25, 0.75, 1.25, 1.75]);
    assert_eq!(lines.iter().map(|l| l.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
    for line in lines {
        let frame = (line.seconds * 8000.0).round() as usize;
        assert_eq!(audio.samples[frame * 2], 0.8);
    }
    assert_eq!(c.tracks[0].bpm, Some(121.0));
    let grid = BeatGrid { track: 1, bpm: 120.0, first_beat: 0.0 };
    assert_eq!(grid.lines(4.0, 100)[4].seconds, 2.0);
    assert_eq!(grid.lines(4.0, 100)[4].index / 4 + 1, 2);
}
#[test]
fn invalid_stale_unknown_and_exhausted_grid_edits_are_atomic() {
    let mut c = catalog();
    c.set_grid(0, 1, Some((120.0, 0.0))).unwrap();
    for value in [
        Some((f64::NAN, 0.0)),
        Some((f64::INFINITY, 0.0)),
        Some((19.0, 0.0)),
        Some((401.0, 0.0)),
        Some((120.0, f64::NAN)),
        Some((120.0, -1.0)),
        Some((120.0, 2.0)),
    ] {
        let before = c.clone();
        assert!(c.set_grid(c.revision, 1, value).is_err());
        assert_eq!(c, before);
    }
    let before = c.clone();
    assert!(c.set_grid(0, 1, None).is_err());
    assert!(c.set_grid(c.revision, 99, None).is_err());
    assert_eq!(c, before);
    let mut duplicate = c.clone();
    duplicate.grids.push(duplicate.grids[0].clone());
    assert!(duplicate.validate().is_err());
    let mut unknown = c.clone();
    unknown.grids[0].track = 99;
    assert!(unknown.validate().is_err());
    c.revision = u64::MAX;
    let before = c.clone();
    assert!(c.set_grid(c.revision, 1, None).is_err());
    assert_eq!(c, before);
}
#[test]
fn grid_density_is_bounded_and_keeps_original_bar_numbers() {
    let grid = BeatGrid { track: 1, bpm: 400.0, first_beat: 0.0 };
    for budget in [0, 1, 2, 50, 2048, usize::MAX] {
        let lines = grid.lines(1200.0, budget);
        assert!(lines.len() <= budget.min(2048));
        assert!(lines.iter().all(|l| l.seconds < 1200.0));
        assert!(lines.windows(2).all(|p| p[0].index < p[1].index));
        if budget > 0 {
            assert_eq!(lines[0].index, 0);
            assert!(lines.iter().all(|l| l.index % 4 == 0));
        }
    }
    for duration in [0.0, -1.0, f64::INFINITY, f64::NAN, 1201.0] {
        assert!(grid.lines(duration, 100).is_empty());
    }
}
#[test]
fn format_three_migrates_read_only_then_backs_up_exactly_with_cues_and_playlists() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("dj-library.json");
    let mut c = catalog();
    c.edit_playlist(0, PlaylistEdit::Create("Set".into())).unwrap();
    c.edit_playlist(c.revision, PlaylistEdit::Add { playlist: 1, track: 1 }).unwrap();
    c.edit_cue(c.revision, 1, CueEdit::Add { name: "Intro".into(), seconds: 0.5 }).unwrap();
    let mut old = serde_json::to_value(&c).unwrap();
    old["format"] = serde_json::json!(3);
    old.as_object_mut().unwrap().remove("grids");
    let bytes = serde_json::to_vec_pretty(&old).unwrap();
    std::fs::write(&file, &bytes).unwrap();
    let before = Catalog::open(&file).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    let mut next = before.clone();
    next.set_grid(next.revision, 1, Some((120.0, 0.25))).unwrap();
    next.save_checked(&before, &file).unwrap();
    assert_eq!(Catalog::open(&file).unwrap(), next);
    assert_eq!(next.cues, before.cues);
    assert_eq!(next.playlists, before.playlists);
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("dj-library-v3-backup-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), bytes);
    let mut stale = before.clone();
    stale.set_grid(stale.revision, 1, Some((100.0, 0.0))).unwrap();
    assert!(stale.save_checked(&before, &file).is_err());
    assert_eq!(Catalog::open(&file).unwrap(), next);
    let original = next.clone();
    next.set_grid(next.revision, 1, None).unwrap();
    next.save_checked(&original, &file).unwrap();
    assert_eq!(Catalog::open(&file).unwrap().cues, before.cues);
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), bytes);
}
#[test]
fn grid_reimport_preserves_manual_values_and_refuses_shortening_past_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("song.wav");
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    let samples: Vec<_> = (0..16000).flat_map(|i| [(i as f32 * 0.2).sin() * 0.1; 2]).collect();
    suhina_media::write::write_audio_file(&file, &samples, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    let mut c = Catalog::default();
    c.import(&file, None).unwrap();
    c.set_grid(c.revision, 1, Some((123.0, 1.5))).unwrap();
    let grids = c.grids.clone();
    c.import(&file, None).unwrap();
    assert_eq!(c.grids, grids);
    suhina_media::write::write_audio_file(&file, &samples[..8000], &spec, suhina_media::write::WriteMode::Replace)
        .unwrap();
    let before = c.clone();
    assert!(c.import(&file, None).is_err());
    assert_eq!(c, before);
}
#[test]
fn malformed_or_future_grid_catalogs_are_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("catalog.json");
    let mut c = catalog();
    c.set_grid(0, 1, Some((120.0, -0.0))).unwrap();
    assert_eq!(c.grids[0].first_beat.to_bits(), 0);
    for bad in [
        serde_json::json!({"format":3,"tracks":[],"playlists":[],"revision":0,"next_playlist_id":1,"cues":[],"grids":[]}),
        serde_json::json!({"format":5,"tracks":[],"playlists":[],"revision":0,"next_playlist_id":1,"cues":[],"grids":[]}),
    ] {
        let bytes = serde_json::to_vec(&bad).unwrap();
        std::fs::write(&file, &bytes).unwrap();
        assert!(Catalog::open(&file).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }
}
