// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Collected, content-addressed audio copies. No peer-supplied source path reaches filesystem I/O.
use crate::protocol::{self, ClientMessage, PROTOCOL, ServerMessage};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use suhina_engine::{AudioData, AudioPool};
use suhina_model::{LaneData, Project};

pub const MAX_ASSET: usize = 64 * 1024 * 1024;
pub const MAX_TOTAL: usize = 256 * 1024 * 1024;
const MAX_ASSETS: usize = 64;
const CHUNK: usize = 16384;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Asset {
    pub hash: String,
    pub bytes: usize,
    pub rate: u32,
    pub channels: u16,
    pub frames: usize,
}
impl Asset {
    pub fn path(&self) -> String {
        format!("Audio/{}.wav", self.hash)
    }
}
pub fn valid_path(path: &str) -> bool {
    path.strip_prefix("Audio/")
        .and_then(|p| p.strip_suffix(".wav"))
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn audio_valid(data: &AudioData) -> Result<(), String> {
    if !(8000..=192000).contains(&data.sample_rate)
        || !(1..=2).contains(&data.channels)
        || data.samples.is_empty()
        || !data.samples.len().is_multiple_of(usize::from(data.channels))
        || data.samples.len() > (MAX_ASSET - 128) / 4
        || data.samples.iter().any(|s| !s.is_finite() || s.abs() > 16.0)
    {
        return Err("audio sharing supports finite mono/stereo 8000..192000 Hz, up to 64 MiB per sound".into());
    }
    Ok(())
}
fn wave(data: &AudioData) -> Result<Vec<u8>, String> {
    audio_valid(data)?;
    let mut out = Cursor::new(Vec::with_capacity(data.samples.len() * 4 + 128));
    {
        let spec = hound::WavSpec {
            channels: data.channels,
            sample_rate: data.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::new(&mut out, spec).map_err(|e| e.to_string())?;
        for sample in &data.samples {
            writer.write_sample(*sample).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    }
    Ok(out.into_inner())
}
fn word(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes.get(at..at + 4).ok_or("truncated WAV header")?;
    let a = <[u8; 4]>::try_from(slice).map_err(|_| "truncated WAV word")?;
    Ok(u32::from_le_bytes(a))
}
fn decode(bytes: &[u8], asset: &Asset) -> Result<AudioData, String> {
    // Bound every RIFF chunk before handing its size to hound, including hostile fmt lengths.
    if bytes.len() != asset.bytes
        || bytes.len() > MAX_ASSET
        || bytes.get(..4) != Some(b"RIFF")
        || bytes.get(8..12) != Some(b"WAVE")
        || word(bytes, 4)? as usize != bytes.len() - 8
        || hash(bytes) != asset.hash
    {
        return Err("invalid audio size, hash or WAV container".into());
    }
    let mut at = 12usize;
    let mut chunks = 0;
    let mut fmt = false;
    let mut data = false;
    while at < bytes.len() {
        chunks += 1;
        if chunks > 4 {
            return Err("too many WAV chunks".into());
        }
        let size = word(bytes, at + 4)? as usize;
        let tag = bytes.get(at..at + 4).ok_or("truncated WAV chunk")?;
        match tag {
            b"fmt " if !fmt && !data && (16..=40).contains(&size) => fmt = true,
            b"fact" if fmt && !data && size == 4 => {}
            b"data" if fmt && !data && size > 0 && size.is_multiple_of(4) => data = true,
            _ => return Err("unsupported WAV chunk".into()),
        }
        at = at
            .checked_add(8)
            .and_then(|n| n.checked_add(size))
            .and_then(|n| n.checked_add(size % 2))
            .filter(|n| *n <= bytes.len())
            .ok_or("WAV chunk exceeds the verified file")?;
    }
    if !fmt || !data {
        return Err("incomplete WAV structure".into());
    }
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Float
        || spec.bits_per_sample != 32
        || spec.sample_rate != asset.rate
        || spec.channels != asset.channels
        || reader.duration() as usize != asset.frames
    {
        return Err("audio metadata does not match its manifest".into());
    }
    let count = asset
        .frames
        .checked_mul(usize::from(asset.channels))
        .filter(|c| *c <= (MAX_ASSET - 128) / 4)
        .ok_or("decoded audio exceeds its budget")?;
    let mut samples = Vec::with_capacity(count);
    for sample in reader.samples::<f32>() {
        if samples.len() == count {
            return Err("too many WAV samples".into());
        }
        samples.push(sample.map_err(|e| e.to_string())?);
    }
    if samples.len() != count {
        return Err("incomplete audio samples".into());
    }
    let decoded = AudioData { sample_rate: asset.rate, channels: asset.channels, samples };
    if wave(&decoded)? != bytes {
        return Err("audio is not the canonical float WAV copy".into());
    }
    Ok(decoded)
}
pub(crate) fn references(project: &Project) -> BTreeSet<String> {
    project
        .contents
        .iter()
        .flat_map(|c| &c.lanes)
        .filter_map(|l| match &l.data {
            LaneData::Audio(a) if !a.file.is_empty() => Some(a.file.clone()),
            _ => None,
        })
        .collect()
}
pub(crate) fn validate_asset(a: &Asset) -> Result<(), String> {
    let expected = a.frames.checked_mul(usize::from(a.channels)).and_then(|n| n.checked_mul(4));
    if !valid_path(&a.path())
        || a.bytes == 0
        || a.bytes > MAX_ASSET
        || !(8000..=192000).contains(&a.rate)
        || !(1..=2).contains(&a.channels)
        || a.frames == 0
        || expected.is_none_or(|n| n > MAX_ASSET - 128 || n > a.bytes || a.bytes - n > 128)
    {
        return Err("invalid audio manifest entry".into());
    }
    Ok(())
}
fn manifest(project: &Project, assets: &[Asset]) -> Result<(), String> {
    if assets.len() > MAX_ASSETS {
        return Err("too many shared audio files".into());
    }
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    for a in assets {
        validate_asset(a)?;
        if !paths.insert(a.path()) {
            return Err("repeated asset".into());
        }
        total = total.checked_add(a.bytes).filter(|n| *n <= MAX_TOTAL).ok_or("shared audio exceeds 256 MiB")?;
    }
    if paths != references(project) {
        return Err("audio manifest does not match the project's references".into());
    }
    for content in &project.contents {
        for lane in &content.lanes {
            if let LaneData::Audio(region) = &lane.data {
                if region.file.is_empty() {
                    continue;
                }
                let a = assets.iter().find(|a| a.path() == region.file).ok_or("missing audio asset")?;
                if region.sample_rate != Some(a.rate)
                    || region.channels != Some(a.channels)
                    || region.duration_seconds != Some(a.frames as f64 / f64::from(a.rate))
                    || region.absolute_file.is_some()
                    || region.stream.is_some()
                {
                    return Err("audio region metadata does not match the manifest".into());
                }
            }
        }
    }
    Ok(())
}

/// A verified snapshot and a lease on its collected folder. Unused or failed snapshots clean up.
pub struct SharedSnapshot {
    pub project: Project,
    pub revision: u64,
    pub audio: AudioPool,
    folder: Option<tempfile::TempDir>,
    pub(crate) metas: Vec<Asset>,
    pub(crate) additions: Vec<AudioAddition>,
}
/// Verified additional sound files, leased until the desktop attaches them successfully.
pub struct AudioAddition {
    pub audio: AudioPool,
    folder: Option<tempfile::TempDir>,
}
impl AudioAddition {
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_ref().map(|d| d.path())
    }
    pub fn keep(mut self) {
        if let Some(folder) = self.folder.take() {
            let _ = folder.keep();
        }
    }
}
impl SharedSnapshot {
    pub(crate) fn add_audio(&mut self, audio: AudioAddition) {
        self.audio.files.extend(audio.audio.files.iter().map(|(key, data)| (key.clone(), Arc::clone(data))));
        self.additions.push(audio);
    }
    pub(crate) fn into_audio(self) -> AudioAddition {
        AudioAddition { audio: self.audio, folder: self.folder }
    }
    pub fn session(&self) -> Result<suhina_session::Session, String> {
        let mut session = suhina_session::Session::shared_copy(
            self.project.clone(),
            self.audio.clone(),
            self.folder().map(Path::to_path_buf),
        );
        for addition in &self.additions {
            session
                .add_collected_audio(&addition.audio, addition.folder().ok_or("collected audio folder missing")?)
                .map_err(|e| e.to_string())?;
        }
        Ok(session)
    }
    /// Keep collected files only after the desktop has added the new session tab successfully.
    pub fn keep(mut self) {
        for addition in self.additions.drain(..) {
            addition.keep();
        }
        if let Some(folder) = self.folder.take() {
            let _ = folder.keep();
        }
    }
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_ref().map(|d| d.path()).or_else(|| self.additions.first().and_then(AudioAddition::folder))
    }
}
pub(crate) struct Encoded {
    pub meta: Asset,
    pub bytes: Vec<u8>,
}
pub(crate) struct Prepared {
    pub snapshot: SharedSnapshot,
    pub assets: Vec<Arc<Encoded>>,
}
fn staging(root: Option<&Path>, needed: bool) -> Result<Option<tempfile::TempDir>, String> {
    if !needed {
        return Ok(None);
    }
    let root = root.filter(|p| p.is_absolute()).ok_or("choose an application data folder before sharing audio")?;
    std::fs::create_dir_all(root).map_err(|e| format!("cannot create shared project storage: {e}"))?;
    let dir = tempfile::Builder::new().prefix("session-").tempdir_in(root).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir.path().join("Audio")).map_err(|e| e.to_string())?;
    Ok(Some(dir))
}
fn finish(
    folder: Option<&tempfile::TempDir>,
    project: &Project,
    assets: &[Asset],
    stop: &AtomicBool,
) -> Result<(), String> {
    if stop.load(Ordering::Relaxed) {
        return Err("session stopped".into());
    }
    if let Some(folder) = folder {
        let file = |name: &str, bytes: &[u8]| -> Result<(), String> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(folder.path().join(name))
                .map_err(|e| e.to_string())?;
            f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
        };
        file("assets.json", &serde_json::to_vec(assets).map_err(|e| e.to_string())?)?;
        file("project.suhina.part", suhina_project_file::to_json(project).as_bytes())?;
        std::fs::rename(folder.path().join("project.suhina.part"), folder.path().join("project.suhina"))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(crate) fn prepare(
    source: &Project,
    pool: &AudioPool,
    root: Option<&Path>,
    stop: &AtomicBool,
) -> Result<Prepared, String> {
    crate::midi::validate_audio_source(source)?;
    let mut project = source.clone();
    let mut encoded = BTreeMap::<String, Arc<Encoded>>::new();
    let mut audio = AudioPool::new();
    let mut total = 0;
    let mut source_names = BTreeMap::<String, Asset>::new();
    for content in &mut project.contents {
        for lane in &mut content.lanes {
            if let LaneData::Audio(region) = &mut lane.data {
                if region.file.is_empty() {
                    region.absolute_file = None;
                    continue;
                }
                if stop.load(Ordering::Relaxed) {
                    return Err("session stopped".into());
                }
                let data = pool
                    .files
                    .get(&region.file)
                    .ok_or("a project sound is not loaded; load or relink it in Studio first")?;
                let meta = if let Some(meta) = source_names.get(&region.file) {
                    meta.clone()
                } else {
                    if source_names.len() >= MAX_ASSETS {
                        return Err("at most 64 source sounds can be shared".into());
                    }
                    let bytes = wave(data)?;
                    let digest = hash(&bytes);
                    let meta = Asset {
                        hash: digest.clone(),
                        bytes: bytes.len(),
                        rate: data.sample_rate,
                        channels: data.channels,
                        frames: data.frames(),
                    };
                    if let std::collections::btree_map::Entry::Vacant(entry) = encoded.entry(digest) {
                        total += bytes.len();
                        if total > MAX_TOTAL {
                            return Err("shared audio exceeds 256 MiB".into());
                        }
                        entry.insert(Arc::new(Encoded { meta: meta.clone(), bytes }));
                    }
                    source_names.insert(region.file.clone(), meta.clone());
                    meta
                };
                region.file = meta.path();
                region.absolute_file = None;
                region.sample_rate = Some(meta.rate);
                region.channels = Some(meta.channels);
                region.duration_seconds = Some(meta.frames as f64 / f64::from(meta.rate));
                region.stream = None;
                audio.files.insert(region.file.clone(), Arc::clone(data));
            }
        }
    }
    let (project, _) = crate::midi::encode_portable(&project)?;
    let assets: Vec<_> = encoded.into_values().collect();
    let metas: Vec<_> = assets.iter().map(|a| a.meta.clone()).collect();
    manifest(&project, &metas)?;
    let folder = staging(root, !assets.is_empty())?;
    if let Some(dir) = &folder {
        for asset in &assets {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.path().join(asset.meta.path()))
                .map_err(|e| e.to_string())?;
            f.write_all(&asset.bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        }
    }
    finish(folder.as_ref(), &project, &metas, stop)?;
    Ok(Prepared {
        snapshot: SharedSnapshot { project, revision: 0, audio, folder, metas, additions: Vec::new() },
        assets,
    })
}

/// Keep only not-yet-verified references for the existing strict manifest machinery.
pub(crate) type Known = BTreeMap<String, Asset>;
pub(crate) fn remember_take(known: &mut Known, meta: &Asset) -> Result<bool, String> {
    validate_asset(meta)?;
    if let Some(old) = known.get(&meta.path()) {
        return if old == meta { Ok(false) } else { Err("known sound metadata changed".into()) };
    }
    if known.len() >= MAX_ASSETS || known.values().map(|a| a.bytes).sum::<usize>() + meta.bytes > MAX_TOTAL {
        return Err("cumulative session audio budget exceeded".into());
    }
    known.insert(meta.path(), meta.clone());
    Ok(true)
}
pub(crate) fn missing(project: &Project, known: &Known) -> Result<Project, String> {
    let mut project = project.clone();
    for content in &mut project.contents {
        for lane in &mut content.lanes {
            if let LaneData::Audio(region) = &mut lane.data
                && let Some(meta) = known.get(&region.file)
            {
                if region.sample_rate != Some(meta.rate)
                    || region.channels != Some(meta.channels)
                    || region.duration_seconds != Some(meta.frames as f64 / f64::from(meta.rate))
                    || region.absolute_file.is_some()
                    || region.stream.is_some()
                {
                    return Err("known audio metadata changed".into());
                }
                *region = suhina_model::AudioRegion::new("");
            }
        }
    }
    Ok(project)
}

pub(crate) fn encode_take(data: &AudioData) -> Result<Arc<Encoded>, String> {
    let bytes = wave(data)?;
    Ok(Arc::new(Encoded {
        meta: Asset {
            hash: hash(&bytes),
            bytes: bytes.len(),
            rate: data.sample_rate,
            channels: data.channels,
            frames: data.frames(),
        },
        bytes,
    }))
}

pub(crate) fn collect_take(asset: &Encoded, root: Option<&Path>, stop: &AtomicBool) -> Result<AudioAddition, String> {
    let folder = staging(root, true)?;
    let dir = folder.as_ref().ok_or("audio storage missing")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.path().join(asset.meta.path()))
        .map_err(|e| e.to_string())?;
    file.write_all(&asset.bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    let mut audio = AudioPool::new();
    audio.files.insert(asset.meta.path(), Arc::new(decode(&asset.bytes, &asset.meta)?));
    if stop.load(Ordering::Relaxed) {
        return Err("session stopped".into());
    }
    Ok(AudioAddition { audio, folder })
}

/// Pull a single offered sound from an authenticated guest into a fresh private folder.
pub(crate) fn receive_take(
    io: &mut (impl Read + Write),
    meta: &Asset,
    root: Option<&Path>,
    stop: &AtomicBool,
) -> Result<(Arc<Encoded>, AudioAddition), String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    validate_asset(meta)?;
    let folder = staging(root, true)?;
    let dir = folder.as_ref().ok_or("audio storage missing")?;
    let part = dir.path().join("take.part");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(|e| e.to_string())?;
    let mut offset = 0;
    let mut digest = blake3::Hasher::new();
    while offset < meta.bytes {
        if stop.load(Ordering::Relaxed) {
            return Err("session stopped".into());
        }
        protocol::write(io, &ServerMessage::UploadRequest { protocol: PROTOCOL, offset })?;
        let ClientMessage::UploadChunk { protocol: PROTOCOL, offset: at, data } = protocol::read(io)? else {
            return Err("invalid upload chunk".into());
        };
        if at != offset || data.len() > CHUNK.div_ceil(3) * 4 {
            return Err("invalid upload offset or size".into());
        }
        let bytes = STANDARD.decode(data).map_err(|_| "invalid upload encoding")?;
        if bytes.len() != CHUNK.min(meta.bytes - offset) {
            return Err("incomplete or oversized upload".into());
        }
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        digest.update(&bytes);
        offset += bytes.len();
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if digest.finalize().to_hex().as_str() != meta.hash {
        return Err("uploaded sound hash mismatch".into());
    }
    let bytes = std::fs::read(&part).map_err(|e| e.to_string())?;
    let data = decode(&bytes, meta)?;
    std::fs::rename(&part, dir.path().join(meta.path())).map_err(|e| e.to_string())?;
    let mut audio = AudioPool::new();
    audio.files.insert(meta.path(), Arc::new(data));
    Ok((Arc::new(Encoded { meta: meta.clone(), bytes }), AudioAddition { audio, folder }))
}

/// Answer exact host pulls until its terminal State. No source path is transmitted or opened.
pub(crate) fn send_take(
    io: &mut (impl Read + Write),
    asset: &Encoded,
    stop: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<ServerMessage, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut offset = 0;
    let mut reported = 0;
    let quantum = asset.bytes.len().div_ceil(16).max(CHUNK);
    progress(0, asset.bytes.len());
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("session stopped".into());
        }
        match protocol::read(io)? {
            ServerMessage::UploadRequest { protocol: PROTOCOL, offset: at }
                if at == offset && offset < asset.bytes.len() =>
            {
                let end = (offset + CHUNK).min(asset.bytes.len());
                protocol::write(
                    io,
                    &ClientMessage::UploadChunk {
                        protocol: PROTOCOL,
                        offset,
                        data: STANDARD.encode(&asset.bytes[offset..end]),
                    },
                )?;
                offset = end;
                if offset == asset.bytes.len() || offset - reported >= quantum {
                    progress(offset, asset.bytes.len());
                    reported = offset;
                }
            }
            value @ ServerMessage::State { .. } if offset == 0 || offset == asset.bytes.len() => return Ok(value),
            _ => return Err("invalid host upload response".into()),
        }
    }
}

