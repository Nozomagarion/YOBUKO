// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::io::Write;
use std::time::Duration;
use suhina_engine::AudioData;

fn sound(samples: usize, seed: f32) -> (Vec<u8>, Asset, Vec<ChainingValue>) {
    let data = AudioData {
        sample_rate: 8000,
        channels: 1,
        samples: (0..samples).map(|i| ((i as f32 + seed) * 0.37).sin() * 0.5).collect(),
    };
    let mut bytes = Vec::new();
    let (meta, cvs) = sound::encode(&data, &mut bytes, &AtomicBool::new(false)).unwrap();
    (bytes, meta, cvs)
}
fn age(path: &Path, seconds: u64) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(seconds)).unwrap();
}

#[test]
fn trimming_removes_least_recently_used_sounds_and_spares_current_and_foreign_files() {
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::open(root.path(), 3 * 40_100).unwrap();
    let mut metas = vec![];
    for (i, seconds) in [500u64, 400, 300, 200, 100].into_iter().enumerate() {
        let (bytes, meta, _) = sound(10_000, i as f32 * 1000.0);
        let path = cache.dir.join(format!("{}.wav", meta.hash));
        std::fs::write(&path, &bytes).unwrap();
        age(&path, seconds);
        metas.push(meta);
    }
    let foreign = cache.dir.join("notes.txt");
    std::fs::write(&foreign, vec![0; 100_000]).unwrap();
    std::fs::create_dir(cache.dir.join(format!("{}.wav", "a".repeat(64)))).unwrap();
    // The oldest sound is in use by the current session.
    let path = cache.find(&metas[0]).unwrap();
    cache.touch(&path);
    age(&path, 600);
    cache.trim(&BTreeSet::from([metas[0].hash.clone()])).unwrap();
    assert!(cache.find(&metas[0]).is_some(), "sounds in use are spared");
    assert!(cache.find(&metas[1]).is_none() && cache.find(&metas[2]).is_none(), "oldest go first");
    assert!(cache.find(&metas[3]).is_some() && cache.find(&metas[4]).is_some());
    assert!(foreign.is_file(), "files the cache did not write are left alone");
    // Using a sound makes it the most recent.
    cache.touch(&cache.find(&metas[3]).unwrap());
    age(&cache.find(&metas[4]).unwrap(), 50);
    let small = Cache::open(root.path(), 50_000).unwrap();
    small.trim(&BTreeSet::new()).unwrap();
    assert!(small.find(&metas[3]).is_some());
    assert!(small.find(&metas[4]).is_none() && small.find(&metas[0]).is_none());
}

#[test]
fn a_partial_copy_resumes_after_its_last_chunk_that_matches_the_tree() {
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::open(root.path(), u64::MAX).unwrap();
    let (bytes, meta, cvs) = sound(6000, 0.0);
    assert!(cvs.len() >= 5);
    let stop = AtomicBool::new(false);
    // Without a stored tree nothing can be trusted.
    std::fs::write(cache.dir.join(format!("{}.part", meta.hash)), &bytes[..3 * CHUNK]).unwrap();
    let partial = cache.resume(&meta, &stop).unwrap();
    assert_eq!((partial.verified, partial.tree.is_none()), (0, true));
    drop(partial);
    // A stored tree whose root is not the sound's hash is ignored.
    let mut forged = cvs.clone();
    forged[0][0] ^= 1;
    cache.save_tree(&meta, &sound::tree_bytes(&forged)).unwrap();
    std::fs::write(cache.dir.join(format!("{}.part", meta.hash)), &bytes[..3 * CHUNK]).unwrap();
    assert_eq!(cache.resume(&meta, &stop).unwrap().verified, 0);
    // With the real tree: three good chunks, then a corrupted one and a torn write.
    cache.save_tree(&meta, &sound::tree_bytes(&cvs)).unwrap();
    let mut part = bytes[..5 * CHUNK + 10].to_vec();
    part[3 * CHUNK + 1] ^= 1;
    std::fs::write(cache.dir.join(format!("{}.part", meta.hash)), &part).unwrap();
    let mut partial = cache.resume(&meta, &stop).unwrap();
    assert_eq!(partial.verified, 3 * CHUNK);
    assert_eq!(partial.tree.as_deref(), Some(cvs.as_slice()));
    assert_eq!(partial.file.metadata().unwrap().len(), (3 * CHUNK) as u64);
    partial.file.write_all(&bytes[3 * CHUNK..]).unwrap();
    let done = cache.complete(&meta, partial.file).unwrap();
    assert_eq!(std::fs::read(&done).unwrap(), bytes);
    assert_eq!(cache.find(&meta), Some(done));
    assert!(!cache.dir.join(format!("{}.tree", meta.hash)).exists());
    assert!(!cache.dir.join(format!("{}.part", meta.hash)).exists());
    cache.discard(&meta);
    assert!(cache.find(&meta).is_none());
}

#[test]
fn a_complete_partial_copy_still_rechecks_its_last_chunk() {
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::open(root.path(), u64::MAX).unwrap();
    let (bytes, meta, cvs) = sound(4 * CHUNK / 4 - 17, 3.0);
    cache.save_tree(&meta, &sound::tree_bytes(&cvs)).unwrap();
    std::fs::write(cache.dir.join(format!("{}.part", meta.hash)), &bytes).unwrap();
    let partial = cache.resume(&meta, &AtomicBool::new(false)).unwrap();
    assert!(partial.verified < meta.bytes);
    assert!(partial.verified.is_multiple_of(CHUNK));
}

#[test]
fn the_cache_refuses_invalid_names_and_a_relative_root() {
    assert!(Cache::open(Path::new("relative"), 1).is_err());
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::open(root.path(), 1).unwrap();
    let (_, mut meta, _) = sound(100, 0.0);
    meta.hash = "../../escape".into();
    assert!(cache.find(&meta).is_none());
    assert!(cache.resume(&meta, &AtomicBool::new(false)).is_err());
    assert!(cache.save_tree(&meta, &[1; 64]).is_err());
    assert_eq!(std::fs::read_dir(&cache.dir).unwrap().count(), 0);
    let file = root.path().join("file");
    std::fs::write(&file, b"x").unwrap();
    assert!(Cache::open(&file, 1).is_err());
}
