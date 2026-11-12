// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::*;
use egui_kittest::kittest::Queryable;

/// Two seconds of a quiet tone at 44.1 kHz.
fn song(path: &Path) {
    let spec = suhina_media::write::AudioFileSpec::float_wav(44_100);
    let audio: Vec<f32> = (0..88_200).flat_map(|i| [0.2 * (i as f32 * 0.05).sin(); 2]).collect();
    suhina_media::write::write_audio_file(path, &audio, &spec, suhina_media::write::WriteMode::Replace).unwrap();
}

/// A window on the DJ space, its DJ library in `dir` holding `tracks` files.
fn dj_window(dir: &Path, tracks: &[&str]) -> egui_kittest::Harness<'static, App> {
    dj_window_sized(dir, tracks, 800.0)
}

/// The same, `height` points tall: a test that clicks the library's side bar gives it room, so
/// what it clicks is on screen whatever the platform's text measures.
fn dj_window_sized(dir: &Path, tracks: &[&str], height: f32) -> egui_kittest::Harness<'static, App> {
    let catalog = suhina_dj::Catalog {
        tracks: tracks
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let path = dir.join(format!("{name}.wav"));
                song(&path);
                suhina_dj::Track {
                    id: i as u32 + 1,
                    path: path.canonicalize().unwrap(),
                    title: (*name).into(),
                    source_project: None,
                    seconds: 2.0,
                    bpm: Some(120.0),
                    key: Some(["8A", "10A", "11B"][i % 3].into()),
                }
            })
            .collect(),
        ..Default::default()
    };
    catalog.save(&dir.join("dj-library.json")).unwrap();
    let mut app = App::new();
    app.set_file_library(suhina_library::Library::new(Some(dir.to_path_buf())));
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, height))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    super::super::menu_bar::pick_mode(&mut harness, "DJ");
    harness
}

/// Steps the window until `done` holds, for up to a minute (the library works on a thread).
fn until(harness: &mut egui_kittest::Harness<'static, App>, what: &str, done: impl Fn(&App) -> bool) {
    let end = Instant::now() + Duration::from_secs(60);
    while !done(harness.state()) {
        assert!(
            Instant::now() < end,
            "timed out waiting for {what}: {:?} {}",
            harness.state().dj.decks.error,
            harness.state().dj.decks.snapshot
        );
        std::thread::sleep(Duration::from_millis(20));
        harness.step();
    }
    harness.step();
}

#[test]
fn the_dj_space_mixes_in_the_window_with_two_decks_a_mixer_and_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 2));
    for label in [
        "Play deck A",
        "Play deck B",
        "Cue A",
        "Sync B",
        "Tempo A",
        "Deck B track overview",
        "High A",
        "Mid B",
        "Low A",
        "Filter B",
        "Headphones A",
        "Level B",
        "Crossfader",
        "Add my Music folder",
    ] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    assert!(harness.query_by_label("Open Mixxx instead").is_none(), "the DJ space opens no other window");
    harness.get_by_label("Load Paper Lanterns on deck B").click();
    until(&mut harness, "deck B to load", |app| app.dj.decks.deck(1)["track"] == 2);
    assert!(harness.state().projects.dj_deck_peaks(1).is_some_and(|w| w.frames == 88_200));
    assert!(harness.state().projects.dj_deck_peaks(0).is_none());
    assert_eq!(harness.state().dj.decks.deck(1)["title"], "Paper Lanterns");
    assert_eq!(harness.state().dj.decks.bpm(1), Some(120.0));
    harness.get_by_label("Load Moonlit Echo on deck A").click();
    until(&mut harness, "deck A to load", |app| app.dj.decks.deck(0)["track"] == 1);
    // Both decks know their tempo: Sync plays A at B's.
    harness.state_mut().set_rate(1, 1.05);
    harness.run();
    harness.get_by_label("Sync A").click();
    harness.run();
    assert!((harness.state().dj.decks.number(0, "rate", 1.0) - 1.05).abs() < 1e-9);
    assert_eq!(harness.state().dj.decks.bpm(0), harness.state().dj.decks.bpm(1));
    harness.state_mut().set_rate(0, 1.0);
    harness.run();
    // The page reads the session's decks: the speed went to deck A in the session.
    harness.state_mut().dj.decks.read_at = None;
    harness.state_mut().dj.decks.snapshot["busy"] = json!(true);
    harness.step();
    assert_eq!(harness.state().dj.decks.deck(0)["rate"], 1.0);
    assert_eq!(harness.state().dj.decks.deck(1)["rate"], 1.05);
    assert!(harness.state().session().is_some(), "the studio's project stays open behind the decks");
}

