// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use egui_kittest::kittest::{NodeT, Queryable};
use std::time::{Duration, Instant};

#[test]
fn the_visible_lan_entry_creates_a_separate_tab_and_keeps_other_projects() {
    let mut app = App::new();
    app.set_session(Session::new(crate::demo::demo_project()), &[]);
    let original = app.projects.active().unwrap();
    app.settings.view.menu_bar = false;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Multiplayer").click();
    harness.run();
    assert!(harness.query_by_label("Create session").is_some());
    harness.get_by_label("Create session").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    assert_ne!(original, shared);
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    assert!(harness.query_by_label("Copy invitation code").is_some());
    assert!(harness.query_by_label("Play locally").is_some());
    harness.get_by_label("Leave session").click();
    harness.run_steps(3);
    assert!(harness.state().multiplayer.bound.is_none());
    assert!(harness.state().projects.get(shared).is_some());
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
}

#[test]
fn an_online_host_shows_the_short_code_beside_the_full_invitation() {
    let mut app = App::new();
    app.settings.view.menu_bar = false;
    app.multiplayer.open = true;
    app.multiplayer_command("multiplayer.host", json!({"address":suhina_multiplayer::ONLINE_LOOPBACK}));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    let deadline = Instant::now() + Duration::from_secs(20);
    while harness.state().multiplayer.short_code.is_empty() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(2);
    let code = harness.state().multiplayer.short_code.clone();
    assert!(code.starts_with("0-"), "a loopback short code: {code:?}");
    assert!(harness.state().multiplayer.invitation.starts_with("suhina-net://"));
    assert!(harness.query_by_label(&code).is_some(), "the code is shown to be read out");
    assert!(harness.query_by_label("Copy code").is_some());
    assert!(harness.query_by_label("Copy full invitation").is_some());
    assert!(harness.query_by_label("Copy invitation code").is_none());
    harness.get_by_label("Leave session").click();
    harness.run_steps(3);
    assert!(harness.state().multiplayer.short_code.is_empty());
}

#[test]
fn two_desktop_projects_apply_confirmed_controls_and_unlock_on_disconnect() {
    let ctx = egui::Context::default();
    let mut host = App::new();
    let mut guest = App::new();
    host.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.multiplayer.invitation.is_empty() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let local = host.multiplayer.bound.unwrap();
    let remote = guest.multiplayer.bound.unwrap();
    guest.multiplayer_command("multiplayer.edit", json!({"base_revision":0,"edit":json!({"command":"track.rename","track":1,"name":"From the other computer"}).to_string()}));
    while host.multiplayer.controls.as_ref().is_none_or(|c| c.revision < 1) && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(host.projects.get(local).unwrap().project().tracks[0].name, "From the other computer");
    assert!(host.projects.execute("track.rename", &json!({"track":1,"name":"Bypass"})).is_err());
    let clip = host.projects.get(local).unwrap().project().arrangement.lanes[0].clips[0].id.0;
    for (base, from_guest, edit) in [
        (1, true, json!({"command":"note.add","clip":clip,"time":0.5,"length":1,"key":90,"velocity":100})),
        (
            2,
            false,
            json!({"command":"note.update","clip":clip,"note":4,"time":1.5,"length":0.75,"key":80,"velocity":60,"muted":true}),
        ),
        (3, true, json!({"command":"note.delete","clip":clip,"note":0})),
    ] {
        let author = if from_guest { &mut guest } else { &mut host };
        author.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit.to_string()}));
        while (host.multiplayer.controls.as_ref().unwrap().revision < base + 1
            || guest.multiplayer.controls.as_ref().unwrap().revision < base + 1)
            && Instant::now() < deadline
        {
            host.poll_multiplayer(&ctx);
            guest.poll_multiplayer(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(host.multiplayer.controls.as_ref().unwrap().revision, base + 1);
        assert_eq!(
            host.projects.get(local).unwrap().project().contents,
            guest.projects.get(remote).unwrap().project().contents
        );
    }
    host.leave_multiplayer();
    assert!(host.projects.execute("track.rename", &json!({"track":1,"name":"Local"})).is_ok());
    while guest.multiplayer.service.active() && Instant::now() < deadline {
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    guest.poll_multiplayer(&ctx);
    assert!(guest.multiplayer.bound.is_none());
    assert_eq!(guest.projects.get(remote).unwrap().project().tracks[0].name, "From the other computer");
    assert!(guest.projects.execute("track.rename", &json!({"track":1,"name":"Retained"})).is_ok());
}

#[test]
#[ignore = "requires a GPU adapter; renders the LAN entry point"]
fn render_multiplayer_panel_for_review() {
    let mut app = App::new();
    app.set_session(Session::new(suhina_multiplayer::starter_project()), &[]);
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    app.multiplayer.open = true;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&harness.ctx);
    harness.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    harness.render().unwrap().save(folder.join("multiplayer-lan.png")).unwrap();
    harness.get_by_label("Share current MIDI project").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(harness.state().multiplayer.bound.is_some());
    harness.run_steps(2);
    harness.render().unwrap().save(folder.join("multiplayer-midi-copy.png")).unwrap();
    harness.get_by_label("MIDI notes").click();
    harness.run_steps(2);
    harness.render().unwrap().save(folder.join("multiplayer-midi-notes.png")).unwrap();
    harness.get_by_label("Clips").click();
    harness.run_steps(2);
    let shared = harness.state().multiplayer.bound.unwrap();
    let clip = harness.state().projects.get(shared).unwrap().project().arrangement.lanes[0].clips[0].id.0;
    harness.get_by_label(&format!("Edit clip {clip}")).click();
    harness.run_steps(2);
    harness.render().unwrap().save(folder.join("multiplayer-clips.png")).unwrap();
}

