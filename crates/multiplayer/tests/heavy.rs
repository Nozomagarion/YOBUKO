// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.

//! Heavy sounds: a sound of tens of MiB streamed in many chunks over LAN and the online
//! transport, and a second session that takes it from the guest's sound cache.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use suhina_engine::{AudioData, AudioPool};
use suhina_model::{AudioRegion, ContentKinds, LaneData, Project};
use suhina_multiplayer::{ONLINE_LOOPBACK, Service};

/// About 40 MiB as a canonical float WAV: forty transfer chunks.
const SAMPLES: usize = 10 * 1024 * 1024;

fn source() -> (Project, AudioPool) {
    let mut project = suhina_multiplayer::starter_project();
    for content in &mut project.contents {
        for lane in &mut content.lanes {
            if let LaneData::Notes(notes) = &mut lane.data {
                notes.clear();
            }
        }
    }
    project.tracks[0].content = ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(AudioRegion::new("/only-on-the-host/long take.flac"));
    let mut audio = AudioPool::new();
    audio.files.insert(
        "/only-on-the-host/long take.flac".into(),
        Arc::new(AudioData {
            sample_rate: 48000,
            channels: 2,
            samples: (0..SAMPLES).map(|i| ((i % 9973) as f32 / 9973.0 - 0.5) * 0.8).collect(),
        }),
    );
    (project, audio)
}
fn wait(service: &mut Service, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let state = service.execute("multiplayer.state", &json!({})).unwrap();
        assert_ne!(state["mode"], "error", "{}", state["error"]);
        if ready(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "heavy session timed out: mode={}", state["mode"]);
        std::thread::sleep(Duration::from_millis(10));
    }
}
/// Share the project, join it, and return the guest's collected sound file.
fn session(address: &str, host_dir: &Path, guest_dir: &Path, project: &Project, audio: &AudioPool) -> PathBuf {
    let mut host = Service::default();
    host.set_storage_root(Some(host_dir.into())).unwrap();
    host.select_audio_project(project, audio).unwrap();
    host.execute("multiplayer.hostProject", &json!({"address":address})).unwrap();
    let ready = wait(&mut host, |s| s["mode"] == "hosting");
    host.take_shared().unwrap().keep();
    let mut guest = Service::default();
    guest.set_storage_root(Some(guest_dir.into())).unwrap();
    guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
    wait(&mut guest, |s| s["mode"] == "connected");
    let snapshot = guest.take_shared().unwrap();
    let folder = snapshot.folder().unwrap().to_path_buf();
    let remote = snapshot.session().unwrap();
    snapshot.keep();
    let (key, data) = remote.audio().files.iter().next().unwrap();
    assert_eq!(data.as_ref(), audio.files.values().next().unwrap().as_ref());
    let file = folder.join(key);
    assert!(file.is_file());
    host.execute("multiplayer.stop", &json!({})).unwrap();
    file
}
fn heavy(address: &str) {
    let host_dir = tempfile::tempdir().unwrap();
    let guest_dir = tempfile::tempdir().unwrap();
    let (project, audio) = source();
    let first = session(address, host_dir.path(), guest_dir.path(), &project, &audio);
    assert!(std::fs::metadata(&first).unwrap().len() > 40 * 1024 * 1024);
    let cache = guest_dir.path().join("Sound cache");
    let cached = cache.join(first.file_name().unwrap());
    assert!(cached.is_file());
    let second = session(address, host_dir.path(), guest_dir.path(), &project, &audio);
    assert_ne!(first.parent(), second.parent(), "each session keeps its own folder");
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1, "no partial copy or second download");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // The second session's file is the cached file itself: it was not downloaded again.
        let inode = std::fs::metadata(&cached).unwrap().ino();
        assert_eq!(std::fs::metadata(&first).unwrap().ino(), inode);
        assert_eq!(std::fs::metadata(&second).unwrap().ino(), inode);
    }
}

#[test]
fn a_forty_mib_sound_crosses_the_lan_in_chunks_and_a_second_session_takes_it_from_the_cache() {
    heavy("127.0.0.1:0");
}

#[test]
fn a_forty_mib_sound_crosses_the_online_transport_and_a_second_session_takes_it_from_the_cache() {
    heavy(ONLINE_LOOPBACK);
}