#[test]
fn audio_files_and_folders_dropped_on_the_dj_space_join_the_library() {
    #[derive(Debug)]
    struct Dropped(PathBuf);
    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &[]);
    until(&mut harness, "the empty library", |app| app.dj.decks.snapshot["tracks"].is_array());
    assert!(harness.query_by_label("Your DJ library is empty").is_some());
    let music = dir.path().join("music");
    std::fs::create_dir_all(music.join("set")).unwrap();
    song(&music.join("set").join("Night Run.wav"));
    std::fs::write(music.join("notes.txt"), "not audio").unwrap();
    harness.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(music)));
    harness.step();
    assert_eq!(harness.state().dj.decks.imports.len() + harness.state().dj.decks.imported, 1, "only the audio file");
    until(&mut harness, "the import", |app| {
        app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 1)
            && app.dj.decks.snapshot["busy"] != true
    });
    assert_eq!(harness.state().dj.decks.snapshot["tracks"][0]["title"], "Night Run");
    assert!(harness.query_by_label("Load Night Run on deck A").is_some());
    assert!(harness.state().session().is_some(), "a drop on the DJ space opens nothing in the studio");
}

#[test]
fn the_eq_knob_is_flat_in_the_middle_kills_at_the_left_and_boosts_at_the_right() {
    assert_eq!(db_of_knob(0.5), 0.0);
    assert_eq!(db_of_knob(0.0), f64::from(suhina_engine::DECK_KILL_DB));
    assert_eq!(db_of_knob(1.0), f64::from(suhina_engine::DECK_BOOST_DB));
    assert!(db_of_knob(0.25) > -15.0, "the first half of a cut trims");
    for knob in [0.0f32, 0.1, 0.25, 0.5, 0.7, 1.0] {
        assert!((knob_of_db(db_of_knob(knob)) - knob).abs() < 1e-5, "{knob}");
    }
    assert_eq!(clock(83.47), "1:23.4");
    assert_eq!(clock(-1.0), "0:00.0");
}

/// Thirty seconds of something like music at 124 BPM: a kick on each beat over a chord.
fn music(path: &Path, root: f32) {
    let rate = 44_100usize;
    let beat = rate as f32 * 60.0 / 124.0;
    let spec = suhina_media::write::AudioFileSpec::float_wav(rate as u32);
    let audio: Vec<f32> = (0..rate * 30)
        .flat_map(|i| {
            let t = i as f32 / rate as f32;
            let since = (i as f32 % beat) / rate as f32;
            let kick = (std::f32::consts::TAU * 55.0 * since).sin() * (-since * 18.0).exp() * 0.8;
            let bar = (i as f32 / (beat * 4.0)).floor() as i32 % 4;
            let swell = 0.12 + 0.1 * (bar as f32 / 3.0);
            let chord = [1.0, 1.26, 1.5].iter().map(|r| (std::f32::consts::TAU * root * r * t).sin()).sum::<f32>()
                * swell
                / 3.0;
            let s = (kick + chord).clamp(-1.0, 1.0);
            [s, s]
        })
        .collect();
    suhina_media::write::write_audio_file(path, &audio, &spec, suhina_media::write::WriteMode::Replace).unwrap();
}

#[test]
#[ignore = "requires a GPU adapter; renders a review artifact"]
fn render_the_decks_for_review() {
    render_decks("dj-decks.png", false);
}

#[test]
#[ignore = "writes a picture of four decks for review; run with --ignored"]
fn render_four_decks_for_review() {
    render_decks("dj-decks-four.png", true);
}

