// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use crate::scan::{self, Analysed};
use crate::{Catalog, Track};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// One second of a 440 Hz tone in AAC, in an M4A file with tags (made with ffmpeg's sine
/// source and AAC encoder).
fn aac() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/tone-aac.m4a")
}

fn wav(path: &Path, seconds: f32) {
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    let frames = (8000.0 * seconds) as usize;
    let audio: Vec<f32> = (0..frames).flat_map(|i| [0.2 * (i as f32 * 0.07).sin(); 2]).collect();
    suhina_media::write::write_audio_file(path, &audio, &spec, suhina_media::write::WriteMode::Replace).unwrap();
}

#[test]
fn m4a_and_aac_are_taken_with_the_other_formats_by_their_extension() {
    for name in ["a.m4a", "b.M4A", "c.aac", "d.mp4", "e.mp3", "f.flac", "g.wav", "h.aiff", "i.ogg", "j.alac"] {
        assert!(scan::is_dj_audio(Path::new(name)), "{name}");
    }
    for name in ["a.txt", "b.mid", "c", "d.m4a.part", "e.jpg"] {
        assert!(!scan::is_dj_audio(Path::new(name)), "{name}");
    }
}

#[test]
fn an_m4a_file_gives_its_tags_and_length_and_decodes_for_the_analysis() {
    let found = scan::read(&aac()).unwrap();
    assert_eq!(
        (found.title.as_str(), found.artist.as_str(), found.album.as_str(), found.genre.as_str()),
        ("Sea Song", "Nozo Band", "Tides", "House")
    );
    assert_eq!(found.year, Some(2024));
    assert_eq!(found.comment, "warm-up");
    assert!((found.seconds - 1.0).abs() < 0.1, "{}", found.seconds);
    let audio = suhina_media::decode_file(&aac()).unwrap();
    assert_eq!((audio.channels, audio.sample_rate), (2, 44100));
    let peak = audio.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1, "the tone is heard: {peak}");
    let analysed = scan::analyse(&aac(), found.seconds).unwrap();
    assert!((analysed.seconds - 1.0).abs() < 0.1);
    assert!(scan::analyse(&aac(), crate::MAX_ANALYSED_SECONDS + 1.0).is_err());
    // A file without tags is named after itself.
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("Night Run.wav");
    wav(&plain, 0.5);
    let found = scan::read(&plain).unwrap();
    assert_eq!((found.title.as_str(), found.artist.as_str(), found.year), ("Night Run", "", None));
    assert!(scan::read(&dir.path().join("none.wav")).is_err());
    std::fs::write(dir.path().join("broken.m4a"), b"not audio").unwrap();
    assert!(scan::read(&dir.path().join("broken.m4a")).is_err());
}

#[test]
fn a_walk_finds_the_audio_in_every_folder_but_hidden_ones_and_can_stop() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("House/Deep")).unwrap();
    std::fs::create_dir_all(root.join(".cache")).unwrap();
    for file in ["a.wav", "House/b.m4a", "House/Deep/c.mp3", ".cache/d.wav", "House/cover.jpg", "notes.txt"] {
        std::fs::write(root.join(file), b"x").unwrap();
    }
    let stop = AtomicBool::new(false);
    let mut seen = Vec::new();
    scan::walk(root, &stop, &mut |p| {
        seen.push(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
        true
    });
    seen.sort();
    assert_eq!(seen, ["House/Deep/c.mp3", "House/b.m4a", "a.wav"]);
    let mut count = 0;
    scan::walk(root, &stop, &mut |_| {
        count += 1;
        false
    });
    assert_eq!(count, 1, "visit says stop");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut none = 0;
    scan::walk(root, &stop, &mut |_| {
        none += 1;
        true
    });
    assert_eq!(none, 0);
    scan::walk(&root.join("gone"), &AtomicBool::new(false), &mut |_| panic!("nothing to find"));
}

#[test]
fn found_files_come_in_once_lost_ones_are_marked_and_analysis_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().canonicalize().unwrap();
    let mut catalog = Catalog::default();
    catalog.set_folders(vec![folder.clone()]).unwrap();
    assert!(catalog.set_folders(vec![PathBuf::from("relative")]).is_err());
    let a = folder.join("a.wav");
    let b = folder.join("b.wav");
    wav(&a, 0.5);
    wav(&b, 0.5);
    let found: Vec<_> = [&a, &b].iter().map(|p| scan::read(p).unwrap()).collect();
    assert_eq!(catalog.add_found(found.clone()).unwrap(), 2);
    assert_eq!(catalog.add_found(found).unwrap(), 0, "a second scan adds nothing");
    let ids: Vec<u32> = catalog.tracks.iter().map(|t| t.id).collect();
    assert_eq!(ids, [1, 2]);
    assert!(catalog.tracks.iter().all(|t| t.added.is_some() && t.needs_analysis()));
    // b's file goes: it is kept, marked missing, and comes back when it is found again.
    std::fs::remove_file(&b).unwrap();
    let seen: HashSet<PathBuf> = [a.clone()].into();
    assert_eq!(catalog.mark_missing(&seen).unwrap(), 1);
    assert!(catalog.tracks[1].missing && !catalog.tracks[0].missing);
    assert!(!catalog.tracks[1].needs_analysis());
    wav(&b, 0.5);
    catalog.add_found(vec![scan::read(&b).unwrap()]).unwrap();
    assert!(!catalog.tracks[1].missing);
    // Analysis keeps a tempo from the tags and fills what is missing.
    catalog.tracks[0].bpm = Some(124.0);
    catalog
        .set_analysis(vec![
            (1, Ok(Analysed { bpm: Some(120.0), key: Some("8A".into()), seconds: 0.5 })),
            (2, Err("unreadable".into())),
        ])
        .unwrap();
    assert_eq!((catalog.tracks[0].bpm, catalog.tracks[0].key.as_deref()), (Some(124.0), Some("8A")));
    assert!(catalog.tracks.iter().all(|t| t.analysed && !t.needs_analysis()));
    let revision = catalog.revision;
    catalog.set_rating(revision, 2, 4).unwrap();
    assert_eq!(catalog.tracks[1].rating, 4);
    assert!(catalog.set_rating(catalog.revision, 2, 6).is_err());
    assert!(catalog.set_rating(revision, 2, 3).is_err(), "a stale revision");
    // The library and its new fields read back; an old library without them still reads.
    let path = folder.join("dj-library.json");
    catalog.save(&path).unwrap();
    assert_eq!(Catalog::open(&path).unwrap(), catalog);
    let old = Catalog {
        tracks: vec![Track { id: 1, path: a, title: "a".into(), seconds: 1.0, ..Track::default() }],
        ..Catalog::default()
    };
    let text = serde_json::to_string(&old).unwrap();
    for field in ["artist", "album", "genre", "rating", "missing", "analysed", "folders"] {
        assert!(!text.contains(field), "{field} is left out when empty");
    }
}

#[test]
fn a_large_library_fits() {
    let mut catalog = Catalog::default();
    let found: Vec<_> = (0..20_000)
        .map(|i| scan::Found {
            path: PathBuf::from(format!("{}/music/track {i}.m4a", if cfg!(windows) { "C:" } else { "" })),
            title: format!("Track {i}"),
            artist: "Someone".into(),
            seconds: 240.0,
            ..scan::Found::default()
        })
        .collect();
    assert_eq!(catalog.add_found(found).unwrap(), 20_000);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj-library.json");
    catalog.save(&path).unwrap();
    assert_eq!(Catalog::open(&path).unwrap().tracks.len(), 20_000);
}
