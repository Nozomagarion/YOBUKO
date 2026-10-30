// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::io::{Cursor, Read};
use std::net::{TcpListener, TcpStream};

fn sine(rate: u32, channels: u16, samples: usize) -> AudioData {
    AudioData { sample_rate: rate, channels, samples: (0..samples).map(|i| (i as f32 * 0.1).sin() * 0.25).collect() }
}
fn source() -> (Project, AudioPool) {
    source_with(sine(8000, 1, 12000))
}
fn source_with(data: AudioData) -> (Project, AudioPool) {
    let mut project = crate::starter_project();
    project.tracks[0].content = suhina_model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(suhina_model::AudioRegion::new("/private/original.wav"));
    let mut pool = AudioPool::new();
    pool.files.insert("/private/original.wav".into(), Arc::new(data));
    (project, pool)
}
fn stop() -> AtomicBool {
    AtomicBool::new(false)
}
struct Script {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}
fn script(input: Vec<u8>) -> Script {
    Script { input: Cursor::new(input), output: vec![] }
}
impl Read for Script {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(bytes)
    }
}
impl Write for Script {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Link for Script {
    fn set_deadline(&mut self, _: Instant) {}
}
fn pages(metas: &[Asset]) -> Vec<u8> {
    let mut input = vec![];
    let total = metas.len();
    if total == 0 {
        protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, total, manifest: vec![] }).unwrap();
    }
    for page in metas.chunks(PAGE) {
        protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, total, manifest: page.to_vec() })
            .unwrap();
    }
    input
}
/// What a host sends for one download request.
fn download(asset: &Encoded, index: usize, from: usize, tree: bool) -> Vec<u8> {
    let mut sink = script(vec![]);
    sound::send(
        &mut sink,
        &asset.source(),
        (from, tree),
        |leaves| ServerMessage::AssetTree { protocol: PROTOCOL, asset: index, leaves },
        |offset, len| ServerMessage::AssetChunk { protocol: PROTOCOL, asset: index, offset, len },
        &stop(),
        |_| {},
    )
    .unwrap();
    sink.output
}
/// What a guest sends for one upload request.
fn upload(asset: &Encoded, from: usize, tree: bool) -> Vec<u8> {
    let mut sink = script(vec![]);
    sound::send(
        &mut sink,
        &asset.source(),
        (from, tree),
        |leaves| ClientMessage::UploadTree { protocol: PROTOCOL, leaves },
        |offset, len| ClientMessage::UploadChunk { protocol: PROTOCOL, offset, len },
        &stop(),
        |_| {},
    )
    .unwrap();
    sink.output
}
/// Where a sound stream ends after its tree and `chunks` whole chunks, plus `extra` bytes.
fn boundary(stream: &[u8], chunks: usize, extra: usize) -> usize {
    let mut io = Cursor::new(stream);
    for _ in 0..=chunks {
        let frame: serde_json::Value = protocol::read(&mut io).unwrap();
        let raw = frame.get("len").or(frame.get("leaves")).unwrap().as_u64().unwrap();
        let raw = if frame.get("leaves").is_some() { raw * 32 } else { raw };
        io.set_position(io.position() + raw);
    }
    io.position() as usize + extra
}
/// The download requests (asset, offset, tree) a guest wrote, and whether it ended with AssetsDone.
fn requests(output: &[u8]) -> (Vec<(usize, usize, bool)>, bool) {
    let mut io = Cursor::new(output);
    let mut found = vec![];
    while (io.position() as usize) < output.len() {
        match protocol::read::<ClientMessage>(&mut io).unwrap() {
            ClientMessage::AssetDownload { asset, offset, tree, .. } => found.push((asset, offset, tree)),
            ClientMessage::AssetsDone { .. } => return (found, true),
            _ => panic!("unexpected guest message"),
        }
    }
    (found, false)
}
/// Session folders left in a storage root; the sound cache is expected to stay.
fn sessions(root: &Path) -> usize {
    std::fs::read_dir(root)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("session-"))
        .count()
}
fn cache_dir(root: &Path) -> PathBuf {
    root.join(crate::cache::FOLDER)
}