fn render_decks(file: &str, four: bool) {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    let theme_id = std::env::var("YOBUKO_SHELL_THEME").unwrap_or_else(|_| theme::YOBUKO.to_owned());
    let dir = tempfile::tempdir().unwrap();
    let names = ["Moonlit Echo", "Paper Lanterns", "Mountain Call", "Vocal Steps", "Night Run"];
    let catalog = suhina_dj::Catalog {
        tracks: names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let path = dir.path().join(format!("{name}.wav"));
                if i < 2 {
                    music(&path, if i == 0 { 220.0 } else { 196.0 });
                } else {
                    song(&path);
                }
                suhina_dj::Track {
                    id: i as u32 + 1,
                    path: path.canonicalize().unwrap(),
                    title: (*name).into(),
                    source_project: None,
                    seconds: if i < 2 { 30.0 } else { 2.0 },
                    bpm: Some([124.0, 126.0, 122.0, 128.0, 174.0][i]),
                    key: Some(["8A", "5A", "11A", "9A", "4A"][i].into()),
                }
            })
            .collect(),
        ..Default::default()
    };
    catalog.save(&dir.path().join("dj-library.json")).unwrap();
    let mut app = App::new();
    app.settings.theme = theme_id.clone();
    app.set_file_library(suhina_library::Library::new(Some(dir.path().to_path_buf())));
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(Vec2::new(1280.0, if four { 1420.0 } else { 920.0 }))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&harness.ctx);
    theme::set_palette(&harness.ctx, crate::settings::themes::Themes::default().palette(&theme_id));
    harness.run_steps(2);
    super::super::menu_bar::pick_mode(&mut harness, "DJ");
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 5));
    harness.get_by_label("Load Moonlit Echo on deck A").click();
    until(&mut harness, "deck A", |app| app.dj.decks.deck(0)["track"] == 1);
    harness.get_by_label("Load Paper Lanterns on deck B").click();
    until(&mut harness, "deck B", |app| app.dj.decks.deck(1)["track"] == 2);
    if four {
        harness.state_mut().dj.decks.four = true;
        harness.run_steps(1);
        harness.state_mut().deck_command("dj.deck.load", json!({"deck":"C","track":3}));
        until(&mut harness, "deck C", |app| app.dj.decks.deck(2)["track"] == 3);
        harness.state_mut().deck_command("dj.deck.load", json!({"deck":"D","track":4}));
        until(&mut harness, "deck D", |app| app.dj.decks.deck(3)["track"] == 4);
        let app = harness.state_mut();
        app.seek(2, 0.6);
        app.deck_command("dj.deck.tone", json!({"deck":"D","low":0.0,"mid":0.0,"high":-8.0,"filter":-0.3}));
        app.dj.decks.tools[2].meter = [0.44, 0.41];
    }
    let app = harness.state_mut();
    app.seek(0, 12.4);
    app.seek(1, 3.1);
    app.set_rate(0, 1.016);
    app.deck_command("dj.deck.tone", json!({"deck":"A","low":-40.0,"mid":0.0,"high":2.0,"filter":0.0}));
    app.deck_command("dj.deck.tone", json!({"deck":"B","low":0.0,"mid":-6.0,"high":0.0,"filter":0.45}));
    app.deck_command(
        "dj.mixer.set",
        json!({"crossfade":0.38,"levelA":0.9,"levelB":0.72,"cueA":false,"cueB":false,"levelC":0.8,"sideD":"thru"}),
    );
    // Hot cues on deck A, a beat loop playing on deck B, a playlist.
    for (pad, seconds) in [(0, 2.0), (1, 9.7), (2, 12.4), (4, 21.0)] {
        harness.state_mut().seek(0, seconds);
        harness.state_mut().hot_cue(0, pad);
        until(&mut harness, "a hot cue", |app| app.dj.decks.hot_cues(0)[pad].is_some());
    }
    let revision = harness.state().dj.decks.snapshot["revision"].clone();
    harness.state_mut().deck_command("dj.playlist.create", json!({"name": "Friday warm-up", "revision": revision}));
    until(&mut harness, "a playlist", |app| {
        app.dj.decks.snapshot["playlists"].as_array().is_some_and(|p| !p.is_empty())
    });
    let app = harness.state_mut();
    app.seek(0, 12.4);
    app.beat_loop(1, 4.0);
    // Effects: an echo and a filter on deck A, a reverb ready on the master; the units showing.
    app.dj.decks.show_fx = Some(true);
    app.set_fx(0, json!({"slot": 1, "effect": "echo", "on": true, "meta": 0.6}));
    app.set_fx(0, json!({"slot": 2, "effect": "filter", "meta": 0.3}));
    app.set_fx(0, json!({"deckA": true, "mix": 0.7, "beats": 0.5}));
    app.set_fx(1, json!({"slot": 1, "effect": "reverb", "meta": 0.8}));
    app.set_fx(1, json!({"master": true}));
    app.key_sync(1);
    app.deck_command("dj.deck.tone", json!({"deck":"A","low":-40.0,"mid":0.0,"high":2.0,"filter":0.0,"trim":3.0}));
    app.dj.decks.tools[0].meter = [0.62, 0.58];
    app.dj.decks.tools[1].meter = [0.3, 0.34];
    app.dj.decks.master_meter = [0.71, 0.66];
    app.dj.decks.read_at = None;
    app.dj.decks.snapshot["busy"] = json!(true);
    harness.run_steps(3);
    // The meters as a playing mix has them (nothing plays in a test).
    let app = harness.state_mut();
    app.dj.decks.tools[0].meter = [0.62, 0.58];
    app.dj.decks.tools[1].meter = [0.3, 0.34];
    app.dj.decks.master_meter = [0.71, 0.66];
    harness.run_steps(1);
    harness.render().unwrap().save(folder.join(file)).unwrap();
}

/// A computer with a DDJ-FLX4 plugged in: the port is there, opening it keeps where it sends.
struct Flx4Plugged(std::sync::Mutex<Option<std::sync::Arc<suhina_session::LiveInput>>>);
impl suhina_session::MidiSystem for Flx4Plugged {
    fn ports(&self) -> Result<Vec<String>, String> {
        Ok(vec!["DDJ-FLX4 MIDI 1".into(), "Keys In".into()])
    }
    fn open(
        &self,
        _name: &str,
        input: std::sync::Arc<suhina_session::LiveInput>,
    ) -> Result<Box<dyn std::any::Any + Send>, String> {
        *self.0.lock().unwrap() = Some(input);
        Ok(Box::new(()))
    }
}