#[test]
fn sharing_the_current_midi_project_opens_a_copy_of_its_real_music() {
    let mut app = App::new();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].name = "My own composition".into();
    project.transport.tempo_bpm = 137.0;
    if let suhina_engine::model::LaneData::Notes(notes) = &mut project.contents[0].lanes[0].data {
        notes[0].key = 77;
    }
    let folder = tempfile::tempdir().unwrap();
    let source_file = folder.path().join("Original.suhina");
    let mut session = Session::new(project);
    session.execute("project.save", &json!({"path":source_file})).unwrap();
    let original_bytes = std::fs::read(&source_file).unwrap();
    app.set_session(session, &[]);
    app.multiplayer.open = true;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let original = app.projects.active().unwrap();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Share current MIDI project").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(2);
    let rows: Vec<_> = harness.query_all_by_label("Set volume").collect();
    assert_eq!(rows.len(), before.tracks.len());
    let mut previous_y = f32::NEG_INFINITY;
    for button in rows {
        assert!(button.rect().left() < 800.0, "track controls must fit in the viewport");
        assert!(button.rect().top() > previous_y, "each track must occupy its own grid row");
        previous_y = button.rect().top();
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    assert_ne!(shared, original);
    let session = harness.state().projects.get(shared).unwrap();
    assert!(session.has_unsaved_changes(), "received music must prompt for a first save");
    assert!(session.source().is_none(), "a copy must not inherit the original's file path");
    let copy = session.project();
    assert_eq!(copy.tracks[0].name, "My own composition");
    assert_eq!(copy.transport.tempo_bpm, 137.0);
    assert_eq!(copy.contents, before.contents);
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    let copy_file = folder.path().join("Shared copy.suhina");
    harness.state_mut().projects.execute_in(Some(shared), "project.save", &json!({"path":copy_file})).unwrap();
    assert!(!harness.state().projects.get(shared).unwrap().has_unsaved_changes());
    assert_eq!(std::fs::read(&source_file).unwrap(), original_bytes);
    assert_eq!(harness.state().projects.get(original).unwrap().source(), Some(source_file.as_path()));
    harness.get_by_label("Leave session").click();
    harness.run_steps(3);
    assert!(harness.state().projects.get(shared).is_some());
}

#[test]
fn an_audio_project_is_refused_without_opening_a_listener_or_replacing_its_tab() {
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].content.audio = true;
    let mut app = App::new();
    app.set_session(Session::new(project), &[]);
    app.multiplayer.open = true;
    let original = app.projects.active().unwrap();
    let before = app.projects.get(original).unwrap().project().clone();
    app.host_current_midi();
    assert!(app.multiplayer.error.as_ref().is_some_and(|e| e.contains("audio")));
    assert!(!app.multiplayer.service.active());
    assert!(app.multiplayer.bound.is_none());
    assert_eq!(app.projects.active(), Some(original));
    assert_eq!(app.projects.get(original).unwrap().project(), &before);
}