#[test]
fn portable_copy_deduplicates_audio_preserves_original_and_cleans_unclaimed_folders() {
    let root = tempfile::tempdir().unwrap();
    let (mut project, pool) = source();
    project.contents[1].lanes[0].data = project.contents[0].lanes[0].data.clone();
    project.tracks[1].content = suhina_model::ContentKinds { notes: false, audio: true };
    let before = project.clone();
    let prepared = prepare(&project, &pool, Some(root.path()), &stop()).unwrap();
    assert_eq!(prepared.assets.len(), 1);
    assert_eq!(prepared.snapshot.audio.files.len(), 1);
    let folder = prepared.snapshot.folder().unwrap().to_path_buf();
    assert!(folder.join("project.suhina").is_file());
    assert!(folder.join("assets.json").is_file());
    let text = suhina_project_file::to_json(&prepared.snapshot.project);
    assert!(!text.contains("private"));
    assert!(!text.contains("original.wav"));
    assert_eq!(project, before);
    assert!(crate::midi::decode_portable(text.as_bytes()).is_ok());
    let decoded = suhina_media::decode_file(&folder.join(prepared.assets[0].meta.path())).unwrap();
    assert_eq!(&decoded, pool.files["/private/original.wav"].as_ref());
    drop(prepared);
    assert!(!folder.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0, "served copies are removed with the session");
}

#[test]
fn invalid_manifests_and_path_attacks_fail_before_any_file_or_download() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    let good = prepared.assets[0].meta.clone();
    let mut invalid = vec![vec![], vec![good.clone(), good.clone()]];
    for (field, value) in [
        ("hash", serde_json::json!("../../escape")),
        ("bytes", serde_json::json!(MAX_ASSET + 1)),
        ("bytes", serde_json::json!(good.bytes + 4)),
        ("frames", serde_json::json!(usize::MAX)),
        ("channels", serde_json::json!(0)),
        ("rate", serde_json::json!(0)),
    ] {
        let mut meta = serde_json::to_value(&good).unwrap();
        meta[field] = value;
        invalid.push(vec![serde_json::from_value(meta).unwrap()]);
    }
    for metas in invalid {
        let mut io = script(pages(&metas));
        assert!(receive(&mut io, prepared.snapshot.project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).is_err());
        assert!(io.output.is_empty());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    for path in ["../outside.wav", "Audio/../x.wav", "C:/outside.wav", "Audio/"] {
        assert!(!valid_path(path));
    }
}

#[test]
fn hostile_manifest_pages_and_totals_are_refused_before_allocation() {
    let root = tempfile::tempdir().unwrap();
    let (project, _) = source();
    let mut forged = vec![];
    for (total, page) in [(MAX_SOUNDS + 1, 1), (usize::MAX, 1), (PAGE + 2, PAGE + 1), (3, 0), (2, 3)] {
        let meta = Asset { hash: "a".repeat(64), bytes: 72, rate: 8000, channels: 1, frames: 1 };
        let mut input = vec![];
        protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, total, manifest: vec![meta; page] })
            .unwrap();
        forged.push(input);
    }
    // A second page that does not continue the first.
    let meta = Asset { hash: "b".repeat(64), bytes: 72, rate: 8000, channels: 1, frames: 1 };
    let mut input = vec![];
    protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, total: 2, manifest: vec![meta.clone()] })
        .unwrap();
    protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, total: 3, manifest: vec![meta] }).unwrap();
    forged.push(input);
    for input in forged {
        let mut io = script(input);
        assert!(receive(&mut io, project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).is_err());
        assert!(io.output.is_empty());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn a_cut_download_resumes_from_its_last_verified_chunk() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    let asset = &prepared.assets[0];
    let leaves = sound::leaves(asset.meta.bytes);
    assert!(leaves > 8);
    // The connection drops in the middle of the sixth chunk.
    let full = download(asset, 0, 0, true);
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend_from_slice(&full[..boundary(&full, 5, 30)]);
    let mut io = script(input);
    let project = prepared.snapshot.project.clone();
    assert!(receive(&mut io, project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).is_err());
    assert_eq!(requests(&io.output), (vec![(0, 0, true)], false));
    assert_eq!(sessions(root.path()), 0);
    let part = cache_dir(root.path()).join(format!("{}.part", asset.meta.hash));
    assert_eq!(std::fs::metadata(&part).unwrap().len(), (5 * CHUNK) as u64);
    // The next attempt asks only for the rest, without the tree it already holds.
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(asset, 0, 5 * CHUNK, false));
    let mut io = script(input);
    let mut updates = vec![];
    let received =
        receive(&mut io, project, 3, Some(root.path()), &stop(), |done, total| updates.push((done, total))).unwrap();
    assert_eq!(requests(&io.output), (vec![(0, 5 * CHUNK, false)], true));
    assert_eq!(received.audio.files.values().next().unwrap().as_ref(), pool.files.values().next().unwrap().as_ref());
    assert!(!part.exists());
    assert!(cache_dir(root.path()).join(format!("{}.wav", asset.meta.hash)).is_file());
    assert_eq!(updates.last(), Some(&(asset.meta.bytes, asset.meta.bytes)));
}

