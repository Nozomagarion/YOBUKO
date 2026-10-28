// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::panic)] // test fixtures
use super::*;
use PlaylistEdit::*;

fn catalog() -> Catalog {
    Catalog {
        tracks: (1..=3)
            .map(|id| Track {
                id,
                path: std::env::temp_dir().join(format!("{id}.wav")),
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
fn apply(c: &mut Catalog, edit: PlaylistEdit) -> Option<u32> {
    c.edit_playlist(c.revision, edit).unwrap()
}

#[test]
fn ordered_playlists_round_trip_and_ids_are_not_reused() {
    let mut c = catalog();
    let id = apply(&mut c, Create("  Warm up  ".into())).unwrap();
    for track in 1..=3 {
        apply(&mut c, Add { playlist: id, track });
    }
    apply(&mut c, Move { playlist: id, track: 3, position: 1 });
    assert_eq!(c.playlists[0].tracks, [3, 1, 2]);
    apply(&mut c, Move { playlist: id, track: 3, position: 3 });
    apply(&mut c, Remove { playlist: id, track: 2 });
    apply(&mut c, Rename { playlist: id, name: "Live set".into() });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj.json");
    c.save(&path).unwrap();
    assert_eq!(Catalog::open(&path).unwrap(), c);
    assert_eq!(c.playlists[0].tracks, [1, 3]);
    apply(&mut c, Delete(id));
    assert!(apply(&mut c, Create("Next".into())).unwrap() > id);
    assert_eq!(c.tracks.len(), 3);
}

#[test]
fn invalid_or_stale_edits_never_mutate_the_catalog() {
    let mut c = catalog();
    let id = apply(&mut c, Create("Set".into())).unwrap();
    apply(&mut c, Add { playlist: id, track: 1 });
    for edit in [
        Create(" ".into()),
        Create("x".repeat(129)),
        Rename { playlist: id, name: "a\nb".into() },
        Add { playlist: id, track: 1 },
        Add { playlist: id, track: 99 },
        Remove { playlist: id, track: 99 },
        Delete(99),
        Move { playlist: id, track: 1, position: 0 },
        Move { playlist: id, track: 1, position: 2 },
        Move { playlist: id, track: 99, position: 1 },
    ] {
        let before = c.clone();
        assert!(c.edit_playlist(c.revision, edit).is_err());
        assert_eq!(c, before);
    }
    let before = c.clone();
    assert!(c.edit_playlist(0, Delete(id)).is_err());
    assert_eq!(c, before);
    c.revision = u64::MAX;
    let before = c.clone();
    assert!(c.edit_playlist(u64::MAX, Delete(id)).is_err());
    assert_eq!(c, before);
}

#[test]
fn playlist_references_counters_and_limits_are_validated() {
    let mut c = catalog();
    let id = apply(&mut c, Create("Set".into())).unwrap();
    apply(&mut c, Add { playlist: id, track: 1 });
    let mut bad = c.clone();
    bad.playlists[0].tracks.push(1);
    assert!(bad.validate().is_err());
    let mut bad = c.clone();
    bad.playlists[0].tracks.push(999);
    assert!(bad.validate().is_err());
    for next in [0, u64::from(id), u64::from(u32::MAX) + 2] {
        let mut bad = c.clone();
        bad.next_playlist_id = next;
        assert!(bad.validate().is_err());
    }
    c.next_playlist_id = u64::from(u32::MAX) + 1;
    let before = c.clone();
    assert!(c.edit_playlist(c.revision, Create("Full".into())).is_err());
    assert_eq!(c, before);
    let mut c = catalog();
    for _ in 0..MAX_PLAYLISTS {
        apply(&mut c, Create("Set".into()));
    }
    let before = c.clone();
    assert!(c.edit_playlist(c.revision, Create("Full".into())).is_err());
    assert_eq!(c, before);
    let mut c = catalog();
    c.tracks = (1..=2000)
        .map(|id| Track { id, path: std::env::temp_dir().join(format!("{id}.wav")), ..c.tracks[0].clone() })
        .collect();
    for _ in 0..8 {
        let id = apply(&mut c, Create("Set".into())).unwrap();
        c.playlists.iter_mut().find(|p| p.id == id).unwrap().tracks = (1..=2000).collect();
    }
    c.validate().unwrap();
    let id = apply(&mut c, Create("Overflow".into())).unwrap();
    let before = c.clone();
    assert!(c.edit_playlist(c.revision, Add { playlist: id, track: 1 }).is_err());
    assert_eq!(c, before);
}

#[test]
fn legacy_open_is_read_only_and_first_commit_keeps_exact_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj-library.json");
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({"format":1,"tracks":catalog().tracks})).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let old = Catalog::open(&path).unwrap();
    assert_eq!(old.format, FORMAT);
    assert_eq!(old.revision, 0);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut next = old.clone();
    apply(&mut next, Create("Migrated".into()));
    next.save_checked(&old, &path).unwrap();
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|p| p.file_name().to_string_lossy().starts_with("dj-library-v1-backup-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), bytes);
    assert_eq!(Catalog::open(&path).unwrap(), next);
    let old = next.clone();
    apply(&mut next, Delete(1));
    next.save_checked(&old, &path).unwrap();
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), bytes);
}

#[test]
fn checked_writes_refuse_external_changes_locks_and_invalid_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj.json");
    let old = catalog();
    old.save(&path).unwrap();
    let mut next = old.clone();
    apply(&mut next, Create("A".into()));
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(next.save_checked(&old, &path).is_err());
    drop(lock);
    next.save_checked(&old, &path).unwrap();
    let mut stale = old.clone();
    apply(&mut stale, Create("B".into()));
    assert!(stale.save_checked(&old, &path).is_err());
    assert_eq!(Catalog::open(&path).unwrap(), next);
    std::fs::write(&path, b"broken").unwrap();
    assert!(stale.save_checked(&old, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
}

#[test]
fn competing_instances_publish_exactly_one_whole_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj.json");
    let old = catalog();
    old.save(&path).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["A", "B"]
        .into_iter()
        .map(|name| {
            let old = old.clone();
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut next = old.clone();
                apply(&mut next, Create(name.into()));
                barrier.wait();
                next.save_checked(&old, &path)
            })
        })
        .collect();
    let successes = handles.into_iter().map(|h| h.join().unwrap().is_ok()).filter(|ok| *ok).count();
    assert_eq!(successes, 1);
    let saved = Catalog::open(&path).unwrap();
    assert_eq!(saved.playlists.len(), 1);
    assert_eq!(saved.revision, 1);
}

#[test]
fn reanalysis_keeps_the_playlist_track_reference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    let audio: Vec<f32> = (0..16000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 8000.0).sin() * 0.2).collect();
    suhina_media::write::write_audio_file(&path, &audio, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut c = Catalog::default();
    let track = c.import(&path, None).unwrap();
    let id = apply(&mut c, Create("Set".into())).unwrap();
    apply(&mut c, Add { playlist: id, track });
    assert_eq!(c.import(&path, None).unwrap(), track);
    assert_eq!(c.playlists[0].tracks, [track]);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