#[test]
fn a_ddj_flx4_drives_the_decks_while_the_dj_space_shows() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns"]);
    let midi = std::sync::Arc::new(Flx4Plugged(std::sync::Mutex::new(None)));
    harness.state_mut().projects.set_midi_system(midi.clone());
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 2));
    let input = midi.0.lock().unwrap().clone().unwrap();
    let flx4 = suhina_engine::port_id("DDJ-FLX4 MIDI 1");
    let keys = suhina_engine::port_id("Keys In");
    assert!(harness.state().dj.decks.controller.is_some(), "the controller is taken in the DJ space");
    // The browse knob down one track, then LOAD on the right deck.
    assert!(input.offer(flx4, &[0xB6, 0x40, 0x01]));
    assert!(input.offer(flx4, &[0x96, 0x47, 0x7F]));
    assert!(!input.offer(keys, &[0x90, 60, 100]), "a keyboard still plays the studio");
    harness.step();
    until(&mut harness, "deck B to load", |app| app.dj.decks.deck(1)["track"] == 2);
    // Faders and knobs.
    for message in [[0xB6, 0x1F, 0x7F], [0xB1, 0x13, 0x40], [0xB1, 0x0F, 0x00], [0xB6, 0x18, 0x7F], [0xB1, 0x00, 0x00]]
    {
        assert!(input.offer(flx4, &message));
    }
    harness.step();
    let decks = &harness.state().dj.decks;
    assert_eq!(decks.snapshot["performance"]["crossfade"], 1.0);
    assert!((decks.snapshot["performance"]["levelB"].as_f64().unwrap() - 64.0 / 127.0).abs() < 1e-6);
    assert_eq!(decks.deck(1)["low"], f64::from(suhina_engine::DECK_KILL_DB));
    assert_eq!(decks.deck(1)["filter"], 1.0);
    assert!((decks.number(1, "rate", 1.0) - 1.08).abs() < 1e-9, "the tempo fader at one end is +8 %");
    // The page read back what the session holds.
    harness.state_mut().dj.decks.read_at = None;
    harness.state_mut().dj.decks.snapshot["busy"] = json!(true);
    harness.step();
    let decks = &harness.state().dj.decks;
    assert_eq!(decks.snapshot["performance"]["crossfade"], 1.0);
    assert!((decks.number(1, "rate", 1.0) - 1.08).abs() < 1e-9);
    // Back in the studio, the controller is let go.
    super::super::menu_bar::pick_mode(&mut harness, "Studio");
    harness.step();
    assert!(harness.state().dj.decks.controller.is_none());
    assert!(!input.offer(flx4, &[0x90, 0x0B, 0x7F]));
}