#[test]
fn a_corrupted_chunk_is_refused_and_only_verified_chunks_are_kept() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    let asset = &prepared.assets[0];
    // A host serving altered bytes for the third chunk, under the genuine tree.
    let altered = host.path().join("altered.wav");
    let mut bytes = std::fs::read(&asset.path).unwrap();
    bytes[2 * CHUNK + 7] ^= 0x10;
    std::fs::write(&altered, &bytes).unwrap();
    let forged = Encoded {
        meta: asset.meta.clone(),
        path: altered,
        tree: asset.tree.clone(),
        _lease: Arc::clone(&asset._lease),
    };
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(&forged, 0, 0, true));
    let mut io = script(input);
    let error =
        receive(&mut io, prepared.snapshot.project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).err().unwrap();
    assert!(error.contains("verification"), "{error}");
    let part = cache_dir(root.path()).join(format!("{}.part", asset.meta.hash));
    assert_eq!(std::fs::metadata(&part).unwrap().len(), (2 * CHUNK) as u64);
    assert_eq!(sessions(root.path()), 0);
    // A forged tree that does not hash to the manifest entry is refused before any chunk.
    let mut tree = asset.tree.clone();
    tree[0][3] ^= 1;
    let forged =
        Encoded { meta: asset.meta.clone(), path: asset.path.clone(), tree, _lease: Arc::clone(&asset._lease) };
    std::fs::remove_file(&part).unwrap();
    std::fs::remove_file(cache_dir(root.path()).join(format!("{}.tree", asset.meta.hash))).unwrap();
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(&forged, 0, 0, true));
    let mut io = script(input);
    let error =
        receive(&mut io, prepared.snapshot.project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).err().unwrap();
    assert!(error.contains("tree"), "{error}");
    // Chunks for another sound, or from another offset than the one asked, are refused too.
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(asset, 1, 0, true));
    assert!(
        receive(&mut script(input), prepared.snapshot.project.clone(), 0, Some(root.path()), &stop(), |_, _| {})
            .is_err()
    );
    let cache = Cache::open(root.path(), CACHE_CAP).unwrap();
    cache.save_tree(&asset.meta, &sound::tree_bytes(&asset.tree)).unwrap();
    let _ = std::fs::remove_file(&part);
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(asset, 0, CHUNK, false));
    let mut io = script(input);
    assert!(receive(&mut io, prepared.snapshot.project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).is_err());
    assert_eq!(requests(&io.output), (vec![(0, 0, false)], false));
    assert_eq!(sessions(root.path()), 0);
}

