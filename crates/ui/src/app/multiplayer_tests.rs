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
    app.multiplayer.address = "127.0.0.1:0".into();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run();
    let before = harness.state().projects.get(original).unwrap().project().clone();
    harness.get_by_label("Multiplayer").click();
    harness.run();
    assert!(harness.query_by_label("Create LAN session").is_some());
    harness.get_by_label("Create LAN session").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.state().multiplayer.bound.is_none() && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(10));
    }
    let shared = harness.state().multiplayer.bound.unwrap();
    assert_ne!(original, shared);
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
    assert!(harness.query_by_label("Copy invitation").is_some());
    assert!(harness.query_by_label("Play locally").is_some());
    harness.get_by_label("Leave session").click();
    harness.run_steps(3);
    assert!(harness.state().multiplayer.bound.is_none());
    assert!(harness.state().projects.get(shared).is_some());
    assert_eq!(harness.state().projects.get(original).unwrap().project(), &before);
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
    app.set_default_data_folder(root.path().into());
    app.multiplayer.open = true;
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
}