#[test]
fn a_deck_has_hot_cues_loops_beat_jumps_and_the_mixer_its_trim_master_and_meters() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 1));
    for label in [
        "Hot cue 1 A",
        "Hot cue 8 B",
        "Loop in A",
        "Loop out A",
        "Reloop A",
        "Beat loop B",
        "Halve loop A",
        "Double loop B",
        "Jump back A",
        "Jump forward B",
        "Beat jump size A",
        "Nudge faster A",
        "Nudge slower B",
        "Back to start A",
        "Eject B",
        "Tempo range A",
        "Trim A",
        "Meter B",
        "Master",
        "Master meter",
        "Headphones level",
        "Headphones mix",
        "Sort by bpm",
        "Show all tracks",
    ] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    harness.get_by_label("Load Moonlit Echo on deck A").click();
    until(&mut harness, "deck A to load", |app| app.dj.decks.deck(0)["track"] == 1);
    // A hot cue is put where the deck is, kept in the library, and jumped back to.
    harness.state_mut().seek(0, 0.5);
    harness.run();
    harness.get_by_label("Hot cue 3 A").click();
    until(&mut harness, "the hot cue", |app| app.dj.decks.hot_cues(0)[2].is_some());
    assert_eq!(harness.state().dj.decks.hot_cues(0)[2].map(|c| c.1), Some(0.5));
    harness.state_mut().seek(0, 1.5);
    harness.run();
    harness.get_by_label("Hot cue 3 A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.position(0), 0.5);
    // A beat loop at 120 BPM: four beats are two seconds, past this track's end; one is half.
    harness.get_by_label("Beat loop A").click();
    harness.run();
    assert!(harness.state().dj.decks.error.as_deref().is_some_and(|e| e.contains("past the end")));
    for _ in 0..2 {
        harness.get_by_label("Halve loop A").click();
        harness.run();
    }
    assert_eq!(harness.state().dj.decks.tools[0].loop_beats, 1.0);
    harness.get_by_label("Beat loop A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.current_loop(0), Some((0.5, 1.0, true)));
    harness.get_by_label("Reloop A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.current_loop(0), Some((0.5, 1.0, false)), "exit keeps the loop");
    // Loop in and out by hand.
    harness.state_mut().seek(0, 0.2);
    harness.get_by_label("Loop in A").click();
    harness.run();
    harness.state_mut().seek(0, 0.7);
    harness.get_by_label("Loop out A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.current_loop(0), Some((0.2, 0.7, true)));
    // A beat jump of one beat: half a second on.
    harness.get_by_label("Beat jump size A").click_secondary();
    harness.run();
    harness.get_by_label("Beat jump size A").click_secondary();
    harness.run();
    assert_eq!(harness.state().dj.decks.tools[0].jump_beats, 1.0);
    harness.state_mut().send_loop_off(0);
    harness.state_mut().seek(0, 0.2);
    harness.run();
    harness.get_by_label("Jump forward A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.position(0), 0.7, "{:?}", harness.state().dj.decks.error);
    // The tempo range goes ±8, ±16, ±50 %.
    harness.get_by_label("Tempo range A").click();
    harness.run();
    assert_eq!(tools::TEMPO_RANGES[harness.state().dj.decks.tools[0].range], 16.0);
    // Back to the start, then out of the deck.
    harness.get_by_label("Back to start A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.position(0), 0.0);
    harness.get_by_label("Eject A").click();
    harness.run();
    assert!(harness.state().dj.decks.track(0).is_none());
    // The mixer's trim and master go to the session.
    harness.state_mut().dj.decks.set(Some(0), "trim", json!(4.0));
    harness.state_mut().send_tone(0);
    harness.state_mut().dj.decks.set(None, "master", json!(0.8));
    harness.state_mut().send_mixer();
    harness.state_mut().dj.decks.read_at = None;
    harness.step();
    assert_eq!(harness.state().dj.decks.deck(0)["trim"], 4.0);
    assert_eq!(harness.state().dj.decks.snapshot["performance"]["master"], json!(0.8f32 as f64));
    assert_eq!(trim_db(trim_knob(-12.0)), -12.0);
    assert_eq!(trim_db(0.5), 0.0);
    assert_eq!(tools::beats_text(0.25), "1/4");
}

#[test]
fn a_deck_locks_moves_and_matches_its_key_taps_its_tempo_and_quantizes() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 2));
    harness.get_by_label("Load Moonlit Echo on deck A").click();
    until(&mut harness, "deck A to load", |app| app.dj.decks.deck(0)["track"] == 1);
    harness.get_by_label("Load Paper Lanterns on deck B").click();
    until(&mut harness, "deck B to load", |app| app.dj.decks.deck(1)["track"] == 2);
    for label in
        ["Key lock A", "Key down B", "Key A", "Key up A", "Key sync B", "Quantize A", "Tap tempo B", "Beat grid A"]
    {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    // Faster without the key lock, the key goes up with the speed; locked, it stays.
    harness.state_mut().set_rate(0, 1.06);
    harness.run();
    assert_eq!(harness.state().dj.decks.shown_key(0).map(beats::Key::text).as_deref(), Some("3A"));
    harness.get_by_label("Key lock A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.shown_key(0).map(beats::Key::text).as_deref(), Some("8A"));
    harness.get_by_label("Key up A").click();
    harness.run();
    harness.get_by_label("Key up A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.shown_key(0).map(beats::Key::text).as_deref(), Some("10A"));
    // The session has it too.
    harness.state_mut().dj.decks.read_at = None;
    harness.step();
    let a = harness.state().dj.decks.deck(0).clone();
    assert_eq!((&a["keyLock"], &a["keyShift"]), (&json!(true), &json!(2.0)), "{a}");
    harness.get_by_label("Key A").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.deck(0)["keyShift"], 0.0, "a click on the key puts it back");
    // Key sync: B (10A) moves to A's 8A, two semitones down.
    harness.get_by_label("Key sync B").click();
    harness.run();
    assert_eq!(harness.state().dj.decks.shown_key(1).map(beats::Key::text).as_deref(), Some("8A"));
    assert_eq!(harness.state().dj.decks.deck(1)["keyShift"], -2.0);
    // Quantize: a hot cue set off the beat lands on it (120 BPM: a beat each half second).
    harness.get_by_label("Quantize A").click();
    harness.run();
    harness.state_mut().seek(0, 0.62);
    harness.run();
    harness.get_by_label("Hot cue 1 A").click();
    until(&mut harness, "the hot cue", |app| app.dj.decks.hot_cues(0)[0].is_some());
    assert_eq!(harness.state().dj.decks.hot_cues(0)[0].map(|c| c.1), Some(0.5));
    // Tapping 100 BPM sets the track's beat grid (the deck plays at its own speed again).
    harness.state_mut().set_rate(0, 1.0);
    let start = Instant::now();
    let revision = harness.state().dj.decks.snapshot["revision"].as_u64();
    for i in 0..4 {
        harness.state_mut().tap_at(0, start + Duration::from_millis(600 * i));
    }
    until(&mut harness, "the tapped grid", |app| app.dj.decks.snapshot["revision"].as_u64() > revision);
    let bpm = harness.state().dj.decks.track_bpm(0).unwrap();
    assert!((bpm - 100.0).abs() < 1.0, "{bpm}");
    // The grid moved to where the deck is, then nudged later.
    harness.state_mut().seek(0, 0.75);
    harness.run();
    let revision = harness.state().dj.decks.snapshot["revision"].as_u64();
    harness.state_mut().grid_here(0);
    until(&mut harness, "the moved grid", |app| app.dj.decks.snapshot["revision"].as_u64() > revision);
    let (first, beat) = harness.state().dj.decks.beat(0).unwrap();
    assert!((first - 0.15).abs() < 1e-3 && (beat - 0.6).abs() < 0.01, "{first} {beat}");
    let revision = harness.state().dj.decks.snapshot["revision"].as_u64();
    harness.state_mut().grid_tempo(0, 0.1);
    until(&mut harness, "the faster grid", |app| app.dj.decks.snapshot["revision"].as_u64() > revision);
    assert!((harness.state().dj.decks.track_bpm(0).unwrap() - bpm - 0.1).abs() < 1e-6);
}

#[test]
fn the_effect_units_pick_switch_and_place_their_effects() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 1));
    harness.state_mut().dj.decks.show_fx = Some(true);
    harness.run();
    for label in
        ["FX 1 effect 1", "FX 2 on 3", "FX 1 knob 2", "FX 2 mix", "FX 1 on deck A", "FX 2 on the master", "FX 1 longer"]
    {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    // The next effect, switched on, put on deck A, its length doubled.
    harness.get_by_label("FX 1 effect 1").click_secondary();
    harness.run();
    assert_eq!(harness.state().dj.decks.fx(0)["slots"][0]["effect"], "echo");
    harness.get_by_label("FX 1 on 1").click();
    harness.run();
    harness.get_by_label("FX 1 on deck A").click();
    harness.run();
    harness.get_by_label("FX 1 longer").click();
    harness.run();
    // The session has it all.
    harness.state_mut().dj.decks.read_at = None;
    harness.step();
    let fx = harness.state().dj.decks.fx(0).clone();
    assert_eq!((&fx["slots"][0]["on"], &fx["deckA"], &fx["beats"]), (&json!(true), &json!(true), &json!(2.0)), "{fx}");
    // The DDJ-FLX4's BEAT FX: the next effect, off, the level, and the channel switch to B.
    let app = harness.state_mut();
    app.flx4(super::flx4::Flx4::FxSelect(1));
    app.flx4(super::flx4::Flx4::FxOn);
    app.flx4(super::flx4::Flx4::FxLevel(0.25));
    app.flx4(super::flx4::Flx4::FxChannel(1, true));
    app.dj.decks.read_at = None;
    harness.step();
    let fx = harness.state().dj.decks.fx(0).clone();
    assert_eq!(fx["slots"][0]["effect"], "pingPong");
    assert_eq!(fx["slots"][0]["on"], false);
    assert_eq!(fx["mix"], 0.25);
    assert_eq!((&fx["deckA"], &fx["deckB"], &fx["master"]), (&json!(false), &json!(true), &json!(false)));
    // The switch off both decks puts the unit on the master.
    harness.state_mut().flx4(super::flx4::Flx4::FxChannel(1, false));
    harness.run();
    assert_eq!(harness.state().dj.decks.fx(0)["master"], true);
}

#[test]
fn the_sampler_loads_from_the_library_and_the_pads_play_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 2));
    harness.state_mut().dj.decks.show_sampler = Some(true);
    harness.run();
    for label in ["Sampler 1", "Sampler 8", "Sampler 3 level", "Pad mode A"] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    // An empty slot clicked loads the library's selected track.
    harness.get_by_label("Sampler 3").click();
    until(&mut harness, "the sampler slot", |app| app.dj.decks.sampler(2)["track"] == 1);
    // Deck A's pads switched to the sampler: pad S3 plays the slot.
    harness.get_by_label("Pad mode A").click();
    harness.run();
    assert!(harness.query_by_label("Hot cue 1 A").is_none());
    // A test has no audio device: the pad asks the session to play, which says so.
    let asked = |app: &App| app.dj.decks.error.as_deref().is_some_and(|e| e.contains("audio"));
    harness.get_by_label("Sampler pad 3 A").click();
    harness.run();
    assert!(asked(harness.state()), "{:?}", harness.state().dj.decks.error);
    harness.state_mut().dj.decks.error = None;
    harness.get_by_label("Sampler pad 3 A").click_secondary();
    harness.run();
    assert_eq!(harness.state().dj.decks.error, None, "stopping a slot that is not playing is fine");
    // The controller's sampler pads do the same.
    harness.state_mut().flx4(super::flx4::Flx4::Sampler(2, false));
    harness.run();
    assert!(asked(harness.state()));
    harness.state_mut().load_sampler(5, 2);
    until(&mut harness, "slot 6", |app| app.dj.decks.sampler(5)["track"] == 2);
}