#[test]
fn a_cached_sound_is_never_requested_again_and_a_corrupt_cache_entry_is_replaced() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    let asset = &prepared.assets[0];
    let project = prepared.snapshot.project.clone();
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(asset, 0, 0, true));
    let first = receive(&mut script(input), project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).unwrap();
    first.keep();
    // A later session: the guest answers the manifest with AssetsDone and nothing else.
    let mut io = script(pages(std::slice::from_ref(&asset.meta)));
    let second = receive(&mut io, project.clone(), 0, Some(root.path()), &stop(), |_, _| {}).unwrap();
    assert_eq!(requests(&io.output), (vec![], true));
    assert_eq!(second.audio.files.values().next().unwrap().as_ref(), pool.files.values().next().unwrap().as_ref());
    let folder = second.folder().unwrap().to_path_buf();
    second.keep();
    assert!(folder.join(asset.meta.path()).is_file());
    // Damage the cached copy: the guest notices while loading it and downloads it again.
    let cached = cache_dir(root.path()).join(format!("{}.wav", asset.meta.hash));
    std::fs::remove_file(&cached).unwrap();
    let mut bytes = std::fs::read(&asset.path).unwrap();
    bytes[200] ^= 1;
    std::fs::write(&cached, &bytes).unwrap();
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(asset, 0, 0, true));
    let mut io = script(input);
    let third = receive(&mut io, project, 0, Some(root.path()), &stop(), |_, _| {}).unwrap();
    assert_eq!(requests(&io.output), (vec![(0, 0, true)], true));
    assert_eq!(third.audio.files.len(), 1);
    assert_eq!(std::fs::read(&cached).unwrap(), std::fs::read(&asset.path).unwrap());
}

#[test]
fn verified_audio_is_persistent_only_after_the_recipient_retains_the_snapshot() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    let asset = &prepared.assets[0];
    let mut input = pages(std::slice::from_ref(&asset.meta));
    input.extend(download(asset, 0, 0, true));
    let received =
        receive(&mut script(input), prepared.snapshot.project.clone(), 7, Some(root.path()), &stop(), |_, _| {})
            .unwrap();
    let folder = received.folder().unwrap().to_path_buf();
    let mut session = received.session().unwrap();
    received.keep();
    assert!(folder.join(asset.meta.path()).is_file());
    assert!(session.has_unsaved_changes());
    let file = folder.join("Saved copy.suhina");
    session.save(&file, false).unwrap();
    let reopened = suhina_project_file::read_file(&file).unwrap();
    let (audio, warnings) = suhina_media::load_project_audio(&reopened, &folder);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(audio.files.len(), 1);
    assert_eq!(audio.files.values().next().unwrap().samples, pool.files.values().next().unwrap().samples);
    // An unclaimed snapshot removes its folder; the cache keeps the sound for next time.
    let unclaimed = receive(
        &mut script(pages(std::slice::from_ref(&asset.meta))),
        prepared.snapshot.project.clone(),
        7,
        Some(root.path()),
        &stop(),
        |_, _| {},
    )
    .unwrap();
    let folder = unclaimed.folder().unwrap().to_path_buf();
    drop(unclaimed);
    assert!(!folder.exists());
    assert!(cache_dir(root.path()).join(format!("{}.wav", asset.meta.hash)).is_file());
}

#[test]
fn missing_or_unsupported_loaded_sound_does_not_touch_sources_or_start_storage() {
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let before = project.clone();
    assert!(prepare(&project, &AudioPool::new(), Some(root.path()), &stop()).is_err());
    for channels in [0, 3] {
        let mut bad = pool.clone();
        let mut data = bad.files["/private/original.wav"].as_ref().clone();
        data.channels = channels;
        bad.files.insert("/private/original.wav".into(), Arc::new(data));
        assert!(prepare(&project, &bad, Some(root.path()), &stop()).is_err());
    }
    assert_eq!(project, before);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    assert!(prepare(&project, &pool, None, &stop()).is_err());
}

fn hostile_project(metas: &[Asset]) -> Project {
    let (mut project, _) = source();
    project.contents.clear();
    for (i, meta) in metas.iter().enumerate() {
        let mut region = suhina_model::AudioRegion::new(meta.path());
        region.sample_rate = Some(meta.rate);
        region.channels = Some(meta.channels);
        region.duration_seconds = Some(meta.frames as f64 / f64::from(meta.rate));
        project.contents.push(suhina_model::ClipContent {
            id: suhina_model::ContentId(1000 + i as u32),
            name: "Audio".into(),
            length: 4.0,
            lanes: vec![suhina_model::ContentLane { target: None, data: LaneData::Audio(region) }],
            extensions: Default::default(),
            scale: None,
        });
    }
    project
}

