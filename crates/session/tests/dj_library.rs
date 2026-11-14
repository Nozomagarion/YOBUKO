// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ library's music folders: scanned and analysed in the background, waited for with
//! `Projects::sync_dj_library` (an explicit signal, never a clock).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers
use crate::temp_dir::TempDir;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use suhina_session::{Agents, Projects};

fn projects(dir: &Path) -> Projects {
    let mut p = Projects::new();
    p.set_file_library(suhina_library::Library::new(Some(dir.to_owned())));
    p
}

fn settled(p: &mut Projects) -> Value {
    p.sync_dj_library().unwrap();
    let library = p.execute("dj.library", &json!({})).unwrap();
    assert!(library["error"].is_null(), "{library}");
    assert!(library["library"]["error"].is_null(), "{library}");
    library
}

fn wav(path: &Path, seconds: u32) {
    let rate = 44_100u32;
    let stereo: Vec<f32> = (0..rate * seconds)
        .flat_map(|i| {
            let s = (i as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin() * 0.3;
            [s, s]
        })
        .collect();
    suhina_media::write::write_audio_file(
        path,
        &stereo,
        &suhina_media::write::AudioFileSpec::float_wav(rate),
        suhina_media::write::WriteMode::Export { overwrite: false },
    )
    .unwrap();
}

fn m4a(path: &Path) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dj/tests/data/tone-aac.m4a");
    std::fs::copy(fixture, path).unwrap();
}

fn track<'a>(library: &'a Value, file: &str) -> &'a Value {
    library["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["path"].as_str().is_some_and(|p| p.ends_with(file)))
        .unwrap_or_else(|| panic!("{file} is not in {library}"))
}

/// A music folder with a WAV, an M4A in a subfolder, a hidden file and a text file.
fn music(dir: &Path) -> PathBuf {
    let music = dir.join("Music");
    std::fs::create_dir_all(music.join("House")).unwrap();
    wav(&music.join("intro.wav"), 2);
    m4a(&music.join("House/sea song.m4a"));
    wav(&music.join(".hidden.wav"), 1);
    std::fs::write(music.join("notes.txt"), b"not audio").unwrap();
    music.canonicalize().unwrap()
}

#[test]
fn a_music_folder_is_scanned_tagged_and_analysed_in_the_background() {
    let dir = TempDir::new("dj-music-folder");
    let music = music(&dir);
    let mut p = projects(&dir);
    p.execute("dj.folders.add", &json!({"folder": music})).unwrap();
    let library = settled(&mut p);

    assert_eq!(library["library"]["folders"], json!([music]));
    assert_eq!(library["tracks"].as_array().unwrap().len(), 2, "{library}");
    assert_eq!(library["library"]["lastScan"], json!({"looked": 2, "new": 2}));
    let song = track(&library, "sea song.m4a");
    assert_eq!(song["title"], "Sea Song");
    assert_eq!(song["artist"], "Nozo Band");
    assert_eq!(song["album"], "Tides");
    assert_eq!(song["genre"], "House");
    assert_eq!(song["year"], 2024);
    assert_eq!(song["analysed"], true);
    let intro = track(&library, "intro.wav");
    assert_eq!(intro["title"], "intro");
    assert_eq!(intro["analysed"], true);
    assert!((intro["seconds"].as_f64().unwrap() - 2.0).abs() < 0.05, "{intro}");
    assert_eq!(library["library"]["analysis"]["left"], 0);

    // The library and its folders are kept: a second app finds both.
    drop(p);
    let mut p = projects(&dir);
    let library = settled(&mut p);
    assert_eq!(library["library"]["folders"], json!([music]));
    assert_eq!(library["tracks"].as_array().unwrap().len(), 2);
}

#[test]
fn the_folders_are_scanned_again_when_the_library_opens_and_lost_files_are_marked_missing() {
    let dir = TempDir::new("dj-rescan");
    let music = music(&dir);
    let mut p = projects(&dir);
    p.execute("dj.folders.add", &json!({"folder": music})).unwrap();
    settled(&mut p);
    drop(p);

    // Between two sessions a file comes and one goes.
    wav(&music.join("House/new.wav"), 1);
    std::fs::remove_file(music.join("intro.wav")).unwrap();
    let mut p = projects(&dir);
    let library = settled(&mut p);
    assert_eq!(library["tracks"].as_array().unwrap().len(), 3, "{library}");
    assert_eq!(track(&library, "new.wav")["analysed"], true);
    assert_eq!(track(&library, "intro.wav")["missing"], true);
    assert_eq!(library["library"]["missing"], 1);

    // It comes back: a scan asked for by hand finds it again, with the same id.
    let id = track(&library, "intro.wav")["id"].clone();
    wav(&music.join("intro.wav"), 2);
    p.execute("dj.folders.scan", &json!({})).unwrap();
    let library = settled(&mut p);
    assert!(track(&library, "intro.wav")["missing"].is_null());
    assert_eq!(track(&library, "intro.wav")["id"], id);
    assert_eq!(library["library"]["missing"], 0);
}

