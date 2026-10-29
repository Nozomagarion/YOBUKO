// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::{dj::idle, dj_audition::seeded, temp_dir::TempDir};
use serde_json::json;

#[test]
fn waveform_preparation_uses_stereo_summaries_without_output_or_catalog_writes() {
    let dir = TempDir::new("dj-waveform");
    let (mut p, out) = seeded(&dir);
    let catalog = std::fs::read(dir.join("dj-library.json")).unwrap();
    let audio = std::fs::read(dir.join("song.wav")).unwrap();
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    let state = idle(&mut p);
    assert!(state["waveform"]["error"].is_null(), "{state}");
    assert_eq!(state["waveform"]["track"], 1);
    assert_eq!(state["waveform"]["frames"], 16000);
    let peaks = p.dj_waveform(1).unwrap();
    assert_eq!(peaks.lanes, 2);
    assert!(!peaks.has_samples(), "the displayed summary must not retain another decoded file");
    for (from, to, level) in [(0.1, 0.2, 0.2), (1.4, 1.5, 0.4)] {
        let (low, high) = peaks.range(from, to);
        assert!((low - level).abs() < 1e-5 && (high - level).abs() < 1e-5);
    }
    assert!(p.dj_waveform(2).is_none());
    assert!(out.renderers.lock().unwrap().is_empty());
    assert!(p.is_empty());
    assert_eq!(std::fs::read(dir.join("dj-library.json")).unwrap(), catalog);
    assert_eq!(std::fs::read(dir.join("song.wav")).unwrap(), audio);
    for command in ["dj.waveform.load", "dj.waveform.clear"] {
        assert!(matches!(suhina_session::policy(command).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}

#[test]
fn clearing_loading_waveform_discards_late_results_and_reload_drops_it() {
    let dir = TempDir::new("dj-waveform-cancel");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    p.execute("dj.waveform.clear", &json!({})).unwrap();
    assert!(idle(&mut p)["waveform"]["track"].is_null());
    assert!(p.dj_waveform(1).is_none());
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    idle(&mut p);
    assert!(p.dj_waveform(1).is_some());
    p.execute("dj.reload", &json!({})).unwrap();
    idle(&mut p);
    assert!(p.dj_waveform(1).is_none());
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    p.set_file_library(suhina_library::Library::new(Some(TempDir::new("dj-waveform-folder").to_path_buf())));
    assert!(p.dj_waveform(1).is_none());
    assert!(out.renderers.lock().unwrap().is_empty());
}

#[test]
fn waveform_errors_are_visible_and_preparation_can_retry_after_a_missing_file() {
    let dir = TempDir::new("dj-waveform-errors");
    let (mut p, out) = seeded(&dir);
    assert!(p.execute("dj.waveform.load", &json!({"track":99})).is_err());
    let path = dir.join("song.wav");
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    assert!(idle(&mut p)["waveform"]["error"].is_string());
    assert!(p.dj_waveform(1).is_none());
    std::fs::write(&path, bytes).unwrap();
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    assert!(idle(&mut p)["waveform"]["error"].is_null());
    assert!(p.dj_waveform(1).is_some());
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(&path, &vec![0.1; 8000], &spec, suhina_media::write::WriteMode::Replace)
        .unwrap();
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    assert!(idle(&mut p)["waveform"]["error"].as_str().unwrap().contains("duration changed"));
    assert!(p.dj_waveform(1).is_none());
    assert!(out.renderers.lock().unwrap().is_empty());
}

#[test]
fn waveform_keeps_independent_channel_impulses_at_known_frame_positions() {
    let dir = TempDir::new("dj-waveform-impulses");
    let (mut p, out) = seeded(&dir);
    let mut audio = vec![0.0; 32000];
    audio[4000] = 0.8; // left at frame 2000 (0.25 s)
    audio[28001] = -0.6; // right at frame 14000 (1.75 s)
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(
        &dir.join("song.wav"),
        &audio,
        &spec,
        suhina_media::write::WriteMode::Replace,
    )
    .unwrap();
    p.execute("dj.waveform.load", &json!({"track":1})).unwrap();
    idle(&mut p);
    let peaks = p.dj_waveform(1).unwrap();
    use suhina_media::waveform::Lane;
    assert_eq!(peaks.column(Lane::Channel(0), 1900.0, 2100.0).unwrap().max, 0.8);
    assert_eq!(peaks.column(Lane::Channel(1), 1900.0, 2100.0).unwrap().max, 0.0);
    assert_eq!(peaks.column(Lane::Channel(1), 13900.0, 14100.0).unwrap().min, -0.6);
    assert_eq!(peaks.column(Lane::Channel(0), 13900.0, 14100.0).unwrap().min, 0.0);
    assert!(out.renderers.lock().unwrap().is_empty());
}