#[test]
fn visible_note_buttons_edit_the_shared_music_and_refuse_a_stale_draft() {
    let mut app = App::new();
    app.set_session(Session::new(suhina_multiplayer::starter_project()), &[]);
    let original = app.projects.active().unwrap();
    app.multiplayer.open = true;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Share current MIDI project").click();
    let deadline = Instant::now() + Duration::from_secs(8);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.get_by_label("MIDI notes").click();
    harness.run_steps(2);
    let shared = harness.state().multiplayer.bound.unwrap();
    let clip = harness.state().projects.get(shared).unwrap().project().arrangement.lanes[0].clips[0].id.0;
    harness.get_by_label("Add note").click();
    while harness.state().multiplayer.controls.as_ref().is_none_or(|c| c.revision < 1) && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.controls.as_ref().unwrap().revision, 1);
    let list = harness.state_mut().projects.execute_in(Some(shared), "note.list", &json!({"clip":clip})).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 5);
    assert!(harness.query_by_label("Refresh note draft").is_some());
    harness.get_by_label("Edit note 1").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Apply note").is_some());
    assert!(harness.query_by_label("Delete note").is_some());
    // Another participant's accepted operation arrives while this draft still names note 0.
    harness.state_mut().multiplayer_command(
        "multiplayer.edit",
        json!({"base_revision":1,
        "edit":json!({"command":"note.delete","clip":clip,"note":0}).to_string()}),
    );
    while harness.state().multiplayer.controls.as_ref().unwrap().revision < 2 && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(2);
    assert!(harness.query_by_label("Refresh note draft").is_some());
    assert!(harness.get_by_label("Apply note").accesskit_node().is_disabled());
    assert!(harness.get_by_label("Delete note").accesskit_node().is_disabled());
    harness.get_by_label("Refresh note draft").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Apply note").is_none(), "refresh must not rebind the shifted index");
    harness.get_by_label("Edit note 1").click();
    harness.run_steps(2);
    harness.get_by_label("Apply note").click();
    while harness.state().multiplayer.controls.as_ref().unwrap().revision < 3 && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.controls.as_ref().unwrap().revision, 3);
    harness.get_by_label("Edit note 1").click();
    harness.run_steps(2);
    harness.get_by_label("Delete note").click();
    while harness.state().multiplayer.controls.as_ref().unwrap().revision < 4 && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.controls.as_ref().unwrap().revision, 4);
    let list = harness.state_mut().projects.execute_in(Some(shared), "note.list", &json!({"clip":clip})).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 3);
    assert!(
        harness
            .state_mut()
            .projects
            .execute_in(Some(shared), "note.delete", &json!({"clip":clip,"notes":[0]}))
            .is_err()
    );
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
}

#[test]
fn visible_clip_buttons_keep_neighbours_and_deleted_note_drafts_cannot_be_rebound() {
    let mut app = App::new();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].name = "T".repeat(256);
    project.contents[0].name = "N".repeat(1024);
    app.set_session(Session::new(project), &[]);
    let original = app.projects.active().unwrap();
    app.multiplayer.open = true;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Share current MIDI project").click();
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    let clip = harness.state().projects.get(shared).unwrap().project().arrangement.lanes[0].clips[0].id.0;
    harness.get_by_label("MIDI notes").click();
    harness.run_steps(2);
    harness.get_by_label("Edit note 1").click();
    harness.run_steps(2);
    harness.get_by_label("Clips").click();
    harness.run_steps(2);
    assert!(
        harness.get_by_label("Create clip").rect().right() < 1280.0,
        "long names must not push actions outside the viewport"
    );
    for (base, button) in [
        (0, "Create clip"),
        (1, "Duplicate clip"),
        (2, "Silence clip"),
        (3, "Enable clip"),
        (4, "Move clip"),
        (5, "Trim clip edges"),
        (6, "Delete clip"),
    ] {
        if base > 0 {
            harness.get_by_label(&format!("Edit clip {clip}")).click();
            harness.run_steps(2);
        }
        harness.get_by_label(button).click();
        while harness.state().multiplayer.controls.as_ref().unwrap().revision < base + 1 && Instant::now() < deadline {
            harness.run_steps(2);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(harness.state().multiplayer.controls.as_ref().unwrap().revision, base + 1, "{button}");
        harness.run_steps(2);
        assert!(harness.query_by_label("Refresh clip draft").is_some());
        assert!(harness.get_by_label("Create clip").accesskit_node().is_disabled());
    }
    let project = harness.state().projects.get(shared).unwrap().project();
    assert_eq!(project.arrangement.lanes[0].clips.len(), 2, "new and duplicate clips survive deleting their source");
    assert_eq!(project.arrangement.lanes[1].clips, before.arrangement.lanes[1].clips, "neighbour track is untouched");
    harness.get_by_label("MIDI notes").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Choose another MIDI clip").is_some());
    assert!(harness.query_by_label("Apply note").is_none());
    harness.get_by_label("Choose another MIDI clip").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Add note").is_some());
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
}