pub(crate) fn send(
    io: &mut (impl Read + Write),
    project: &Project,
    assets: &[Arc<Encoded>],
    stop: &AtomicBool,
) -> Result<(), String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let refs = references(project);
    let selected: Vec<_> = assets.iter().filter(|a| refs.contains(&a.meta.path())).collect();
    let metas: Vec<_> = selected.iter().map(|a| a.meta.clone()).collect();
    manifest(project, &metas)?;
    protocol::write(io, &ServerMessage::Assets { protocol: PROTOCOL, manifest: metas })?;
    for (index, asset) in selected.iter().enumerate() {
        for (chunk_index, chunk) in asset.bytes.chunks(CHUNK).enumerate() {
            if stop.load(Ordering::Relaxed) {
                return Err("session stopped".into());
            }
            let offset = chunk_index * CHUNK;
            match protocol::read::<ClientMessage>(io)? {
                ClientMessage::AssetDownload { protocol: PROTOCOL, asset: requested, offset: at }
                    if requested == index && at == offset => {}
                _ => return Err("invalid audio download request".into()),
            }
            protocol::write(
                io,
                &ServerMessage::AssetChunk { protocol: PROTOCOL, asset: index, offset, data: STANDARD.encode(chunk) },
            )?;
        }
    }
    Ok(())
}
pub(crate) fn receive(
    io: &mut (impl Read + Write),
    project: Project,
    revision: u64,
    root: Option<&Path>,
    stop: &AtomicBool,
    progress: impl FnMut(usize, usize),
) -> Result<SharedSnapshot, String> {
    receive_known(io, project, revision, root, stop, &Known::new(), progress)
}

