// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Collected, content-addressed audio copies. No peer-supplied source path reaches filesystem I/O.
//! Sounds are streamed from and to disk in fixed-size chunks ([`crate::sound`]) and kept in a
//! per-computer cache ([`crate::cache`]), so a sound already received is never sent again.
use crate::cache::{Cache, link_or_copy};
use crate::protocol::{self, ClientMessage, PROTOCOL, ServerMessage};
use crate::sound::{self, Incoming, Source};
use crate::transport::Link;
use blake3::hazmat::ChainingValue;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use suhina_engine::{AudioData, AudioPool};
use suhina_model::{LaneData, Project};

// Hard caps on shared sounds, checked before any allocation, disk reservation or download.
/// Distinct sounds in one session, across the initial copy and every take published later.
pub const MAX_SOUNDS: usize = 4096;
/// Bytes of one canonical WAV copy.
pub const MAX_ASSET: usize = 2 << 30;
/// Bytes of all distinct sounds in one session.
pub const MAX_TOTAL: u64 = 16 << 30;
/// Bytes the sound cache on each computer keeps before removing the least recently used sounds.
pub const CACHE_CAP: u64 = 20 << 30;
/// Bytes per transfer chunk; every chunk is verified against the sound's BLAKE3 tree. Part of
/// the wire format: both sides of a session use the same value.
#[cfg(not(test))]
pub(crate) const CHUNK: usize = 1 << 20;
/// Unit tests use small chunks, so small sounds still cross many chunks.
#[cfg(test)]
pub(crate) const CHUNK: usize = 4096;
/// Manifest entries per frame.
pub(crate) const PAGE: usize = 128;
/// The longest wait for one chunk or request before a transfer is considered cut.
pub(crate) const STALL: Duration = Duration::from_secs(60);

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
fn stopped(stop: &AtomicBool) -> Result<(), String> {
    if stop.load(Ordering::Relaxed) { Err("session stopped".into()) } else { Ok(()) }
}
fn too_much() -> String {
    format!("shared audio exceeds {} GiB", MAX_TOTAL >> 30)
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
/// A manifest entry describes exactly one canonical float WAV copy within the caps.
pub(crate) fn validate_asset(a: &Asset) -> Result<(), String> {
    let expected = a
        .frames
        .checked_mul(usize::from(a.channels))
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(sound::HEADER));
    if !valid_path(&a.path())
        || a.bytes > MAX_ASSET
        || !(8000..=192000).contains(&a.rate)
        || !(1..=2).contains(&a.channels)
        || a.frames == 0
        || expected != Some(a.bytes)
    {
        return Err("invalid audio manifest entry".into());
    }
    Ok(())
}
fn manifest(project: &Project, assets: &[Asset]) -> Result<(), String> {
    if assets.len() > MAX_SOUNDS {
        return Err("too many shared audio files".into());
    }
    let mut by_path = BTreeMap::new();
    let mut total = 0u64;
    for a in assets {
        validate_asset(a)?;
        if by_path.insert(a.path(), a).is_some() {
            return Err("repeated asset".into());
        }
        total = total.checked_add(a.bytes as u64).filter(|n| *n <= MAX_TOTAL).ok_or_else(too_much)?;
    }
    if by_path.keys().cloned().collect::<BTreeSet<_>>() != references(project) {
        return Err("audio manifest does not match the project's references".into());
    }
    for content in &project.contents {
        for lane in &content.lanes {
            if let LaneData::Audio(region) = &lane.data {
                if region.file.is_empty() {
                    continue;
                }
                let a = by_path.get(&region.file).ok_or("missing audio asset")?;
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

/// A canonical sound file this computer can send, with its chunk tree. The file lives in a
/// private folder leased for as long as any session still refers to it.
pub(crate) struct Encoded {
    pub meta: Asset,
    pub path: PathBuf,
    pub tree: Vec<ChainingValue>,
    _lease: Arc<tempfile::TempDir>,
}
impl Encoded {
    fn source(&self) -> Source<'_> {
        Source { meta: &self.meta, path: &self.path, tree: &self.tree }
    }
}
pub(crate) struct Prepared {
    pub snapshot: SharedSnapshot,
    pub assets: Vec<Arc<Encoded>>,
}
fn storage(root: Option<&Path>) -> Result<&Path, String> {
    root.filter(|p| p.is_absolute()).ok_or_else(|| "choose an application data folder before sharing audio".into())
}
fn staging(root: Option<&Path>, needed: bool) -> Result<Option<tempfile::TempDir>, String> {
    if !needed {
        return Ok(None);
    }
    let root = storage(root)?;
    std::fs::create_dir_all(root).map_err(|e| format!("cannot create shared project storage: {e}"))?;
    let dir = tempfile::Builder::new().prefix("session-").tempdir_in(root).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir.path().join("Audio")).map_err(|e| e.to_string())?;
    Ok(Some(dir))
}
/// A private folder for the sounds this computer serves during one session.
fn outgoing(root: Option<&Path>) -> Result<Arc<tempfile::TempDir>, String> {
    let root = storage(root)?;
    std::fs::create_dir_all(root).map_err(|e| format!("cannot create shared project storage: {e}"))?;
    let dir = tempfile::Builder::new().prefix("outgoing-").tempdir_in(root).map_err(|e| e.to_string())?;
    Ok(Arc::new(dir))
}
/// Write loaded audio's canonical copy into a leased folder, streamed and hashed on the way.
fn encode_into(lease: &Arc<tempfile::TempDir>, data: &AudioData, stop: &AtomicBool) -> Result<Encoded, String> {
    let mut temp = tempfile::NamedTempFile::new_in(lease.path()).map_err(|e| e.to_string())?;
    let (meta, tree) = {
        let mut out = BufWriter::with_capacity(CHUNK, temp.as_file_mut());
        let encoded = sound::encode(data, &mut out, stop)?;
        out.flush().map_err(|e| e.to_string())?;
        encoded
    };
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let path = lease.path().join(format!("{}.wav", meta.hash));
    if std::fs::symlink_metadata(&path).is_err() {
        temp.persist_noclobber(&path).map_err(|e| e.error.to_string())?;
    }
    Ok(Encoded { meta, path, tree, _lease: Arc::clone(lease) })
}
fn finish(
    folder: Option<&tempfile::TempDir>,
    project: &Project,
    assets: &[Asset],
    stop: &AtomicBool,
) -> Result<(), String> {
    stopped(stop)?;
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
    let mut total = 0u64;
    let mut source_names = BTreeMap::<String, Asset>::new();
    let mut lease: Option<Arc<tempfile::TempDir>> = None;
    for content in &mut project.contents {
        for lane in &mut content.lanes {
            if let LaneData::Audio(region) = &mut lane.data {
                if region.file.is_empty() {
                    region.absolute_file = None;
                    continue;
                }
                stopped(stop)?;
                let data = pool
                    .files
                    .get(&region.file)
                    .ok_or("a project sound is not loaded; load or relink it in Studio first")?;
                let meta = if let Some(meta) = source_names.get(&region.file) {
                    meta.clone()
                } else {
                    if source_names.len() >= MAX_SOUNDS {
                        return Err(format!("at most {MAX_SOUNDS} source sounds can be shared"));
                    }
                    sound::audio_valid(data)?;
                    // Refuse before writing anything that would exceed the session budget.
                    let estimate = (data.samples.len() as u64).saturating_mul(4).saturating_add(sound::HEADER as u64);
                    if total.saturating_add(estimate) > MAX_TOTAL {
                        return Err(too_much());
                    }
                    let lease = match &lease {
                        Some(lease) => Arc::clone(lease),
                        None => Arc::clone(lease.insert(outgoing(root)?)),
                    };
                    let asset = encode_into(&lease, data, stop)?;
                    let meta = asset.meta.clone();
                    if !encoded.contains_key(&meta.hash) {
                        total += meta.bytes as u64;
                        encoded.insert(meta.hash.clone(), Arc::new(asset));
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
            stopped(stop)?;
            link_or_copy(&asset.path, &dir.path().join(asset.meta.path()))?;
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
fn known_bytes(known: &Known) -> u64 {
    known.values().map(|a| a.bytes as u64).fold(0, u64::saturating_add)
}
pub(crate) fn remember_take(known: &mut Known, meta: &Asset) -> Result<bool, String> {
    validate_asset(meta)?;
    if let Some(old) = known.get(&meta.path()) {
        return if old == meta { Ok(false) } else { Err("known sound metadata changed".into()) };
    }
    if known.len() >= MAX_SOUNDS || known_bytes(known).saturating_add(meta.bytes as u64) > MAX_TOTAL {
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

/// The canonical copy of a finished take, written to a private folder under the storage root.
pub(crate) fn encode_take(data: &AudioData, root: Option<&Path>, stop: &AtomicBool) -> Result<Arc<Encoded>, String> {
    sound::audio_valid(data)?;
    Ok(Arc::new(encode_into(&outgoing(root)?, data, stop)?))
}

/// Collect this computer's own take: the decoded audio it came from and a link to its copy.
pub(crate) fn collect_take(
    asset: &Encoded,
    data: Arc<AudioData>,
    root: Option<&Path>,
    stop: &AtomicBool,
) -> Result<AudioAddition, String> {
    let folder = staging(root, true)?;
    let dir = folder.as_ref().ok_or("audio storage missing")?;
    link_or_copy(&asset.path, &dir.path().join(asset.meta.path()))?;
    stopped(stop)?;
    let mut audio = AudioPool::new();
    audio.files.insert(asset.meta.path(), data);
    Ok(AudioAddition { audio, folder })
}

/// A cached sound when its contents still verify; a corrupt copy is removed.
fn cached(cache: &Cache, meta: &Asset, stop: &AtomicBool) -> Result<Option<(PathBuf, AudioData)>, String> {
    let Some(path) = cache.find(meta) else { return Ok(None) };
    match sound::decode(&path, meta, stop) {
        Ok(data) => Ok(Some((path, data))),
        Err(_) => {
            stopped(stop)?;
            cache.discard(meta);
            Ok(None)
        }
    }
}
fn decode_received(cache: &Cache, path: &Path, meta: &Asset, stop: &AtomicBool) -> Result<AudioData, String> {
    sound::decode(path, meta, stop).inspect_err(|_| {
        if !stop.load(Ordering::Relaxed) {
            cache.discard(meta);
        }
    })
}

/// Pull a single offered sound from an authenticated guest, through this computer's cache: an
/// already cached sound is not requested, and a cut upload resumes from its last verified chunk.
pub(crate) fn receive_take(
    io: &mut impl Link,
    meta: &Asset,
    root: Option<&Path>,
    stop: &AtomicBool,
) -> Result<(Arc<Encoded>, AudioAddition), String> {
    validate_asset(meta)?;
    let root = storage(root)?;
    let cache = Cache::open(root, CACHE_CAP)?;
    let folder = staging(Some(root), true)?;
    let dir = folder.as_ref().ok_or("audio storage missing")?;
    let (path, data) = match cached(&cache, meta, stop)? {
        Some(found) => found,
        None => {
            let path = sound::receive(
                io,
                meta,
                &cache,
                |io, offset, tree| {
                    protocol::write(io, &ServerMessage::UploadRequest { protocol: PROTOCOL, offset, tree })
                },
                Incoming {
                    tree: |m: ClientMessage| match m {
                        ClientMessage::UploadTree { protocol: PROTOCOL, leaves } => Some(leaves),
                        _ => None,
                    },
                    chunk: |m: ClientMessage| match m {
                        ClientMessage::UploadChunk { protocol: PROTOCOL, offset, len } => Some((offset, len)),
                        _ => None,
                    },
                },
                stop,
                |_| {},
            )?;
            let data = decode_received(&cache, &path, meta, stop)?;
            (path, data)
        }
    };
    cache.touch(&path);
    link_or_copy(&path, &dir.path().join(meta.path()))?;
    let tree = sound::tree_of(&path, meta, stop)?;
    let lease = outgoing(Some(root))?;
    let served = lease.path().join(format!("{}.wav", meta.hash));
    link_or_copy(&path, &served)?;
    let _ = cache.trim(&BTreeSet::from([meta.hash.clone()]));
    let mut audio = AudioPool::new();
    audio.files.insert(meta.path(), Arc::new(data));
    Ok((Arc::new(Encoded { meta: meta.clone(), path: served, tree, _lease: lease }), AudioAddition { audio, folder }))
}

/// Answer the host's pull until its terminal State. No source path is transmitted or opened.
pub(crate) fn send_take(
    io: &mut impl Link,
    asset: &Encoded,
    stop: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<ServerMessage, String> {
    let bytes = asset.meta.bytes;
    let quantum = bytes.div_ceil(16).max(CHUNK);
    progress(0, bytes);
    let mut sent = false;
    loop {
        stopped(stop)?;
        match protocol::read(io)? {
            ServerMessage::UploadRequest { protocol: PROTOCOL, offset, tree } if !sent => {
                sent = true;
                let mut reported = offset;
                sound::send(
                    io,
                    &asset.source(),
                    (offset, tree),
                    |leaves| ClientMessage::UploadTree { protocol: PROTOCOL, leaves },
                    |offset, len| ClientMessage::UploadChunk { protocol: PROTOCOL, offset, len },
                    stop,
                    |done| {
                        if done == bytes || done - reported >= quantum {
                            progress(done, bytes);
                            reported = done;
                        }
                    },
                )?;
            }
            value @ ServerMessage::State { .. } => return Ok(value),
            _ => return Err("invalid host upload response".into()),
        }
    }
}

/// Offer the sounds a project refers to, then serve each requested sound once from the asked
/// offset until the guest says it has everything.
pub(crate) fn send(
    io: &mut impl Link,
    project: &Project,
    assets: &[Arc<Encoded>],
    stop: &AtomicBool,
) -> Result<(), String> {
    let refs = references(project);
    let selected: Vec<_> = assets.iter().filter(|a| refs.contains(&a.meta.path())).collect();
    let metas: Vec<_> = selected.iter().map(|a| a.meta.clone()).collect();
    manifest(project, &metas)?;
    let total = metas.len();
    if total == 0 {
        return protocol::write(io, &ServerMessage::Assets { protocol: PROTOCOL, total, manifest: Vec::new() });
    }
    for page in metas.chunks(PAGE) {
        protocol::write(io, &ServerMessage::Assets { protocol: PROTOCOL, total, manifest: page.to_vec() })?;
    }
    let mut served = vec![false; total];
    loop {
        stopped(stop)?;
        io.set_deadline(Instant::now() + STALL);
        match protocol::read::<ClientMessage>(io)? {
            ClientMessage::AssetDownload { protocol: PROTOCOL, asset, offset, tree }
                if served.get(asset) == Some(&false) =>
            {
                if let Some(flag) = served.get_mut(asset) {
                    *flag = true;
                }
                let item = selected.get(asset).ok_or("invalid audio download request")?;
                sound::send(
                    io,
                    &item.source(),
                    (offset, tree),
                    |leaves| ServerMessage::AssetTree { protocol: PROTOCOL, asset, leaves },
                    |offset, len| ServerMessage::AssetChunk { protocol: PROTOCOL, asset, offset, len },
                    stop,
                    |_| {},
                )?;
            }
            ClientMessage::AssetsDone { protocol: PROTOCOL } => return Ok(()),
            _ => return Err("invalid audio download request".into()),
        }
    }
}
fn read_page(io: &mut impl Link) -> Result<(usize, Vec<Asset>), String> {
    io.set_deadline(Instant::now() + STALL);
    let ServerMessage::Assets { protocol: PROTOCOL, total, manifest } = protocol::read(io)? else {
        return Err("missing audio manifest".into());
    };
    if total > MAX_SOUNDS || manifest.len() > PAGE || manifest.len() > total {
        return Err("too many shared audio files".into());
    }
    Ok((total, manifest))
}
fn read_manifest(io: &mut impl Link) -> Result<Vec<Asset>, String> {
    let (total, first) = read_page(io)?;
    if total > 0 && first.is_empty() {
        return Err("empty audio manifest page".into());
    }
    let mut metas = Vec::with_capacity(total);
    metas.extend(first);
    while metas.len() < total {
        let (again, page) = read_page(io)?;
        if again != total || page.is_empty() || metas.len() + page.len() > total {
            return Err("inconsistent audio manifest pages".into());
        }
        metas.extend(page);
    }
    Ok(metas)
}
pub(crate) fn receive(
    io: &mut impl Link,
    project: Project,
    revision: u64,
    root: Option<&Path>,
    stop: &AtomicBool,
    progress: impl FnMut(usize, usize),
) -> Result<SharedSnapshot, String> {
    receive_known(io, project, revision, root, stop, &Known::new(), progress)
}

/// Receive the sounds a project refers to: each one from this computer's cache when it is there
/// and still verifies, otherwise requested from the host from the last verified chunk of any
/// earlier partial copy. Only sounds the cache lacks cross the network.
pub(crate) fn receive_known(
    io: &mut impl Link,
    project: Project,
    revision: u64,
    root: Option<&Path>,
    stop: &AtomicBool,
    known: &Known,
    mut progress: impl FnMut(usize, usize),
) -> Result<SharedSnapshot, String> {
    let metas = read_manifest(io)?;
    manifest(&project, &metas)?;
    let total = metas.iter().map(|a| a.bytes as u64).fold(0, u64::saturating_add);
    if known.len() + metas.len() > MAX_SOUNDS
        || known_bytes(known).checked_add(total).is_none_or(|n| n > MAX_TOTAL)
        || metas.iter().any(|a| known.contains_key(&a.path()))
    {
        return Err("cumulative session audio budget exceeded or repeated asset".into());
    }
    if metas.is_empty() {
        finish(None, &project, &metas, stop)?;
        return Ok(SharedSnapshot {
            project,
            revision,
            audio: AudioPool::new(),
            folder: None,
            metas,
            additions: Vec::new(),
        });
    }
    let total = usize::try_from(total).map_err(|_| too_much())?;
    let root = storage(root)?;
    let cache = Cache::open(root, CACHE_CAP)?;
    let folder = staging(Some(root), true)?;
    let dir = folder.as_ref().ok_or("audio storage missing")?.path().to_path_buf();
    let mut done = 0;
    let mut reported = 0;
    let quantum = total.div_ceil(16).max(CHUNK);
    let mut report = |added: usize, progress: &mut dyn FnMut(usize, usize)| {
        done += added;
        if done == total || done - reported >= quantum {
            progress(done, total);
            reported = done;
        }
    };
    progress(0, total);
    let mut audio = AudioPool::new();
    for (index, meta) in metas.iter().enumerate() {
        let (path, data) = match cached(&cache, meta, stop)? {
            Some(found) => {
                report(meta.bytes, &mut progress);
                found
            }
            None => {
                let path = sound::receive(
                    io,
                    meta,
                    &cache,
                    |io, offset, tree| {
                        protocol::write(
                            io,
                            &ClientMessage::AssetDownload { protocol: PROTOCOL, asset: index, offset, tree },
                        )
                    },
                    Incoming {
                        tree: |m: ServerMessage| match m {
                            ServerMessage::AssetTree { protocol: PROTOCOL, asset, leaves } if asset == index => {
                                Some(leaves)
                            }
                            _ => None,
                        },
                        chunk: |m: ServerMessage| match m {
                            ServerMessage::AssetChunk { protocol: PROTOCOL, asset, offset, len } if asset == index => {
                                Some((offset, len))
                            }
                            _ => None,
                        },
                    },
                    stop,
                    |added| report(added, &mut progress),
                )?;
                let data = decode_received(&cache, &path, meta, stop)?;
                (path, data)
            }
        };
        cache.touch(&path);
        link_or_copy(&path, &dir.join(meta.path()))?;
        audio.files.insert(meta.path(), Arc::new(data));
    }
    protocol::write(io, &ClientMessage::AssetsDone { protocol: PROTOCOL })?;
    let _ = cache.trim(&metas.iter().map(|a| a.hash.clone()).collect());
    finish(folder.as_ref(), &project, &metas, stop)?;
    Ok(SharedSnapshot { project, revision, audio, folder, metas, additions: Vec::new() })
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