#[test]
fn the_audio_share_button_opens_a_collected_copy_and_keeps_the_original_file() {
    use suhina_engine::model::{AudioRegion, ContentKinds, LaneData};
    use suhina_engine::{AudioData, AudioPool};
    let root = tempfile::tempdir().unwrap();
    let original_file = root.path().join("Original.wav");
    let data = std::sync::Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 8000] });
    let mut writer = hound::WavWriter::create(
        &original_file,
        hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for s in &data.samples {
        writer.write_sample(*s).unwrap();
    }
    writer.finalize().unwrap();
    let original_bytes = std::fs::read(&original_file).unwrap();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].content = ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new(original_file.display().to_string()));
    let mut session = Session::new(project);
    let mut pool = AudioPool::new();
    pool.files.insert(original_file.display().to_string(), data);
    session.set_audio(pool);
    let mut app = App::new();
    app.set_default_data_folder(root.path().join("Application data"));
    app.set_session(session, &[]);
    app.multiplayer.open = true;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let original = app.projects.active().unwrap();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Share project + audio").click();
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    let session = harness.state().projects.get(shared).unwrap();
    assert_eq!(session.audio().files.len(), 1);
    let folder = session.audio_folder().unwrap().to_path_buf();
    let file = session.audio().files.keys().next().unwrap();
    assert!(file.starts_with("Audio/"));
    assert!(folder.join(file).is_file());
    assert!(session.source().is_none());
    assert!(session.has_unsaved_changes());
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    assert_eq!(std::fs::read(&original_file).unwrap(), original_bytes);
    harness.get_by_label("Leave session").click();
    harness.run_steps(2);
    assert!(folder.join("project.suhina").is_file(), "leaving must retain the collected copy");
    let saved = folder.join("My saved song.suhina");
    harness.state_mut().projects.execute_in(Some(shared), "project.save", &json!({"path":saved})).unwrap();
    assert!(saved.is_file());
    assert_eq!(std::fs::read(&original_file).unwrap(), original_bytes);
}

#[test]
fn missing_loaded_audio_is_refused_before_creating_a_shared_tab() {
    let mut app = App::new();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].content = suhina_engine::model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data =
        suhina_engine::model::LaneData::Audio(suhina_engine::model::AudioRegion::new("/do-not-read-this-file.wav"));
    app.set_session(Session::new(project), &[]);
    app.host_current_audio();
    assert!(app.multiplayer.error.as_ref().unwrap().contains("not loaded"));
    assert!(!app.multiplayer.service.active());
    assert!(app.multiplayer.bound.is_none());
}

#[test]
#[ignore = "requires a GPU adapter; renders the collected audio sharing state"]
fn render_shared_audio_panel_for_review() {
    use suhina_engine::model::{AudioRegion, ContentKinds, LaneData};
    use suhina_engine::{AudioData, AudioPool};
    let root = tempfile::tempdir().unwrap();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].name = "Shared recording".into();
    project.tracks[0].content = ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new("/original/recording.wav"));
    let mut session = Session::new(project);
    let mut audio = AudioPool::new();
    audio.files.insert(
        "/original/recording.wav".into(),
        std::sync::Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 8000] }),
    );
    session.set_audio(audio);
    let mut app = App::new();
    app.set_session(session, &[]);
    let original = app.projects.active().unwrap();
    app.set_default_data_folder(root.path().into());
    app.multiplayer.open = true;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    theme::install(&harness.ctx);
    harness.run();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&folder).unwrap();
    harness.render().unwrap().save(folder.join("multiplayer-audio-entry.png")).unwrap();
    harness.get_by_label("Share project + audio").click();
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(harness.state().multiplayer.bound.is_some());
    harness.run_steps(2);
    harness.render().unwrap().save(folder.join("multiplayer-audio-ready.png")).unwrap();
    harness.state_mut().multiplayer.page = SharedPage::Tracks;
    harness.state_mut().multiplayer.tracks.name = "New shared keys".into();
    harness.state_mut().multiplayer.tracks.midi = true;
    harness.run_steps(3);
    harness.render().unwrap().save(folder.join("multiplayer-tracks.png")).unwrap();
    let app = harness.state_mut();
    app.projects.switch(original).unwrap();
    app.multiplayer.page = SharedPage::Takes;
    app.multiplayer.take_file = "/original/recording.wav".into();
    app.multiplayer.take_track = 1;
    app.multiplayer.take_start = 16.0;
    harness.run_steps(3);
    harness.render().unwrap().save(folder.join("multiplayer-publish-take.png")).unwrap();
}

