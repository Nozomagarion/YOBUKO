// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use CueEdit::*;
fn catalog() -> Catalog {
    Catalog {
        tracks: (1..=2)
            .map(|id| Track {
                id,
                path: std::env::temp_dir().join(format!("cue-{id}.wav")),
                title: format!("Track {id}"),
                source_project: None,
                seconds: 2.0,
                bpm: None,
                key: None,
            })
            .collect(),
        ..Catalog::default()
    }
}
fn apply(c: &mut Catalog, edit: CueEdit) {
    c.edit_cue(c.revision, 1, edit).unwrap();
}

#[test]
fn cues_sort_round_trip_and_keep_ids_when_another_is_removed() {
    let mut c = catalog();
    c.edit_playlist(0, PlaylistEdit::Create("Set".into())).unwrap();
    c.edit_playlist(c.revision, PlaylistEdit::Add { playlist: 1, track: 1 }).unwrap();
    apply(&mut c, Add { name: "Drop".into(), seconds: 1.5 });
    apply(&mut c, Add { name: " Intro ".into(), seconds: 0.0 });
    assert_eq!(c.cues[0].points.iter().map(|c| c.id).collect::<Vec<_>>(), [2, 1]);
    apply(&mut c, Rename { cue: 1, name: "Main drop".into() });
    apply(&mut c, Move { cue: 1, seconds: 0.25 });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.json");
    c.save(&path).unwrap();
    assert_eq!(Catalog::open(&path).unwrap(), c);
    assert_eq!(c.playlists[0].tracks, [1]);
    apply(&mut c, Delete(1));
    apply(&mut c, Add { name: "Outro".into(), seconds: 1.8 });
    assert_eq!(c.cues[0].points[1].id, 3);
    assert_eq!(c.tracks.len(), 2);
}

#[test]
fn invalid_cue_edits_and_stale_revisions_are_atomic() {
    let mut c = catalog();
    apply(&mut c, Add { name: "Cue".into(), seconds: 0.5 });
    for edit in [
        Add { name: " ".into(), seconds: 0.0 },
        Add { name: "x".repeat(65), seconds: 0.0 },
        Add { name: "x\n".into(), seconds: 0.0 },
        Add { name: "X".into(), seconds: f64::NAN },
        Add { name: "X".into(), seconds: f64::INFINITY },
        Move { cue: 1, seconds: -1.0 },
        Move { cue: 1, seconds: 2.0 },
        Move { cue: 99, seconds: 0.0 },
        Rename { cue: 99, name: "X".into() },
        Delete(99),
    ] {
        let before = c.clone();
        assert!(c.edit_cue(c.revision, 1, edit).is_err());
        assert_eq!(c, before);
    }
    let before = c.clone();
    assert!(c.edit_cue(0, 1, Delete(1)).is_err());
    assert_eq!(c, before);
    assert!(c.edit_cue(c.revision, 99, Delete(1)).is_err());
    assert_eq!(c, before);
    c.revision = u64::MAX;
    let before = c.clone();
    assert!(c.edit_cue(c.revision, 1, Delete(1)).is_err());
    assert_eq!(c, before);
}

#[test]
fn cue_references_counters_order_and_limits_are_validated() {
    let mut c = catalog();
    for i in 0..16 {
        apply(&mut c, Add { name: format!("Cue {i}"), seconds: f64::from(i) / 10.0 });
    }
    let before = c.clone();
    assert!(c.edit_cue(c.revision, 1, Add { name: "Full".into(), seconds: 0.0 }).is_err());
    assert_eq!(c, before);
    for change in 0..6 {
        let mut bad = c.clone();
        match change {
            0 => bad.cues[0].track = 99,
            1 => bad.cues[0].next_id = 1,
            2 => bad.cues[0].points[1].id = 1,
            3 => bad.cues[0].points.swap(0, 1),
            4 => bad.cues.push(bad.cues[0].clone()),
            _ => bad.cues[0].points[0].name = "X".repeat(65),
        };
        assert!(bad.validate().is_err());
    }
    let mut c = catalog();
    c.cues.push(CueSet { track: 1, next_id: u64::from(u32::MAX) + 1, points: Vec::new() });
    let before = c.clone();
    assert!(c.edit_cue(c.revision, 1, Add { name: "Full".into(), seconds: 0.0 }).is_err());
    assert_eq!(c, before);
    let template = c.tracks[0].clone();
    c.tracks = (1..=501)
        .map(|id| Track { id, path: std::env::temp_dir().join(format!("global-cue-{id}.wav")), ..template.clone() })
        .collect();
    c.cues = (1..=500)
        .map(|track| CueSet {
            track,
            next_id: 17,
            points: (1..=16).map(|id| Cue { id, name: format!("Cue {id}"), seconds: f64::from(id) / 10.0 }).collect(),
        })
        .collect();
    c.validate().unwrap();
    let before = c.clone();
    assert!(c.edit_cue(c.revision, 501, Add { name: "Full".into(), seconds: 0.0 }).is_err());
    assert_eq!(c, before);
}

#[test]
fn format_two_migration_preserves_playlist_ids_revision_and_exact_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj-library.json");
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({"format":2,"tracks":catalog().tracks,
        "playlists":[{"id":7,"name":"Set","tracks":[2,1]}],"revision":9,"next_playlist_id":12}))
    .unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let old = Catalog::open(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(old.revision, 9);
    assert_eq!(old.next_playlist_id, 12);
    let mut next = old.clone();
    apply(&mut next, Add { name: "Intro".into(), seconds: 0.0 });
    next.save_checked(&old, &path).unwrap();
    assert_eq!(Catalog::open(&path).unwrap(), next);
    assert_eq!(next.playlists[0].tracks, [2, 1]);
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("dj-library-v2-backup-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), bytes);
}

#[test]
fn reimport_keeps_cues_and_refuses_shortening_audio_past_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cue-tone.wav");
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    let stereo: Vec<f32> = (0..16000).flat_map(|i| [(i as f32 * 0.1).sin() * 0.2; 2]).collect();
    suhina_media::write::write_audio_file(&path, &stereo, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    let mut c = Catalog::default();
    c.import(&path, None).unwrap();
    apply(&mut c, Add { name: "Drop".into(), seconds: 1.5 });
    let points = c.cues.clone();
    c.import(&path, None).unwrap();
    assert_eq!(c.cues, points);
    suhina_media::write::write_audio_file(&path, &stereo[..8000], &spec, suhina_media::write::WriteMode::Replace)
        .unwrap();
    let before = c.clone();
    assert!(c.import(&path, None).is_err());
    assert_eq!(c, before);
}

#[test]
fn signed_zero_cue_positions_are_canonical_and_stably_ordered() {
    let mut c = catalog();
    apply(&mut c, Add { name: "First".into(), seconds: 0.0 });
    apply(&mut c, Add { name: "Second".into(), seconds: -0.0 });
    assert_eq!(c.cues[0].points.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 2]);
    assert!(c.cues[0].points.iter().all(|c| c.seconds.to_bits() == 0));
}