#[test]
fn the_library_searches_sorts_and_shows_a_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns", "Mountain Call"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 3));
    let titles = |app: &App| -> Vec<String> {
        app.dj.decks.shown_tracks().iter().map(|t| t["title"].as_str().unwrap_or_default().to_owned()).collect()
    };
    harness.state_mut().dj.decks.library.search = "moun".into();
    harness.run();
    assert_eq!(titles(harness.state()), ["Mountain Call"]);
    assert!(harness.query_by_label("Load Paper Lanterns on deck A").is_none());
    harness.state_mut().dj.decks.library.search.clear();
    harness.get_by_label("Sort by title").click();
    harness.run();
    assert_eq!(titles(harness.state()), ["Moonlit Echo", "Mountain Call", "Paper Lanterns"]);
    harness.get_by_label("Sort by title").click();
    harness.run();
    assert_eq!(titles(harness.state()), ["Paper Lanterns", "Mountain Call", "Moonlit Echo"]);
    harness.get_by_label("Sort by title").click();
    harness.run();
    assert_eq!(
        titles(harness.state()),
        ["Moonlit Echo", "Paper Lanterns", "Mountain Call"],
        "back to the library's order"
    );
    // A playlist made here, a track added to it, and the playlist shown.
    harness.get_by_label("+ New playlist").click();
    until(&mut harness, "the playlist", |app| {
        app.dj.decks.snapshot["playlists"].as_array().is_some_and(|p| p.len() == 1)
            && app.dj.decks.snapshot["busy"] != true
    });
    let playlist = harness.state().dj.decks.snapshot["playlists"][0]["id"].as_u64().unwrap();
    assert_eq!(harness.state().dj.decks.snapshot["playlists"][0]["name"], "Playlist 1");
    let revision = harness.state().dj.decks.snapshot["revision"].clone();
    harness
        .state_mut()
        .deck_command("dj.playlist.addTrack", json!({"playlist": playlist, "track": 2, "revision": revision}));
    until(&mut harness, "the track in the playlist", |app| {
        app.dj.decks.snapshot["playlists"][0]["tracks"].as_array().is_some_and(|t| t.len() == 1)
    });
    harness.get_by_label("Show playlist Playlist 1").click();
    harness.run();
    assert_eq!(titles(harness.state()), ["Paper Lanterns"]);
    harness.get_by_label("Show all tracks").click();
    harness.run();
    assert_eq!(titles(harness.state()).len(), 3);
}