#[test]
fn total_manifest_quota_is_checked_before_disk_or_audio_allocation() {
    let root = tempfile::tempdir().unwrap();
    // Nine sounds of the largest size: each is allowed, together they exceed the session cap.
    let frames = (MAX_ASSET - sound::HEADER) / 4;
    let metas: Vec<_> = (0..9)
        .map(|i| Asset { hash: format!("{i:064x}"), bytes: MAX_ASSET, rate: 8000, channels: 1, frames })
        .collect();
    let mut io = script(pages(&metas));
    let error = receive(&mut io, hostile_project(&metas), 0, Some(root.path()), &stop(), |_, _| {}).err().unwrap();
    assert!(error.contains("16 GiB"), "{error}");
    assert!(io.output.is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    // Too many sounds, even small ones.
    let metas: Vec<_> = (0..=MAX_SOUNDS)
        .map(|i| Asset { hash: format!("{i:064x}"), bytes: 72, rate: 8000, channels: 1, frames: 1 })
        .collect();
    assert!(manifest(&hostile_project(&metas), &metas).is_err());
    assert!(manifest(&hostile_project(&metas[..MAX_SOUNDS]), &metas[..MAX_SOUNDS]).is_ok());
}

#[test]
fn portable_document_rejects_paths_and_metadata_beyond_the_manifest() {
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(root.path()), &stop()).unwrap();
    for bad in ["../secret.wav", "/etc/private.wav", "Audio/../../escape.wav", "C:\\private.wav"] {
        let mut p = prepared.snapshot.project.clone();
        let LaneData::Audio(r) = &mut p.contents[0].lanes[0].data else { panic!("audio") };
        r.file = bad.into();
        assert!(crate::midi::decode_portable(suhina_project_file::to_json(&p).as_bytes()).is_err());
    }
    let mut p = prepared.snapshot.project.clone();
    let LaneData::Audio(r) = &mut p.contents[0].lanes[0].data else { panic!("audio") };
    r.absolute_file = Some("/private/path.wav".into());
    assert!(crate::midi::decode_portable(suhina_project_file::to_json(&p).as_bytes()).is_err());
    let mut p = prepared.snapshot.project.clone();
    let LaneData::Audio(r) = &mut p.contents[0].lanes[0].data else { panic!("audio") };
    r.sample_rate = Some(44100);
    assert!(manifest(&p, &[prepared.assets[0].meta.clone()]).is_err());
}

#[test]
fn local_storage_failures_and_finalize_collisions_never_overwrite_existing_files() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("existing file");
    std::fs::write(&sentinel, b"keep me").unwrap();
    let (project, pool) = source();
    assert!(prepare(&project, &pool, Some(&sentinel), &stop()).is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep me");
    let folder = staging(Some(root.path()), true).unwrap().unwrap();
    let path = folder.path().to_path_buf();
    std::fs::write(path.join("assets.json"), b"do not overwrite").unwrap();
    assert!(finish(Some(&folder), &project, &[], &stop()).is_err());
    assert_eq!(std::fs::read(path.join("assets.json")).unwrap(), b"do not overwrite");
    drop(folder);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep me");
}

#[test]
fn many_small_files_keep_progress_bounded_so_the_connected_event_has_queue_space() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut all = vec![];
    let mut streams = vec![];
    for i in 0..300 {
        let data = AudioData { sample_rate: 8000, channels: 1, samples: vec![i as f32 * 0.001; 1 + i % 3] };
        let asset = encode_take(&data, Some(host.path()), &stop()).unwrap();
        streams.push(download(&asset, i, 0, true));
        all.push(asset.meta.clone());
    }
    let total: usize = all.iter().map(|a| a.bytes).sum();
    let mut input = pages(&all);
    for stream in streams {
        input.extend(stream);
    }
    let mut updates = vec![];
    let mut io = script(input);
    let received = receive(&mut io, hostile_project(&all), 0, Some(root.path()), &stop(), |done, total| {
        updates.push((done, total))
    })
    .unwrap();
    assert_eq!(received.audio.files.len(), 300);
    assert!(updates.len() <= 18);
    assert_eq!(updates.last(), Some(&(total, total)));
    assert_eq!(requests(&io.output).0.len(), 300);
}

