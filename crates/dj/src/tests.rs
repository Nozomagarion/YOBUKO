// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn track() -> Track {
    Track {
        id: 1,
        path: std::env::temp_dir().join("test.wav"),
        title: "Test".into(),
        source_project: None,
        seconds: 1.0,
        bpm: Some(120.0),
        key: None,
    }
}

#[test]
fn a_catalog_round_trips_and_a_failed_write_preserves_the_previous_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj.json");
    let original = Catalog { tracks: vec![track()], ..Catalog::default() };
    original.save(&path).unwrap();
    assert_eq!(Catalog::open(&path).unwrap(), original);
    let mut invalid = original.clone();
    invalid.tracks[0].seconds = f64::NAN;
    assert!(invalid.save(&path).is_err());
    assert_eq!(Catalog::open(&path).unwrap(), original);
}

#[test]
fn invalid_versions_duplicate_ids_paths_and_oversized_catalogs_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dj.json");
    for text in [r#"{"format":2,"tracks":[]}"#, r#"{"format":1,"tracks":[],"extra":true}"#, "broken"] {
        std::fs::write(&path, text).unwrap();
        assert!(Catalog::open(&path).is_err());
    }
    let bad = Catalog { tracks: vec![track(), track()], ..Catalog::default() };
    assert!(bad.validate().is_err());
    std::fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
    assert!(Catalog::open(&path).is_err());
}

#[test]
fn real_rbxport_analysis_imports_a_generated_file_and_reimport_keeps_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    let rate = 8000u32;
    let samples: Vec<f32> =
        (0..rate * 2).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.2).collect();
    let stereo: Vec<f32> = samples.iter().flat_map(|s| [*s, *s]).collect();
    suhina_media::write::write_audio_file(
        &path,
        &stereo,
        &suhina_media::write::AudioFileSpec::float_wav(rate),
        suhina_media::write::WriteMode::Export { overwrite: false },
    )
    .unwrap();
    let mut catalog = Catalog::default();
    let project = dir.path().join("source.suhina");
    let id = catalog.import(&path, Some(project.clone())).unwrap();
    assert_eq!(catalog.import(&path, None).unwrap(), id);
    assert_eq!(catalog.tracks.len(), 1);
    assert_eq!(catalog.tracks[0].source_project, Some(project));
    assert!((catalog.tracks[0].seconds - 2.0).abs() < 0.01);
    let before = catalog.clone();
    assert!(catalog.import(&dir.path().join("missing.wav"), None).is_err());
    assert_eq!(catalog, before);
}