#[test]
fn the_rec_button_records_the_mix_and_shows_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 1));
    assert!(harness.query_by_label("Record the mix").is_some());
    // A test has no audio device: recording asks the session, which says so, and stays off.
    harness.state_mut().toggle_recording(Some(dir.path().join("Recordings")));
    harness.run();
    assert!(harness.state().dj.decks.error.as_deref().is_some_and(|e| e.contains("audio")));
    assert_eq!(harness.state().dj.decks.recording()["on"], false);
    // While the session records, the button counts.
    assert_eq!(super::record::rec_text(true, 3675.4), "REC 1:01:15");
    assert_eq!(super::record::rec_text(false, 12.0), "REC");
    assert!(super::record::recordings_folder().is_none_or(|f| f.ends_with("YOBUKO Recordings")));
}

#[test]
fn the_library_makes_crates_and_shows_each_days_history() {
    let dir = tempfile::tempdir().unwrap();
    let history = serde_json::json!({"format": 1, "days": [
        {"day": "2026-11-12", "plays": [{"track": 2, "title": "Paper Lanterns", "at": "22:10"}, {"track": 1, "title": "Moonlit Echo", "at": "22:14"}]}
    ]});
    std::fs::write(dir.path().join("dj-history.json"), serde_json::to_vec(&history).unwrap()).unwrap();
    // A tall window without the effects and sampler rows, so the side bar's last entries show.
    let height = 1100.0;
    let mut harness = dj_window_sized(dir.path(), &["Moonlit Echo", "Paper Lanterns", "Mountain Call"], height);
    harness.state_mut().dj.decks.show_fx = Some(false);
    harness.state_mut().dj.decks.show_sampler = Some(false);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 3));
    let titles = |app: &App| -> Vec<String> {
        app.dj.decks.shown_tracks().iter().map(|t| t["title"].as_str().unwrap_or_default().to_owned()).collect()
    };
    // The day's history, in the order played.
    let day = harness.get_by_label("Show the history of 2026-11-12").rect();
    assert!(day.bottom() < height - 40.0, "the history entry is off screen: {day:?}");
    harness.get_by_label("Show the history of 2026-11-12").click();
    harness.run();
    assert_eq!(titles(harness.state()), ["Paper Lanterns", "Moonlit Echo"]);
    // A crate, made and filled.
    harness.get_by_label("+ New crate").click();
    until(&mut harness, "the crate", |app| app.dj.decks.lists().iter().any(|l| l.is_crate && l.name == "Crate 1"));
    let id = harness.state().dj.decks.lists().iter().find(|l| l.is_crate).map(|l| l.id).unwrap();
    let revision = harness.state().dj.decks.snapshot["revision"].clone();
    harness
        .state_mut()
        .deck_command("dj.playlist.addTrack", serde_json::json!({"playlist": id, "track": 3, "revision": revision}));
    until(&mut harness, "the crate's track", |app| app.dj.decks.lists().iter().any(|l| l.is_crate && l.count == 1));
    harness.get_by_label("Show crate Crate 1").click();
    harness.run();
    assert_eq!(titles(harness.state()), ["Mountain Call"]);
    assert!(harness.state().dj.decks.library.day.is_none());
    harness.get_by_label("Show all tracks").click();
    harness.run();
    assert_eq!(titles(harness.state()).len(), 3);
}