#[test]
fn the_publish_button_uses_a_finished_local_sound_and_keeps_original_and_shared_tabs() {
    use suhina_engine::{
        AudioData, AudioPool,
        model::{AudioRegion, ContentKinds, LaneData},
    };
    let root = tempfile::tempdir().unwrap();
    let mut project = suhina_multiplayer::starter_project();
    project.tracks[0].content = ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new(""));
    let mut app = App::new();
    app.set_default_data_folder(root.path().into());
    app.set_session(Session::new(project), &[]);
    let original = app.projects.active().unwrap();
    app.multiplayer.open = true;
    app.multiplayer.online = false;
    app.multiplayer.address = "127.0.0.1:0".into();
    app.host_current_audio();
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.multiplayer.bound.is_none() && Instant::now() < deadline {
        app.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let shared = app.multiplayer.bound.unwrap();
    // A take finished later in the ordinary tab; shared recording remains gated.
    app.projects.switch(original).unwrap();
    let source = app.projects.get_mut(original).unwrap();
    let mut pool = AudioPool::new();
    pool.files.insert(
        "Recorded/Voice.wav".into(),
        std::sync::Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.3; 8000] }),
    );
    source.set_audio(pool);
    // This native local import is already represented by a finished recording's region.
    let mut source_project = source.project().clone();
    source_project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new("Recorded/Voice.wav"));
    let mut local = Session::new(source_project);
    local.set_audio(source.audio().clone());
    let local_id = app.projects.add(local, Some("Finished local recording".into())).unwrap();
    app.multiplayer.page = SharedPage::Takes;
    app.multiplayer.take_file = "Recorded/Voice.wav".into();
    app.multiplayer.take_track = 1;
    app.multiplayer.take_start = 12.0;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run_steps(3);
    let before = harness.state().projects.get(local_id).unwrap().project().clone();
    harness.get_by_label("Publish loaded take").click();
    while harness.state().multiplayer.controls.as_ref().unwrap().revision < 1 && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    let app = harness.state_mut();
    assert_eq!(app.multiplayer.controls.as_ref().unwrap().revision, 1, "{:?}", app.multiplayer.error);
    let shared_session = app.projects.get(shared).unwrap();
    assert_eq!(shared_session.audio().files.len(), 1);
    assert!(shared_session.project().arrangement.lanes.iter().flat_map(|l| &l.clips).any(|c| c.start == 12.0));
    assert_eq!(app.projects.get(local_id).unwrap().project(), &before);
    let paths = shared_session.loaded_audio_paths();
    assert!(app.projects.execute_in(Some(shared), "transport.record", &json!({"on":true})).is_err());
    assert_eq!(paths.len(), 1);
    assert!(paths[0].is_file());
    app.leave_multiplayer();
    assert!(paths[0].is_file());
}

#[test]
fn visible_track_creation_adds_midi_and_audio_to_both_tabs_and_refuses_a_stale_draft() {
    let ctx = egui::Context::default();
    let mut host = App::new();
    host.set_session(Session::new(suhina_multiplayer::starter_project()), &[]);
    host.multiplayer.open = true;
    host.multiplayer.online = false;
    host.multiplayer.address = "127.0.0.1:0".into();
    let original = host.projects.active().unwrap();
    host.host_current_midi();
    let deadline = Instant::now() + Duration::from_secs(10);
    while host.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let local = host.multiplayer.bound.unwrap();
    let mut guest = App::new();
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let remote = guest.multiplayer.bound.unwrap();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    harness.run_steps(3);
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Tracks").click();
    harness.run_steps(2);
    assert!(harness.get_by_label("Add shared track").accesskit_node().is_disabled());
    harness.state_mut().multiplayer.tracks.name = "Shared keys".into();
    harness.get_by_label("MIDI track · FM Synth").click();
    harness.run_steps(2);
    harness.get_by_label("Add shared track").click();
    while (harness.state().multiplayer.applied_revision < 1 || guest.multiplayer.applied_revision < 1)
        && Instant::now() < deadline
    {
        harness.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.applied_revision, 1, "{:?}", harness.state().multiplayer.error);
    harness.run_steps(2);
    assert!(harness.get_by_label("Add shared track").accesskit_node().is_disabled());
    harness.get_by_label("Refresh track draft").click();
    harness.run_steps(2);
    assert!(harness.state().multiplayer.tracks.name.is_empty());
    harness.state_mut().multiplayer.tracks.name = "Shared voice".into();
    harness.get_by_label("Audio track").click();
    harness.run_steps(2);
    harness.get_by_label("Add shared track").click();
    while (harness.state().multiplayer.applied_revision < 2 || guest.multiplayer.applied_revision < 2)
        && Instant::now() < deadline
    {
        harness.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    guest.settings.view.fixed_track_colour = true;
    guest.poll_plugins(&ctx);
    let host = harness.state_mut();
    assert_eq!(host.multiplayer.applied_revision, 2);
    assert_eq!(host.projects.get(local).unwrap().project(), guest.projects.get(remote).unwrap().project());
    let p = host.projects.get(local).unwrap().project();
    let keys = p.tracks.iter().find(|t| t.name == "Shared keys").unwrap();
    assert!(keys.content.notes);
    assert_eq!(
        p.channel(keys.channel.unwrap()).unwrap().devices[0].kind,
        suhina_engine::model::DeviceKind::Builtin("fm-synth".into())
    );
    assert!(p.tracks.iter().any(|t| t.name == "Shared voice" && t.content.audio));
    assert_eq!(host.projects.get(original).unwrap().project(), &before);
    assert!(
        host.projects.execute_in(Some(local), "track.createShared", &json!({"kind":"audio","name":"Bypass"})).is_err()
    );
}

#[test]
fn a_guest_can_publish_a_loaded_local_take_through_the_visible_button() {
    guest_take_button(false);
}

#[test]
#[ignore = "requires a GPU adapter; renders guest take publication"]
fn render_guest_take_publication_for_review() {
    guest_take_button(true);
}

fn guest_take_button(render: bool) {
    use suhina_engine::{
        AudioData, AudioPool,
        model::{AudioRegion, ContentKinds, LaneData},
    };
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut shared = suhina_multiplayer::starter_project();
    shared.tracks[0].content = ContentKinds { notes: false, audio: true };
    shared.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new(""));
    let mut host = App::new();
    host.set_default_data_folder(root.path().join("Host"));
    host.set_session(Session::new(shared.clone()), &[]);
    host.multiplayer.online = false;
    host.multiplayer.address = "127.0.0.1:0".into();
    host.host_current_audio();
    let deadline = Instant::now() + Duration::from_secs(10);
    while host.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    shared.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new("Recorded/My guest take.wav"));
    let mut source = Session::new(shared);
    let mut pool = AudioPool::new();
    pool.files.insert(
        "Recorded/My guest take.wav".into(),
        std::sync::Arc::new(AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 12000] }),
    );
    source.set_audio(pool);
    let mut guest = App::new();
    guest.set_default_data_folder(root.path().join("Guest"));
    guest.set_session(source, &[]);
    let original = guest.projects.active().unwrap();
    guest.multiplayer.open = true;
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let remote = guest.multiplayer.bound.unwrap();
    let local = host.multiplayer.bound.unwrap();
    guest.projects.switch(original).unwrap();
    guest.multiplayer.page = SharedPage::Takes;
    guest.multiplayer.take_file = "Recorded/My guest take.wav".into();
    guest.multiplayer.take_track = 1;
    guest.multiplayer.take_start = 12.0;
    let builder = egui_kittest::Harness::builder().with_size(Vec2::new(1280.0, 900.0));
    let builder = if render { builder.wgpu() } else { builder };
    let mut harness = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), guest);
    theme::install(&harness.ctx);
    harness.run_steps(3);
    assert_eq!(harness.state().multiplayer.state["mode"], "connected");
    let before = harness.state().projects.get(original).unwrap().project().clone();
    if render {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        std::fs::create_dir_all(&folder).unwrap();
        harness.render().unwrap().save(folder.join("multiplayer-guest-take.png")).unwrap();
    }
    harness.get_by_label("Publish loaded take").click();
    while (host.multiplayer.applied_revision < 1 || harness.state().multiplayer.applied_revision < 1)
        && Instant::now() < deadline
    {
        host.poll_multiplayer(&ctx);
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.applied_revision, 1, "{:?}", harness.state().multiplayer.error);
    assert_eq!(host.projects.get(local).unwrap().project(), harness.state().projects.get(remote).unwrap().project());
    assert_eq!(host.projects.get(local).unwrap().audio().files.len(), 1);
    assert_eq!(harness.state().projects.get(remote).unwrap().audio().files.len(), 1);
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    let paths = harness.state().projects.get(remote).unwrap().loaded_audio_paths();
    assert!(paths.iter().all(|p| p.is_file()));
    host.leave_multiplayer();
    harness.state_mut().leave_multiplayer();
    assert!(paths.iter().all(|p| p.is_file()));
}