#[test]
fn live_assets_refuse_cumulative_budgets_and_changed_known_metadata_before_writing() {
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(root.path()), &stop()).unwrap();
    let asset = &prepared.assets[0];
    let mut known = Known::new();
    for n in 0..MAX_SOUNDS {
        let mut meta = asset.meta.clone();
        meta.hash = format!("{n:064x}");
        known.insert(meta.path(), meta);
    }
    let before = std::fs::read_dir(root.path()).unwrap().count();
    let mut io = script(pages(std::slice::from_ref(&asset.meta)));
    assert!(
        receive_known(&mut io, prepared.snapshot.project.clone(), 0, Some(root.path()), &stop(), &known, |_, _| {})
            .is_err()
    );
    assert!(io.output.is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), before);
    known.clear();
    known.insert(asset.meta.path(), asset.meta.clone());
    let mut changed = prepared.snapshot.project.clone();
    let LaneData::Audio(region) = &mut changed.contents[0].lanes[0].data else { panic!("audio") };
    region.channels = Some(2);
    assert!(missing(&changed, &known).is_err());
    let missing = missing(&prepared.snapshot.project, &known).unwrap();
    assert!(references(&missing).is_empty());
}

#[test]
fn reverse_take_upload_verifies_chunks_hash_and_metadata_and_resumes_a_cut_upload() {
    let guest = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let data = Arc::new(sine(8000, 1, 12000));
    let encoded = encode_take(&data, Some(guest.path()), &stop()).unwrap();
    let full = upload(&encoded, 0, true);
    // Cut in the middle: refused, nothing collected, the verified prefix kept for a retry.
    let mut io = script(full[..boundary(&full, 3, 100)].to_vec());
    assert!(receive_take(&mut io, &encoded.meta, Some(root.path()), &stop()).is_err());
    assert_eq!(sessions(root.path()), 0);
    let mut sent = Cursor::new(io.output);
    assert!(matches!(protocol::read(&mut sent).unwrap(), ServerMessage::UploadRequest { offset: 0, tree: true, .. }));
    let mut io = script(upload(&encoded, 3 * CHUNK, false));
    let (asset, audio) = receive_take(&mut io, &encoded.meta, Some(root.path()), &stop()).unwrap();
    let mut sent = Cursor::new(io.output);
    assert!(matches!(
        protocol::read(&mut sent).unwrap(),
        ServerMessage::UploadRequest { offset, tree: false, .. } if offset == 3 * CHUNK
    ));
    assert_eq!(std::fs::read(&asset.path).unwrap(), std::fs::read(&encoded.path).unwrap());
    assert_eq!(asset.tree, encoded.tree);
    assert_eq!(&**audio.audio.files.values().next().unwrap(), &*data);
    let path = audio.folder().unwrap().to_path_buf();
    assert!(path.join(encoded.meta.path()).is_file());
    drop(audio);
    assert!(!path.exists());
    // Already cached on the host: no upload is requested at all.
    let mut io = script(vec![]);
    let (again, _audio) = receive_take(&mut io, &encoded.meta, Some(root.path()), &stop()).unwrap();
    assert!(io.output.is_empty());
    assert_eq!(again.tree, encoded.tree);
    // Altered bytes, another sound's metadata and invalid names are refused.
    let other = tempfile::tempdir().unwrap();
    let mut bytes = std::fs::read(&encoded.path).unwrap();
    bytes[CHUNK + 100] ^= 1;
    let altered = guest.path().join("altered.wav");
    std::fs::write(&altered, &bytes).unwrap();
    let forged = Encoded {
        meta: encoded.meta.clone(),
        path: altered,
        tree: encoded.tree.clone(),
        _lease: Arc::clone(&encoded._lease),
    };
    assert!(receive_take(&mut script(upload(&forged, 0, true)), &encoded.meta, Some(other.path()), &stop()).is_err());
    let mut meta = encoded.meta.clone();
    meta.channels = 2;
    meta.frames /= 2;
    assert!(receive_take(&mut script(upload(&encoded, 0, true)), &meta, Some(other.path()), &stop()).is_err());
    let mut invalid = encoded.meta.clone();
    invalid.hash = "../outside".into();
    let mut io = script(vec![]);
    assert!(receive_take(&mut io, &invalid, Some(other.path()), &stop()).is_err());
    assert!(io.output.is_empty());
    assert!(
        receive_take(&mut script(upload(&encoded, 0, true)), &encoded.meta, Some(other.path()), &AtomicBool::new(true))
            .is_err()
    );
    assert_eq!(sessions(other.path()), 0);
}

