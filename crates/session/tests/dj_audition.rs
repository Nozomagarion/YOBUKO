// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::panic)]
use crate::{dj::idle, temp_dir::TempDir};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use suhina_engine::{AudioPool, DeviceAssets, Renderer};
use suhina_session::audio::{HostInfo, Opened};
use suhina_session::{AudioSystem, OutputChoice, Player, Projects, Session};

#[derive(Default)]
struct Output {
    renderers: Mutex<Vec<Renderer>>,
}
impl AudioSystem for Output {
    fn hosts(&self) -> Vec<HostInfo> {
        Vec::new()
    }
    fn open(
        &self,
        p: &suhina_model::Project,
        audio: &AudioPool,
        _: &DeviceAssets,
        _: &OutputChoice,
        previous: Option<Player>,
    ) -> Result<Opened, String> {
        drop(previous);
        let mut renderer = Renderer::stopped(p, audio, 8000).map_err(|e| e.to_string())?;
        let player = Player::new(renderer.controller(), None);
        self.renderers.lock().unwrap().push(renderer);
        Ok(Opened { player, notice: None, plugins_kept: true })
    }
}
fn seeded(dir: &std::path::Path) -> (Projects, Arc<Output>) {
    let path = dir.join("song.wav");
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    let audio: Vec<f32> = (0..16000).flat_map(|i| [if i < 8000 { 0.2 } else { 0.4 }; 2]).collect();
    suhina_media::write::write_audio_file(&path, &audio, &spec, suhina_media::write::WriteMode::Replace).unwrap();
    suhina_dj::Catalog {
        tracks: vec![suhina_dj::Track {
            id: 1,
            path,
            title: "Song".into(),
            source_project: None,
            seconds: 2.0,
            bpm: None,
            key: None,
        }],
        ..Default::default()
    }
    .save(&dir.join("dj-library.json"))
    .unwrap();
    let mut p = Projects::new();
    p.set_file_library(suhina_library::Library::new(Some(dir.to_owned())));
    let output = Arc::new(Output::default());
    p.set_audio_system(output.clone());
    idle(&mut p);
    (p, output)
}
fn render(output: &Output) -> Vec<f32> {
    let mut samples = vec![0.0; 1024];
    output.renderers.lock().unwrap().last_mut().unwrap().process(&mut samples);
    samples
}
fn cue(p: &mut Projects, cmd: &str, mut params: Value) -> Value {
    let state = idle(p);
    params["revision"] = state["revision"].clone();
    params["track"] = json!(1);
    p.execute(cmd, &params).unwrap();
    let state = idle(p);
    assert!(state["error"].is_null(), "{state}");
    state
}

#[test]
fn audition_uses_real_renderer_seeks_reports_position_and_does_not_persist() {
    let dir = TempDir::new("dj-audition");
    let (mut p, out) = seeded(&dir);
    let catalog = std::fs::read(dir.join("dj-library.json")).unwrap();
    let audio = std::fs::read(dir.join("song.wav")).unwrap();
    p.execute("dj.preview.stop", &json!({})).unwrap();
    assert!(out.renderers.lock().unwrap().is_empty());
    p.execute("dj.preview.play", &json!({"track":1,"from":1.0})).unwrap();
    idle(&mut p);
    let samples = render(&out);
    assert!((samples[600] - 0.4 * 0.501_187_2).abs() < 1e-4);
    let state = idle(&mut p);
    assert_eq!(state["preview"]["playing"], true);
    assert!((state["preview"]["position"].as_f64().unwrap() - 1.064).abs() < 0.001);
    p.execute("dj.preview.stop", &json!({})).unwrap();
    assert!(render(&out).iter().all(|s| *s == 0.0));
    p.execute("dj.preview.play", &json!({"track":1,"from":0.0})).unwrap();
    idle(&mut p);
    assert!((render(&out)[600] - 0.2 * 0.501_187_2).abs() < 1e-4);
    assert_eq!(std::fs::read(dir.join("dj-library.json")).unwrap(), catalog);
    assert_eq!(std::fs::read(dir.join("song.wav")).unwrap(), audio);
    assert!(p.is_empty());
}