#[test]
fn background_changes_do_not_make_the_shown_revision_stale_but_another_edit_does() {
    let dir = TempDir::new("dj-revision-rebase");
    let music = music(&dir);
    let mut p = projects(&dir);
    p.execute("dj.folders.add", &json!({"folder": music})).unwrap();
    let shown = settled(&mut p);
    let id = track(&shown, "intro.wav")["id"].clone();

    // The scan and analysis change the library after the page read it.
    wav(&music.join("later.wav"), 1);
    p.execute("dj.folders.scan", &json!({})).unwrap();
    let library = settled(&mut p);
    assert!(library["revision"].as_u64().unwrap() > shown["revision"].as_u64().unwrap());

    p.execute("dj.track.rate", &json!({"track": id, "revision": shown["revision"], "stars": 4})).unwrap();
    let library = settled(&mut p);
    assert_eq!(track(&library, "intro.wav")["rating"], 4);

    // The person's own edit since then does make it stale.
    p.execute("dj.playlist.create", &json!({"name": "Set", "revision": shown["revision"]})).unwrap();
    let library = settled_with_error(&mut p);
    assert!(library["error"].is_string(), "{library}");
    assert_eq!(library["playlists"], json!([]));
    assert!(p.execute("dj.track.rate", &json!({"track": id, "revision": library["revision"], "stars": 6})).is_err());
}

fn settled_with_error(p: &mut Projects) -> Value {
    p.sync_dj_library().unwrap();
    p.execute("dj.library", &json!({})).unwrap()
}

#[test]
fn dropped_files_come_in_through_the_background_scan_and_the_library_is_sent_only_when_it_changed() {
    let dir = TempDir::new("dj-dropped-files");
    let loose = dir.join("loose");
    std::fs::create_dir(&loose).unwrap();
    wav(&loose.join("one.wav"), 1);
    m4a(&loose.join("two.m4a"));
    std::fs::write(loose.join("three.txt"), b"text").unwrap();
    let mut p = projects(&dir);
    settled(&mut p);
    let paths: Vec<PathBuf> = ["one.wav", "two.m4a", "three.txt", "one.wav"].iter().map(|f| loose.join(f)).collect();
    p.execute("dj.files.add", &json!({"paths": paths})).unwrap();
    assert!(p.execute("dj.files.add", &json!({"paths": [loose.join("three.txt")]})).is_err());
    let library = settled(&mut p);
    assert_eq!(library["tracks"].as_array().unwrap().len(), 2, "{library}");
    assert_eq!(track(&library, "two.m4a")["artist"], "Nozo Band");
    // Dropped files are not a music folder.
    assert_eq!(library["library"]["folders"], json!([]));

    let again = p.execute("dj.library", &json!({"revision": library["revision"]})).unwrap();
    assert_eq!(again["unchanged"], true);
    assert!(again["tracks"].is_null());
    assert!(again["performance"].is_object());
    let older = p.execute("dj.library", &json!({"revision": 0})).unwrap();
    assert_eq!(older["tracks"].as_array().unwrap().len(), 2);
}

#[test]
fn music_folders_nest_into_one_and_can_be_removed_keeping_their_tracks() {
    let dir = TempDir::new("dj-folder-nesting");
    let music = music(&dir);
    let house = music.join("House");
    let mut p = projects(&dir);
    p.execute("dj.folders.add", &json!({"folder": house})).unwrap();
    assert_eq!(settled(&mut p)["tracks"].as_array().unwrap().len(), 1);
    // A folder inside a music folder is already there.
    assert!(p.execute("dj.folders.add", &json!({"folder": house})).is_err());
    // A folder that holds a music folder takes its place.
    p.execute("dj.folders.add", &json!({"folder": music})).unwrap();
    let library = settled(&mut p);
    assert_eq!(library["library"]["folders"], json!([music]));
    assert_eq!(library["tracks"].as_array().unwrap().len(), 2);
    assert!(p.execute("dj.folders.add", &json!({"folder": dir.join("nowhere")})).is_err());
    assert!(p.execute("dj.folders.remove", &json!({"folder": house})).is_err());
    p.execute("dj.folders.remove", &json!({"folder": music})).unwrap();
    let library = settled(&mut p);
    assert_eq!(library["library"]["folders"], json!([]));
    assert_eq!(library["tracks"].as_array().unwrap().len(), 2);
    assert!(p.execute("dj.folders.scan", &json!({})).is_err());
}

#[test]
fn paused_analysis_leaves_new_tracks_unanalysed_until_it_resumes() {
    let dir = TempDir::new("dj-analysis-pause");
    let music = music(&dir);
    let mut p = projects(&dir);
    settled(&mut p);
    p.execute("dj.analysis.set", &json!({"on": false})).unwrap();
    p.execute("dj.folders.add", &json!({"folder": music})).unwrap();
    let library = settled(&mut p);
    assert_eq!(library["library"]["analysis"]["on"], false);
    assert_eq!(library["library"]["analysis"]["left"], 2, "{library}");
    assert!(track(&library, "intro.wav")["analysed"].is_null());
    p.execute("dj.analysis.set", &json!({"on": true})).unwrap();
    let library = settled(&mut p);
    assert_eq!(library["library"]["analysis"]["left"], 0);
    assert_eq!(track(&library, "intro.wav")["analysed"], true);
}

#[test]
fn the_library_commands_are_the_persons_only() {
    for command in
        ["dj.folders.add", "dj.folders.remove", "dj.folders.scan", "dj.files.add", "dj.analysis.set", "dj.track.rate"]
    {
        assert!(matches!(suhina_session::policy(command).unwrap().agents, Agents::Denied(_)), "{command}");
    }
}