/// A real TCP pair, so the host and guest run their sides of a transfer at the same time.
struct Pipe {
    stream: TcpStream,
    /// Bytes this side may still read before the connection "drops".
    budget: usize,
    sent: Vec<u8>,
}
impl Read for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.budget == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "cut"));
        }
        let n = bytes.len().min(self.budget);
        let n = self.stream.read(&mut bytes[..n])?;
        self.budget -= n;
        Ok(n)
    }
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.stream.write(bytes)?;
        self.sent.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
impl Link for Pipe {
    fn set_deadline(&mut self, deadline: Instant) {
        let left = deadline.saturating_duration_since(Instant::now()).max(std::time::Duration::from_millis(1));
        self.stream.set_read_timeout(Some(left)).unwrap();
    }
}
fn pipes(guest_budget: usize) -> (Pipe, Pipe) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let guest = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (host, _) = listener.accept().unwrap();
    (
        Pipe { stream: host, budget: usize::MAX, sent: vec![] },
        Pipe { stream: guest, budget: guest_budget, sent: vec![] },
    )
}

#[test]
fn a_multi_chunk_sound_streams_with_bounded_buffers_and_resumes_after_a_cut_connection() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    // About 2 MiB: hundreds of chunks at the test chunk size.
    let (project, pool) = source_with(sine(48000, 2, 2 * 262_144));
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    let assets = prepared.assets.clone();
    let shared = prepared.snapshot.project.clone();
    assert!(sound::leaves(assets[0].meta.bytes) > 500);
    let run = |budget: usize| {
        let (mut a, mut b) = pipes(budget);
        let (served, offered) = (assets.clone(), shared.clone());
        let server = std::thread::spawn(move || send(&mut a, &offered, &served, &AtomicBool::new(false)));
        let result = receive(&mut b, shared.clone(), 0, Some(root.path()), &stop(), |_, _| {});
        drop(b.stream);
        let _ = server.join().unwrap();
        (result, requests(&b.sent))
    };
    let (result, (asked, done)) = run(assets[0].meta.bytes / 3);
    assert!(result.is_err());
    assert_eq!((asked, done), (vec![(0, 0, true)], false));
    let part = cache_dir(root.path()).join(format!("{}.part", assets[0].meta.hash));
    let kept = std::fs::metadata(&part).unwrap().len() as usize;
    assert!(kept > 0 && kept.is_multiple_of(CHUNK));
    let (result, (asked, done)) = run(usize::MAX);
    assert_eq!((asked, done), (vec![(0, kept, false)], true));
    let received = result.unwrap();
    assert_eq!(received.audio.files.values().next().unwrap().as_ref(), pool.files.values().next().unwrap().as_ref());
    // And once more: everything comes from the cache.
    let (result, (asked, done)) = run(usize::MAX);
    assert!(result.is_ok());
    assert_eq!((asked, done), (vec![], true));
}

#[test]
fn a_host_refuses_repeated_or_unknown_download_requests() {
    let host = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &stop()).unwrap();
    for requests in [
        vec![(0, 0, true), (0, 0, true)],
        vec![(1, 0, true)],
        vec![(0, 1, true)],
        vec![(0, prepared.assets[0].meta.bytes, true)],
    ] {
        let mut input = vec![];
        for (asset, offset, tree) in requests {
            protocol::write(&mut input, &ClientMessage::AssetDownload { protocol: PROTOCOL, asset, offset, tree })
                .unwrap();
        }
        let mut io = script(input);
        assert!(send(&mut io, &prepared.snapshot.project, &prepared.assets, &stop()).is_err());
    }
}
