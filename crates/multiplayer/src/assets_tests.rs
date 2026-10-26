// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn canonical_float_wave_preserves_samples_and_refuses_bad_headers() {
    let data = AudioData { sample_rate: 8000, channels: 2, samples: vec![0.1, -0.25, 0.3, 0.5] };
    let bytes = wave(&data).unwrap();
    let asset = Asset { hash: hash(&bytes), bytes: bytes.len(), rate: 8000, channels: 2, frames: 2 };
    assert_eq!(decode(&bytes, &asset).unwrap(), data);
    let mut bad = bytes.clone();
    bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut forged = asset.clone();
    forged.hash = hash(&bad);
    assert!(decode(&bad, &forged).is_err());
    let mut forged = asset.clone();
    forged.frames = usize::MAX;
    assert!(decode(&bytes, &forged).is_err());
}

fn source() -> (Project, AudioPool) {
    let mut project = crate::starter_project();
    project.tracks[0].content = suhina_model::ContentKinds { notes: false, audio: true };
    project.contents[0].lanes[0].data = LaneData::Audio(suhina_model::AudioRegion::new("/private/original.wav"));
    let mut pool = AudioPool::new();
    pool.files.insert(
        "/private/original.wav".into(),
        Arc::new(AudioData {
            sample_rate: 8000,
            channels: 1,
            samples: (0..12000).map(|i| (i as f32 * 0.1).sin() * 0.25).collect(),
        }),
    );
    (project, pool)
}
struct Script {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
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
fn script(metas: Vec<Asset>, bytes: &[u8], wrong_offset: bool) -> Script {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut input = vec![];
    protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, manifest: metas }).unwrap();
    for (i, chunk) in bytes.chunks(CHUNK).enumerate() {
        protocol::write(
            &mut input,
            &ServerMessage::AssetChunk {
                protocol: PROTOCOL,
                asset: 0,
                offset: if wrong_offset { 1 } else { i * CHUNK },
                data: STANDARD.encode(chunk),
            },
        )
        .unwrap();
    }
    Script { input: Cursor::new(input), output: vec![] }
}
#[test]
fn portable_copy_deduplicates_audio_preserves_original_and_cleans_unclaimed_folders() {
    let root = tempfile::tempdir().unwrap();
    let (mut project, pool) = source();
    project.contents[1].lanes[0].data = project.contents[0].lanes[0].data.clone();
    project.tracks[1].content = suhina_model::ContentKinds { notes: false, audio: true };
    let before = project.clone();
    let prepared = prepare(&project, &pool, Some(root.path()), &AtomicBool::new(false)).unwrap();
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
}
#[test]
fn invalid_manifests_and_path_attacks_fail_before_any_file_or_download() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &AtomicBool::new(false)).unwrap();
    let good = prepared.assets[0].meta.clone();
    let mut invalid = vec![vec![], vec![good.clone(), good.clone()]];
    for (field, value) in [
        ("hash", serde_json::json!("../../escape")),
        ("bytes", serde_json::json!(MAX_ASSET + 1)),
        ("frames", serde_json::json!(usize::MAX)),
        ("channels", serde_json::json!(0)),
        ("rate", serde_json::json!(0)),
    ] {
        let mut meta = serde_json::to_value(&good).unwrap();
        meta[field] = value;
        invalid.push(vec![serde_json::from_value(meta).unwrap()]);
    }
    for metas in invalid {
        let mut io = script(metas, &[], false);
        assert!(
            receive(
                &mut io,
                prepared.snapshot.project.clone(),
                0,
                Some(root.path()),
                &AtomicBool::new(false),
                |_, _| {}
            )
            .is_err()
        );
        assert!(io.output.is_empty());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    for path in ["../outside.wav", "Audio/../x.wav", "C:/outside.wav", "Audio/"] {
        assert!(!valid_path(path));
    }
}
#[test]
fn truncated_wrong_hash_wrong_offset_and_cancelled_transfers_leave_no_partial_project() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &AtomicBool::new(false)).unwrap();
    let asset = &prepared.assets[0];
    let mut corrupt = asset.bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    for (bytes, wrong_offset, stopped) in [
        (&asset.bytes[..asset.bytes.len() - 4], false, false),
        (corrupt.as_slice(), false, false),
        (asset.bytes.as_slice(), true, false),
        (asset.bytes.as_slice(), false, true),
    ] {
        let mut io = script(vec![asset.meta.clone()], bytes, wrong_offset);
        assert!(
            receive(
                &mut io,
                prepared.snapshot.project.clone(),
                0,
                Some(root.path()),
                &AtomicBool::new(stopped),
                |_, _| {}
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
#[test]
fn verified_audio_is_persistent_only_after_the_recipient_retains_the_snapshot() {
    let host = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(host.path()), &AtomicBool::new(false)).unwrap();
    let asset = &prepared.assets[0];
    let mut io = script(vec![asset.meta.clone()], &asset.bytes, false);
    let received =
        receive(&mut io, prepared.snapshot.project.clone(), 7, Some(root.path()), &AtomicBool::new(false), |_, _| {})
            .unwrap();
    let folder = received.folder().unwrap().to_path_buf();
    let mut session = received.session();
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
}
#[test]
fn missing_or_unsupported_loaded_sound_does_not_touch_sources_or_start_storage() {
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let before = project.clone();
    assert!(prepare(&project, &AudioPool::new(), Some(root.path()), &AtomicBool::new(false)).is_err());
    for channels in [0, 3] {
        let mut bad = pool.clone();
        let mut data = bad.files["/private/original.wav"].as_ref().clone();
        data.channels = channels;
        bad.files.insert("/private/original.wav".into(), Arc::new(data));
        assert!(prepare(&project, &bad, Some(root.path()), &AtomicBool::new(false)).is_err());
    }
    assert_eq!(project, before);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn total_manifest_quota_is_checked_before_disk_or_audio_allocation() {
    let root = tempfile::tempdir().unwrap();
    let (mut project, _) = source();
    let mut metas = vec![];
    project.contents.clear();
    for i in 0..5 {
        let meta = Asset {
            hash: format!("{i:064x}"),
            bytes: MAX_ASSET,
            rate: 8000,
            channels: 1,
            frames: (MAX_ASSET - 68) / 4,
        };
        let mut region = suhina_model::AudioRegion::new(meta.path());
        region.sample_rate = Some(meta.rate);
        region.channels = Some(1);
        region.duration_seconds = Some(meta.frames as f64 / 8000.0);
        project.contents.push(suhina_model::ClipContent {
            id: suhina_model::ContentId(1000 + i),
            name: "Audio".into(),
            length: 4.0,
            lanes: vec![suhina_model::ContentLane { target: None, data: LaneData::Audio(region) }],
            extensions: Default::default(),
            scale: None,
        });
        metas.push(meta);
    }
    let mut io = script(metas, &[], false);
    let error = receive(&mut io, project, 0, Some(root.path()), &AtomicBool::new(false), |_, _| {}).err().unwrap();
    assert!(error.contains("256 MiB"));
    assert!(io.output.is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn portable_document_rejects_paths_and_metadata_beyond_the_manifest() {
    let root = tempfile::tempdir().unwrap();
    let (project, pool) = source();
    let prepared = prepare(&project, &pool, Some(root.path()), &AtomicBool::new(false)).unwrap();
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
    assert!(prepare(&project, &pool, Some(&sentinel), &AtomicBool::new(false)).is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep me");
    let folder = staging(Some(root.path()), true).unwrap().unwrap();
    let path = folder.path().to_path_buf();
    std::fs::write(path.join("assets.json"), b"do not overwrite").unwrap();
    assert!(finish(Some(&folder), &project, &[], &AtomicBool::new(false)).is_err());
    assert_eq!(std::fs::read(path.join("assets.json")).unwrap(), b"do not overwrite");
    drop(folder);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep me");
}

#[test]
fn many_small_files_keep_progress_bounded_so_the_connected_event_has_queue_space() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let root = tempfile::tempdir().unwrap();
    let (mut project, _) = source();
    project.contents.clear();
    let mut all = vec![];
    let mut chunks = vec![];
    for i in 0..64 {
        let bytes = wave(&AudioData { sample_rate: 8000, channels: 1, samples: vec![i as f32 * 0.001] }).unwrap();
        let meta = Asset { hash: hash(&bytes), bytes: bytes.len(), rate: 8000, channels: 1, frames: 1 };
        let mut region = suhina_model::AudioRegion::new(meta.path());
        region.sample_rate = Some(8000);
        region.channels = Some(1);
        region.duration_seconds = Some(1.0 / 8000.0);
        project.contents.push(suhina_model::ClipContent {
            id: suhina_model::ContentId(1000 + i),
            name: "Audio".into(),
            length: 4.0,
            lanes: vec![suhina_model::ContentLane { target: None, data: LaneData::Audio(region) }],
            extensions: Default::default(),
            scale: None,
        });
        all.push(meta);
        chunks.push(bytes);
    }
    let total: usize = all.iter().map(|a| a.bytes).sum();
    let mut input = vec![];
    protocol::write(&mut input, &ServerMessage::Assets { protocol: PROTOCOL, manifest: all }).unwrap();
    for (i, bytes) in chunks.iter().enumerate() {
        protocol::write(
            &mut input,
            &ServerMessage::AssetChunk { protocol: PROTOCOL, asset: i, offset: 0, data: STANDARD.encode(bytes) },
        )
        .unwrap();
    }
    let mut io = Script { input: Cursor::new(input), output: vec![] };
    let mut updates = vec![];
    let received = receive(&mut io, project, 0, Some(root.path()), &AtomicBool::new(false), |done, total| {
        updates.push((done, total))
    })
    .unwrap();
    assert_eq!(received.audio.files.len(), 64);
    assert!(updates.len() <= 18);
    assert_eq!(updates.last(), Some(&(total, total)));
}