pub(crate) fn receive_known(
    io: &mut (impl Read + Write),
    project: Project,
    revision: u64,
    root: Option<&Path>,
    stop: &AtomicBool,
    known: &Known,
    mut progress: impl FnMut(usize, usize),
) -> Result<SharedSnapshot, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let ServerMessage::Assets { protocol: PROTOCOL, manifest: metas } = protocol::read(io)? else {
        return Err("missing audio manifest".into());
    };
    manifest(&project, &metas)?;
    let total: usize = metas.iter().map(|a| a.bytes).sum();
    let old_total: usize = known.values().map(|a| a.bytes).sum();
    if known.len() + metas.len() > MAX_ASSETS
        || old_total.checked_add(total).is_none_or(|n| n > MAX_TOTAL)
        || metas.iter().any(|a| known.contains_key(&a.path()))
    {
        return Err("cumulative session audio budget exceeded or repeated asset".into());
    }
    let mut done = 0;
    let quantum = total.div_ceil(16).max(CHUNK);
    let mut reported = 0;
    if total > 0 {
        progress(0, total);
    }
    let mut audio = AudioPool::new();
    let folder = staging(root, !metas.is_empty())?;
    for (index, meta) in metas.iter().enumerate() {
        let dir = folder.as_ref().ok_or("audio storage missing")?;
        let part = dir.path().join(format!("{}.part", meta.hash));
        let mut file =
            std::fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(|e| e.to_string())?;
        let mut offset = 0;
        let mut hasher = blake3::Hasher::new();
        while offset < meta.bytes {
            if stop.load(Ordering::Relaxed) {
                return Err("session stopped".into());
            }
            protocol::write(io, &ClientMessage::AssetDownload { protocol: PROTOCOL, asset: index, offset })?;
            let ServerMessage::AssetChunk { protocol: PROTOCOL, asset, offset: at, data } = protocol::read(io)? else {
                return Err("invalid audio chunk".into());
            };
            if asset != index || at != offset || data.len() > CHUNK.div_ceil(3) * 4 {
                return Err("invalid audio chunk offset or size".into());
            }
            let bytes = STANDARD.decode(data).map_err(|_| "invalid audio chunk encoding")?;
            if bytes.len() != CHUNK.min(meta.bytes - offset) {
                return Err("incomplete or oversized audio chunk".into());
            }
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            hasher.update(&bytes);
            offset += bytes.len();
            done += bytes.len();
            if done == total || done - reported >= quantum {
                progress(done, total);
                reported = done;
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if hasher.finalize().to_hex().as_str() != meta.hash {
            return Err("audio file hash mismatch".into());
        }
        let bytes = std::fs::read(&part).map_err(|e| e.to_string())?;
        let data = decode(&bytes, meta)?;
        std::fs::rename(&part, dir.path().join(meta.path())).map_err(|e| e.to_string())?;
        audio.files.insert(meta.path(), Arc::new(data));
    }
    finish(folder.as_ref(), &project, &metas, stop)?;
    Ok(SharedSnapshot { project, revision, audio, folder, metas, additions: Vec::new() })
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