#[test]
fn auto_dj_queues_from_the_library_and_a_deck_marks_its_intro_and_outro() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 2));
    harness.state_mut().queue_auto_dj(2);
    harness.state_mut().queue_auto_dj(1);
    until(&mut harness, "the queue", |app| app.dj.decks.auto_queue() == [2, 1]);
    harness.get_by_label("Show the Auto DJ list").click();
    harness.run();
    let titles: Vec<String> = harness
        .state()
        .dj
        .decks
        .shown_tracks()
        .iter()
        .map(|t| t["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(titles, ["Paper Lanterns", "Moonlit Echo"]);
    for label in ["Auto DJ", "Skip to the next track", "Longer fade"] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    harness.get_by_label("Longer fade").click();
    until(&mut harness, "the fade", |app| app.dj.decks.auto()["fade"] == 9.0);
    // A test has no audio device: switched on, Auto DJ loads deck A and then says it cannot play.
    harness.get_by_label("Auto DJ").click();
    until(&mut harness, "deck A loaded", |app| app.dj.decks.track(0) == Some(2));
    until(&mut harness, "Auto DJ off", |app| {
        app.dj.decks.auto()["on"] == false && app.dj.decks.snapshot["busy"] == false
    });
    assert!(harness.state().dj.decks.auto()["error"].as_str().is_some_and(|e| e.contains("audio")));
    assert_eq!(harness.state().dj.decks.auto_queue(), [1], "the second track waits");
    // The intro and outro, at the playhead.
    harness.state_mut().set_named_cue(0, super::auto::INTRO);
    until(&mut harness, "the intro", |app| {
        app.dj.decks.named_cue(0, "Intro").is_some() && app.dj.decks.snapshot["busy"] == false
    });
    harness.state_mut().set_named_cue(0, super::auto::OUTRO);
    until(&mut harness, "the outro", |app| {
        app.dj.decks.named_cue(0, "Outro").is_some() && app.dj.decks.snapshot["busy"] == false
    });
    harness.state_mut().clear_named_cue(0, super::auto::INTRO);
    until(&mut harness, "no intro", |app| app.dj.decks.named_cue(0, "Intro").is_none());
}

#[test]
fn the_microphone_row_sets_its_input_level_and_ducking() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 1));
    for label in ["Talk on the microphone", "Microphone input", "Microphone level", "Music ducking"] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    harness.get_by_label("Microphone input").click();
    until(&mut harness, "input 2", |app| app.dj.decks.mic()["channel"] == 2);
    harness.state_mut().set_mic(serde_json::json!({"duck": 0.8}));
    until(&mut harness, "the ducking", |app| (app.dj.decks.mic()["duck"].as_f64().unwrap_or(0.0) - 0.8).abs() < 1e-6);
    // A test has no audio device: TALK asks the session, which says so.
    harness.get_by_label("Talk on the microphone").click();
    harness.run();
    assert!(harness.state().dj.decks.error.as_deref().is_some_and(|e| e.contains("audio")));
}

#[test]
fn four_decks_show_c_and_d_with_their_channels_and_crossfader_sides() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = dj_window(dir.path(), &["Moonlit Echo", "Paper Lanterns"]);
    until(&mut harness, "the library", |app| app.dj.decks.snapshot["tracks"].as_array().is_some_and(|t| t.len() == 2));
    assert!(harness.query_by_label("Play deck C").is_none());
    assert!(harness.query_by_label("Load Moonlit Echo on deck C").is_none());
    harness.get_by_label("4 decks").click();
    harness.run();
    for label in ["Play deck C", "Play deck D", "Level C", "Level D", "Crossfader side C", "Crossfader side D"] {
        assert!(harness.query_by_label(label).is_some(), "{label}");
    }
    // The library's Load C (below the decks, out of this small window's view) loads the row.
    assert!(harness.query_by_label("Load Moonlit Echo on deck C").is_some());
    harness.state_mut().dj.decks.selected = 0;
    harness.state_mut().load_selected(2);
    until(&mut harness, "deck C", |app| app.dj.decks.deck(2)["track"] == 1);
    // With a track on C, four decks keep showing.
    harness.get_by_label("4 decks").click();
    harness.run();
    assert!(harness.state().dj.decks.four);
    // Deck C goes from the left of the crossfader through the middle, then right.
    harness.get_by_label("Crossfader side C").click();
    until(&mut harness, "C through", |app| app.dj.decks.snapshot["performance"]["sideC"] == "thru");
    harness.get_by_label("Crossfader side C").click();
    until(&mut harness, "C right", |app| app.dj.decks.snapshot["performance"]["sideC"] == "right");
    assert_eq!(harness.state().dj.decks.crossfader_sides(), (vec![0], vec![1, 2, 3]));
    // Ejected, C and D can hide again.
    harness.get_by_label("Eject C").click();
    until(&mut harness, "C empty", |app| app.dj.decks.deck(2)["track"].is_null());
    harness.get_by_label("4 decks").click();
    harness.run();
    assert!(!harness.state().dj.decks.four);
}