#[test]
fn visible_invitation_renewal_preserves_the_host_and_opens_a_new_guest_copy() {
    invitation_renewal_button(false);
}

#[test]
#[ignore = "manual GPU capture of the invitation renewal control"]
fn render_invitation_renewal_for_review() {
    invitation_renewal_button(true);
}

fn invitation_renewal_button(render: bool) {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut host = App::new();
    host.set_default_data_folder(root.path().join("Host"));
    host.multiplayer.open = true;
    host.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + Duration::from_secs(15);
    while host.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let host_tab = host.multiplayer.bound.unwrap();
    let old_invitation = host.multiplayer.invitation.clone();
    let mut guest = App::new();
    guest.set_default_data_folder(root.path().join("Guest"));
    guest.multiplayer_command("multiplayer.join", json!({"invitation":old_invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let old_guest_tab = guest.multiplayer.bound.unwrap();
    let builder = egui_kittest::Harness::builder().with_size(Vec2::new(1280.0, 900.0));
    let builder = if render { builder.wgpu() } else { builder };
    let mut harness = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    theme::install(&harness.ctx);
    harness.run_steps(3);
    assert!(!harness.get_by_label("New invitation").accesskit_node().is_disabled());
    guest.leave_multiplayer();
    guest
        .projects
        .execute_in(Some(old_guest_tab), "track.rename", &json!({"track":1,"name":"My solo version"}))
        .unwrap();
    let old_music = guest.projects.get(old_guest_tab).unwrap().project().clone();
    while harness.state().multiplayer.state["guest_connected"] == true && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.state_mut().multiplayer_command("multiplayer.edit", json!({"base_revision":0,"edit":json!({"command":"track.createShared","kind":"midi","name":"Added while absent"}).to_string()}));
    while harness.state().multiplayer.applied_revision < 1 && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.applied_revision, 1);
    assert!(!harness.get_by_label("New invitation").accesskit_node().is_disabled());
    if render {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        std::fs::create_dir_all(&folder).unwrap();
        harness.render().unwrap().save(folder.join("multiplayer-renew-invitation.png")).unwrap();
    }
    harness.get_by_label("New invitation").click();
    while harness.state().multiplayer.invitation == old_invitation && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(harness.state().multiplayer.invitation != old_invitation, "invitation was not renewed");
    assert_eq!(harness.state().multiplayer.bound, Some(host_tab));
    assert_eq!(harness.state().multiplayer.applied_revision, 1);
    guest.multiplayer_command("multiplayer.join", json!({"invitation":harness.state().multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let new_guest_tab = guest.multiplayer.bound.unwrap();
    assert_ne!(old_guest_tab, new_guest_tab);
    assert_eq!(guest.projects.get(old_guest_tab).unwrap().project(), &old_music);
    assert_eq!(
        guest.projects.get(new_guest_tab).unwrap().project(),
        harness.state().projects.get(host_tab).unwrap().project()
    );
    assert_eq!(guest.multiplayer.applied_revision, 1);
    guest.multiplayer_command("multiplayer.edit", json!({"base_revision":1,"edit":json!({"command":"track.rename","track":1,"name":"Back together"}).to_string()}));
    while (guest.multiplayer.applied_revision < 2 || harness.state().multiplayer.applied_revision < 2)
        && Instant::now() < deadline
    {
        harness.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().multiplayer.applied_revision, 2);
    assert_eq!(guest.multiplayer.applied_revision, 2);
    assert_eq!(guest.projects.get(old_guest_tab).unwrap().project(), &old_music);
    harness.state_mut().leave_multiplayer();
    guest.leave_multiplayer();
}

#[test]
fn visible_shared_device_controls_apply_both_types_and_refresh_stale_drafts_without_touching_sources() {
    shared_device_buttons(false);
}

#[test]
#[ignore = "manual GPU capture of generated shared device controls"]
fn render_shared_device_parameters_for_review() {
    shared_device_buttons(true);
}

fn shared_device_buttons(render: bool) {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut source = Session::new(suhina_multiplayer::starter_project());
    for kind in ["fm-synth", "utility"] {
        source.execute("device.add", &json!({"track":1,"id":kind})).unwrap();
    }
    let mut host = App::new();
    host.set_default_data_folder(root.path().join("Host"));
    host.set_session(source, &[]);
    let original = host.projects.active().unwrap();
    host.multiplayer.open = true;
    host.multiplayer.online = false;
    host.multiplayer.address = "127.0.0.1:0".into();
    host.host_current_midi();
    let deadline = Instant::now() + Duration::from_secs(20);
    while host.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let local = host.multiplayer.bound.unwrap();
    let mut guest = App::new();
    guest.set_default_data_folder(root.path().join("Guest"));
    guest.multiplayer_command("multiplayer.join", json!({"invitation":host.multiplayer.invitation}));
    while guest.multiplayer.bound.is_none() && Instant::now() < deadline {
        host.poll_multiplayer(&ctx);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let remote = guest.multiplayer.bound.unwrap();
    let builder = egui_kittest::Harness::builder().with_size(Vec2::new(1280., 900.));
    let builder = if render { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), host);
    theme::install(&h.ctx);
    h.run_steps(3);
    let original_music = h.state().projects.get(original).unwrap().project().clone();
    h.get_by_label("Devices").click();
    h.run_steps(3);
    assert_eq!(h.state().multiplayer.devices.param.as_deref(), Some("volume"));
    assert!(h.query_by_label("Parameter value").is_some());
    h.state_mut().multiplayer.devices.value = -12.;
    h.run_steps(2);
    assert!(
        h.state().projects.get(local).unwrap().project().channels[0].devices[0].params.is_empty(),
        "changing a draft must not publish it"
    );
    if render {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        std::fs::create_dir_all(&folder).unwrap();
        h.render().unwrap().save(folder.join("multiplayer-fm-parameters.png")).unwrap();
    }
    h.get_by_label("Apply shared parameter").click();
    while (h.state().multiplayer.applied_revision < 1 || guest.multiplayer.applied_revision < 1)
        && Instant::now() < deadline
    {
        h.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().multiplayer.applied_revision, 1, "{:?}", h.state().multiplayer.error);
    assert!(h.get_by_label("Apply shared parameter").accesskit_node().is_disabled());
    assert_eq!(h.state().projects.get(local).unwrap().project(), guest.projects.get(remote).unwrap().project());
    h.get_by_label("Refresh device draft").click();
    h.run_steps(2);
    h.state_mut().multiplayer.devices.value = -3.;
    guest.multiplayer_command("multiplayer.edit",json!({"base_revision":1,"edit":json!({"command":"device.setSharedParam","track":1,"device":1,"param":"gain","value":-6}).to_string()}));
    while (h.state().multiplayer.applied_revision < 2 || guest.multiplayer.applied_revision < 2)
        && Instant::now() < deadline
    {
        h.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().multiplayer.applied_revision, 2);
    assert!(h.get_by_label("Apply shared parameter").accesskit_node().is_disabled());
    h.get_by_label("Refresh device draft").click();
    h.run_steps(2);
    assert_eq!(h.state().multiplayer.devices.value, -12.);
    h.get_by_label("Shared device").click();
    h.run_steps(2);
    h.get_by_label("Pulse · Utility · device 2").click();
    h.run_steps(2);
    assert_eq!(h.state().multiplayer.devices.param.as_deref(), Some("gain"));
    assert_eq!(h.state().multiplayer.devices.value, -6.);
    h.state_mut().multiplayer.devices.value = -9.;
    h.get_by_label("Apply shared parameter").click();
    while (h.state().multiplayer.applied_revision < 3 || guest.multiplayer.applied_revision < 3)
        && Instant::now() < deadline
    {
        h.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().multiplayer.applied_revision, 3, "{:?}", h.state().multiplayer.error);
    h.get_by_label("Refresh device draft").click();
    h.run_steps(2);
    h.get_by_label("Find parameter").focus();
    h.run_steps(2);
    h.get_by_label("Find parameter").type_text("mono");
    h.run_steps(2);
    h.get_by_label("Shared parameter").click();
    h.run_steps(2);
    assert!(h.query_by_label("Gain").is_none(), "parameter search must filter out unrelated controls");
    h.get_by_label("Mono").click();
    h.run_steps(2);
    assert_eq!(h.state().multiplayer.devices.param.as_deref(), Some("mono"));
    h.get_by_label("Parameter value").click();
    h.run_steps(2);
    assert_eq!(h.state().multiplayer.devices.value, 1.);
    assert_eq!(h.state().multiplayer.applied_revision, 3, "a checkbox changes only its draft");
    if render {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
        h.render().unwrap().save(folder.join("multiplayer-utility-parameters.png")).unwrap();
    }
    h.get_by_label("Apply shared parameter").click();
    while (h.state().multiplayer.applied_revision < 4 || guest.multiplayer.applied_revision < 4)
        && Instant::now() < deadline
    {
        h.run_steps(2);
        guest.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().multiplayer.applied_revision, 4);
    assert_eq!(h.state().projects.get(local).unwrap().project(), guest.projects.get(remote).unwrap().project());
    let utility = &guest.projects.get(remote).unwrap().project().channels[0].devices[1];
    assert_eq!(utility.params.iter().find(|p| p.id == "mono").unwrap().value, 1.);
    assert_eq!(h.state().projects.get(original).unwrap().project(), &original_music);
    h.state_mut().leave_multiplayer();
    guest.leave_multiplayer();
}

#[path = "multiplayer_chain_tests.rs"]
mod chain_tests;

#[path = "multiplayer_launcher_tests.rs"]
mod launcher_tests;

#[path = "multiplayer_guest_tests.rs"]
mod guest_tests;

#[path = "multiplayer_linked_tests.rs"]
mod linked_tests;

#[test]
fn synchronous_refusals_survive_state_refreshes_and_can_be_acknowledged_without_editing() {
    let ctx = egui::Context::default();
    let mut app = App::new();
    app.multiplayer.open = true;
    app.multiplayer_command("multiplayer.host", json!({"address":"127.0.0.1:0"}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.multiplayer.bound.is_none() && Instant::now() < deadline {
        app.poll_multiplayer(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let id = app.multiplayer.bound.unwrap();
    let original = app.projects.get(id).unwrap().project().clone();
    let revision = app.multiplayer.controls.as_ref().unwrap().revision;
    let refuse = |app: &mut App| {
        app.multiplayer_command("multiplayer.edit", json!({"base_revision":revision,"edit":"{invalid-json"}));
    };
    refuse(&mut app);
    let error = app.multiplayer.error.clone().unwrap();
    for _ in 0..8 {
        app.poll_multiplayer(&ctx);
        assert_eq!(app.multiplayer.error.as_deref(), Some(error.as_str()));
    }
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run_steps(4);
    assert!(harness.query_by_label(&error).is_some());
    harness.get_by_label("Dismiss multiplayer message").click();
    harness.run_steps(8);
    assert!(harness.state().multiplayer.error.is_none());
    assert_eq!(harness.state().projects.get(id).unwrap().project(), &original);
    assert_eq!(harness.state().multiplayer.controls.as_ref().unwrap().revision, revision);
    refuse(harness.state_mut());
    harness.run_steps(4);
    assert!(harness.state().multiplayer.error.is_some());
    harness.state_mut().multiplayer_command("multiplayer.renewInvitation", json!({}));
    harness.run_steps(4);
    assert!(harness.state().multiplayer.error.is_none());
    harness.state_mut().leave_multiplayer();
}