#[test]
fn stop_during_loading_and_switching_projects_prevent_late_audio() {
    let dir = TempDir::new("dj-cancel-load");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.preview.play", &json!({"track":1,"from":0.0})).unwrap();
    p.execute("dj.preview.stop", &json!({})).unwrap();
    let state = idle(&mut p);
    assert_eq!(state["preview"]["loading"], false);
    assert!(render(&out).iter().all(|s| *s == 0.0));
    p.execute("dj.preview.play", &json!({"track":1,"from":0.5})).unwrap();
    let id = p.add(Session::new(suhina_session::blank_project()), None).unwrap();
    p.open_output(None).unwrap();
    let before = p.get(id).unwrap().project().clone();
    idle(&mut p);
    assert!(render(&out).iter().all(|s| *s == 0.0));
    assert_eq!(p.get(id).unwrap().project(), &before);
}

#[test]
fn folder_switch_cancels_pending_audio_and_cache_detects_file_changes() {
    let dir = TempDir::new("dj-cache-and-folder");
    let (mut p, out) = seeded(&dir);
    p.execute("dj.preview.play", &json!({"track":1,"from":0.0})).unwrap();
    idle(&mut p);
    render(&out);
    let spec = suhina_media::write::AudioFileSpec::float_wav(8000);
    suhina_media::write::write_audio_file(
        &dir.join("song.wav"),
        &vec![0.1; 8000],
        &spec,
        suhina_media::write::WriteMode::Replace,
    )
    .unwrap();
    p.execute("dj.preview.play", &json!({"track":1,"from":0.0})).unwrap();
    let result = idle(&mut p);
    assert!(result["error"].as_str().unwrap().contains("duration changed"));
    assert!(render(&out).iter().all(|s| *s == 0.0));
    let second = TempDir::new("dj-other-folder");
    p.set_file_library(suhina_library::Library::new(Some(second.to_path_buf())));
    assert_eq!(idle(&mut p)["tracks"], json!([]));
    assert!(render(&out).iter().all(|s| *s == 0.0));
}

#[test]
fn cue_commands_keep_track_identity_across_playlists_and_restart() {
    let dir = TempDir::new("dj-cue-commands");
    let (mut p, _) = seeded(&dir);
    let a = cue(&mut p, "dj.cue.add", json!({"name":"Intro","seconds":0.0}));
    assert_eq!(a["cues"][0]["points"][0]["id"], 1);
    cue(&mut p, "dj.cue.add", json!({"name":"Drop","seconds":1.5}));
    cue(&mut p, "dj.cue.rename", json!({"cue":2,"name":"Main drop"}));
    cue(&mut p, "dj.cue.move", json!({"cue":2,"seconds":1.25}));
    drop(p);
    let mut p = Projects::new();
    p.set_file_library(suhina_library::Library::new(Some(dir.to_path_buf())));
    let state = idle(&mut p);
    assert_eq!(state["cues"][0]["points"][1]["name"], "Main drop");
    assert_eq!(state["cues"][0]["points"][1]["seconds"], 1.25);
    p.execute("dj.cue.delete", &json!({"track":1,"cue":1,"revision":0})).unwrap();
    assert!(idle(&mut p)["error"].is_string());
    let state = cue(&mut p, "dj.cue.delete", json!({"cue":1}));
    assert_eq!(state["cues"][0]["points"].as_array().unwrap().len(), 1);
    for command in ["dj.preview.play", "dj.preview.stop", "dj.cue.add", "dj.cue.rename", "dj.cue.move", "dj.cue.delete"]
    {
        assert!(matches!(suhina_session::policy(command).unwrap().agents, suhina_session::Agents::Denied(_)));
    }
}

#[test]
fn missing_files_invalid_positions_and_missing_output_preserve_the_catalog() {
    let dir = TempDir::new("dj-audition-errors");
    let (mut p, _) = seeded(&dir);
    let before = std::fs::read(dir.join("dj-library.json")).unwrap();
    for params in [json!({"track":1,"from":-1.0}), json!({"track":1,"from":2.0}), json!({"track":99,"from":0.0})] {
        assert!(p.execute("dj.preview.play", &params).is_err());
    }
    std::fs::remove_file(dir.join("song.wav")).unwrap();
    p.execute("dj.preview.play", &json!({"track":1,"from":0.0})).unwrap();
    assert!(idle(&mut p)["error"].is_string());
    let mut no_output = Projects::new();
    no_output.set_file_library(suhina_library::Library::new(Some(dir.to_path_buf())));
    idle(&mut no_output);
    assert!(no_output.execute("dj.preview.play", &json!({"track":1,"from":0.0})).is_err());
    assert_eq!(std::fs::read(dir.join("dj-library.json")).unwrap(), before);
}
