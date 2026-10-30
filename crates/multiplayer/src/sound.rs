// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Canonical float WAV copies streamed to and from disk in fixed-size chunks. Each chunk is
//! checked against the sound's BLAKE3 tree before it is written, so no whole sound file sits in
//! memory and a cut transfer resumes from its last verified chunk.

use crate::assets::{Asset, CHUNK, MAX_ASSET, STALL, validate_asset};
use crate::cache::{Cache, Partial};
use crate::protocol;
use crate::transport::Link;
use blake3::hazmat::{ChainingValue, HasherExt, Mode, merge_subtrees_non_root, merge_subtrees_root};
use serde::{Serialize, de::DeserializeOwned};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use suhina_engine::AudioData;

/// Bytes before the samples: RIFF, a WAVE_FORMAT_EXTENSIBLE `fmt ` chunk and the `data` header.
pub(crate) const HEADER: usize = 68;
/// KSDATAFORMAT_SUBTYPE_IEEE_FLOAT.
const FLOAT: [u8; 16] = [3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71];
/// BLAKE3 subtrees must start on its 1 KiB chunk boundaries; a power of two keeps every leaf whole.
const _: () = assert!(CHUNK.is_power_of_two() && CHUNK >= 1024 && CHUNK <= 1024 * 1024);

fn stopped(stop: &AtomicBool) -> Result<(), String> {
    if stop.load(Ordering::Relaxed) { Err("session stopped".into()) } else { Ok(()) }
}

/// The only header a shared sound may have, for its rate, channels and sample bytes.
pub(crate) fn header(rate: u32, channels: u16, data: usize) -> Result<[u8; HEADER], String> {
    let data = u32::try_from(data).ok().filter(|d| d.checked_add(60).is_some()).ok_or("sound exceeds the WAV limit")?;
    let block = channels.checked_mul(4).ok_or("invalid channel count")?;
    let per_second = rate.checked_mul(u32::from(block)).ok_or("invalid sample rate")?;
    let mask = (1u32 << channels.min(18)) - 1;
    let mut out = Vec::with_capacity(HEADER);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(data + 60).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&0xfffeu16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&per_second.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&22u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&mask.to_le_bytes());
    out.extend_from_slice(&FLOAT);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    <[u8; HEADER]>::try_from(out).map_err(|_| "invalid WAV header".into())
}

pub(crate) fn audio_valid(data: &AudioData) -> Result<(), String> {
    if !(8000..=192000).contains(&data.sample_rate)
        || !(1..=2).contains(&data.channels)
        || data.samples.is_empty()
        || !data.samples.len().is_multiple_of(usize::from(data.channels))
        || data.samples.len() > (MAX_ASSET - HEADER) / 4
        || data.samples.iter().any(|s| !s.is_finite() || s.abs() > 16.0)
    {
        return Err(format!(
            "audio sharing supports finite mono/stereo 8000..192000 Hz, up to {} MiB per sound",
            MAX_ASSET >> 20
        ));
    }
    Ok(())
}

/// How many transfer chunks a sound of this size has.
pub(crate) fn leaves(bytes: usize) -> usize {
    bytes.div_ceil(CHUNK)
}

fn leaf(offset: usize, data: &[u8]) -> Result<ChainingValue, String> {
    if data.is_empty() || data.len() > CHUNK || !offset.is_multiple_of(CHUNK) {
        return Err("invalid audio chunk".into());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.set_input_offset(offset as u64);
    hasher.update(data);
    Ok(hasher.finalize_non_root())
}

/// BLAKE3 splits a subtree so its left part is the largest power of two that leaves a right part.
fn split(len: u64) -> Option<(usize, u64)> {
    let left = len.checked_add(1)?.checked_div(2)?.checked_next_power_of_two()?;
    Some((usize::try_from(left / CHUNK as u64).ok()?, left))
}
fn subtree(cvs: &[ChainingValue], len: u64) -> Option<ChainingValue> {
    if let [only] = cvs {
        return Some(*only);
    }
    let (count, left) = split(len)?;
    let (l, r) = (cvs.get(..count)?, cvs.get(count..)?);
    if l.is_empty() || r.is_empty() {
        return None;
    }
    Some(merge_subtrees_non_root(&subtree(l, left)?, &subtree(r, len.checked_sub(left)?)?, Mode::Hash))
}
/// The root hash of a sound from its chunk chaining values; `None` for an invalid tree.
pub(crate) fn root(cvs: &[ChainingValue], bytes: usize) -> Option<String> {
    if cvs.len() < 2 || cvs.len() != leaves(bytes) {
        return None;
    }
    let len = bytes as u64;
    let (count, left) = split(len)?;
    let (l, r) = (cvs.get(..count)?, cvs.get(count..)?);
    if l.is_empty() || r.is_empty() {
        return None;
    }
    let hash = merge_subtrees_root(&subtree(l, left)?, &subtree(r, len.checked_sub(left)?)?, Mode::Hash);
    Some(hash.to_hex().to_string())
}
/// A sound of one chunk is checked against its hash directly and has no tree.
pub(crate) fn verify_tree(meta: &Asset, cvs: &[ChainingValue]) -> Result<(), String> {
    let valid =
        if leaves(meta.bytes) < 2 { cvs.is_empty() } else { root(cvs, meta.bytes).as_deref() == Some(&meta.hash) };
    if valid { Ok(()) } else { Err("audio chunk tree does not match the sound's hash".into()) }
}
fn verify_chunk(meta: &Asset, cvs: &[ChainingValue], offset: usize, data: &[u8]) -> Result<(), String> {
    let valid = if cvs.is_empty() {
        offset == 0 && data.len() == meta.bytes && blake3::hash(data).to_hex().as_str() == meta.hash
    } else {
        cvs.get(offset / CHUNK).is_some_and(|cv| leaf(offset, data).is_ok_and(|got| got == *cv))
    };
    if valid { Ok(()) } else { Err("audio chunk failed verification".into()) }
}
pub(crate) fn tree_bytes(cvs: &[ChainingValue]) -> Vec<u8> {
    cvs.iter().flatten().copied().collect()
}
pub(crate) fn tree_from(bytes: &[u8]) -> Vec<ChainingValue> {
    bytes.as_chunks::<32>().0.to_vec()
}

/// Hashes a byte stream as it is written: the whole-file hash and one chaining value per chunk.
struct Tree {
    whole: blake3::Hasher,
    leaf: blake3::Hasher,
    filled: usize,
    offset: usize,
    cvs: Vec<ChainingValue>,
}
impl Tree {
    fn new() -> Self {
        Self { whole: blake3::Hasher::new(), leaf: blake3::Hasher::new(), filled: 0, offset: 0, cvs: Vec::new() }
    }
    fn update(&mut self, mut bytes: &[u8]) {
        self.whole.update(bytes);
        while !bytes.is_empty() {
            let take = (CHUNK - self.filled).min(bytes.len());
            let (now, rest) = bytes.split_at(take);
            self.leaf.update(now);
            self.filled += take;
            bytes = rest;
            if self.filled == CHUNK {
                self.cvs.push(self.leaf.finalize_non_root());
                self.offset += CHUNK;
                self.leaf = blake3::Hasher::new();
                self.leaf.set_input_offset(self.offset as u64);
                self.filled = 0;
            }
        }
    }
    fn finish(mut self) -> (String, Vec<ChainingValue>) {
        if self.filled > 0 {
            self.cvs.push(self.leaf.finalize_non_root());
        }
        if self.cvs.len() < 2 {
            self.cvs.clear();
        }
        (self.whole.finalize().to_hex().to_string(), self.cvs)
    }
}

/// Write the canonical WAV copy of loaded audio to a new file, hashing it on the way.
pub(crate) fn encode(
    data: &AudioData,
    out: &mut impl Write,
    stop: &AtomicBool,
) -> Result<(Asset, Vec<ChainingValue>), String> {
    audio_valid(data)?;
    let bytes = data.samples.len().checked_mul(4).and_then(|n| n.checked_add(HEADER)).ok_or("sound too large")?;
    let head = header(data.sample_rate, data.channels, bytes - HEADER)?;
    let mut tree = Tree::new();
    let mut write = |part: &[u8]| -> Result<(), String> {
        tree.update(part);
        out.write_all(part).map_err(|e| format!("cannot write shared sound: {e}"))
    };
    write(&head)?;
    let mut buffer = Vec::with_capacity(CHUNK);
    for block in data.samples.chunks(CHUNK / 4) {
        stopped(stop)?;
        buffer.clear();
        buffer.extend(block.iter().flat_map(|s| s.to_le_bytes()));
        write(&buffer)?;
    }
    out.flush().map_err(|e| e.to_string())?;
    let (hash, cvs) = tree.finish();
    let meta = Asset { hash, bytes, rate: data.sample_rate, channels: data.channels, frames: data.frames() };
    validate_asset(&meta)?;
    Ok((meta, cvs))
}

/// The chunk tree of a verified sound already on disk, read in chunks.
pub(crate) fn tree_of(path: &Path, meta: &Asset, stop: &AtomicBool) -> Result<Vec<ChainingValue>, String> {
    let mut file = open_exact(path, meta.bytes)?;
    let mut tree = Tree::new();
    let mut buffer = vec![0; CHUNK];
    let mut left = meta.bytes;
    while left > 0 {
        stopped(stop)?;
        let n = left.min(CHUNK);
        let part = buffer.get_mut(..n).ok_or("invalid chunk")?;
        file.read_exact(part).map_err(|e| e.to_string())?;
        tree.update(part);
        left -= n;
    }
    let (hash, cvs) = tree.finish();
    if hash != meta.hash {
        return Err("audio file hash mismatch".into());
    }
    Ok(cvs)
}

fn open_exact(path: &Path, bytes: usize) -> Result<File, String> {
    let info = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !info.is_file() || info.len() != bytes as u64 {
        return Err("shared sound file has the wrong size".into());
    }
    File::open(path).map_err(|e| e.to_string())
}

/// Read a canonical sound from disk in chunks, checking its header, samples and whole-file hash.
pub(crate) fn decode(path: &Path, meta: &Asset, stop: &AtomicBool) -> Result<AudioData, String> {
    validate_asset(meta)?;
    let mut file = open_exact(path, meta.bytes)?;
    let mut head = [0u8; HEADER];
    file.read_exact(&mut head).map_err(|e| e.to_string())?;
    if head != header(meta.rate, meta.channels, meta.bytes - HEADER)? {
        return Err("audio is not the canonical float WAV copy".into());
    }
    let count = (meta.bytes - HEADER) / 4;
    let mut samples = Vec::new();
    samples.try_reserve_exact(count).map_err(|_| "not enough memory to load this shared sound")?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&head);
    let mut buffer = vec![0; CHUNK];
    let mut left = meta.bytes - HEADER;
    while left > 0 {
        stopped(stop)?;
        let n = left.min(CHUNK);
        let part = buffer.get_mut(..n).ok_or("invalid chunk")?;
        file.read_exact(part).map_err(|e| e.to_string())?;
        hasher.update(part);
        for word in part.as_chunks::<4>().0 {
            let sample = f32::from_le_bytes(*word);
            if !sample.is_finite() || sample.abs() > 16.0 {
                return Err("shared sound holds an invalid sample".into());
            }
            samples.push(sample);
        }
        left -= n;
    }
    if hasher.finalize().to_hex().as_str() != meta.hash {
        return Err("audio file hash mismatch".into());
    }
    Ok(AudioData { sample_rate: meta.rate, channels: meta.channels, samples })
}

/// A verified sound file to send, with its chunk tree.
pub(crate) struct Source<'a> {
    pub meta: &'a Asset,
    pub path: &'a Path,
    pub tree: &'a [ChainingValue],
}

/// Which part of a sound to send: from a whole-chunk offset, with or without its tree, and at
/// most this many chunks (`usize::MAX` for the rest of the sound).
#[derive(Clone, Copy)]
pub(crate) struct Range {
    pub from: usize,
    pub tree: bool,
    pub chunks: usize,
}

/// Stream part of a sound: its tree when asked, then chunk headers each followed by the chunk's
/// raw bytes. Only whole chunk offsets are accepted. Returns the bytes sent.
pub(crate) fn send<M: Serialize>(
    io: &mut impl Link,
    source: &Source<'_>,
    range: Range,
    tree_message: impl Fn(usize) -> M,
    chunk_message: impl Fn(usize, usize) -> M,
    stop: &AtomicBool,
    mut progress: impl FnMut(usize),
) -> Result<usize, String> {
    let bytes = source.meta.bytes;
    let from = range.from;
    if !from.is_multiple_of(CHUNK) || from >= bytes || range.chunks == 0 {
        return Err("invalid audio resume offset".into());
    }
    if range.tree {
        io.set_deadline(Instant::now() + STALL);
        protocol::write(io, &tree_message(source.tree.len()))?;
        io.write_all(&tree_bytes(source.tree)).and_then(|_| io.flush()).map_err(|e| e.to_string())?;
    }
    let mut file = open_exact(source.path, bytes)?;
    file.seek(SeekFrom::Start(from as u64)).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; CHUNK];
    let mut offset = from;
    let mut sent = 0;
    while offset < bytes && sent < range.chunks {
        stopped(stop)?;
        let n = (bytes - offset).min(CHUNK);
        let part = buffer.get_mut(..n).ok_or("invalid chunk")?;
        file.read_exact(part).map_err(|e| format!("cannot read shared sound: {e}"))?;
        io.set_deadline(Instant::now() + STALL);
        protocol::write(io, &chunk_message(offset, n))?;
        io.write_all(part).and_then(|_| io.flush()).map_err(|e| format!("connection write failed: {e}"))?;
        offset += n;
        sent += 1;
        progress(offset);
    }
    Ok(offset - from)
}

/// How a receiver reads the sender's tree and chunk headers.
pub(crate) struct Incoming<T, C> {
    pub tree: T,
    pub chunk: C,
}

/// One sound being received into the cache, possibly over several requests. It keeps its partial
/// copy open, so continuing never rereads what was already verified.
pub(crate) struct Download {
    pub meta: Asset,
    partial: Partial,
    cvs: Option<Vec<ChainingValue>>,
}
impl Download {
    /// Reopen any partial copy in the cache, keeping its verified chunks.
    pub fn start(meta: &Asset, cache: &Cache, stop: &AtomicBool) -> Result<Self, String> {
        validate_asset(meta)?;
        let mut partial = cache.resume(meta, stop)?;
        let cvs = partial.tree.take();
        Ok(Self { meta: meta.clone(), partial, cvs })
    }
    /// The offset to ask for, and whether the tree is still needed.
    pub fn next(&self) -> (usize, bool) {
        (self.partial.verified, self.cvs.is_none())
    }
    pub fn complete(&self) -> bool {
        self.partial.verified >= self.meta.bytes
    }
    /// Read the answer to the request [`Download::next`] described: the tree if it was asked,
    /// then exactly `min(chunks, remaining)` chunks, each verified before it is written.
    pub fn read<M: DeserializeOwned>(
        &mut self,
        io: &mut impl Link,
        cache: &Cache,
        parse: &Incoming<impl Fn(M) -> Option<usize>, impl Fn(M) -> Option<(usize, usize)>>,
        chunks: usize,
        stop: &AtomicBool,
        mut progress: impl FnMut(usize),
    ) -> Result<(), String> {
        let meta = &self.meta;
        if self.cvs.is_none() {
            io.set_deadline(Instant::now() + STALL);
            let count = (parse.tree)(protocol::read(io)?).ok_or("invalid audio chunk tree")?;
            let expected = if leaves(meta.bytes) < 2 { 0 } else { leaves(meta.bytes) };
            if count != expected {
                return Err("invalid audio chunk tree size".into());
            }
            let mut raw = vec![0; count * 32];
            io.read_exact(&mut raw).map_err(|e| format!("incomplete audio chunk tree: {e}"))?;
            let cvs = tree_from(&raw);
            verify_tree(meta, &cvs)?;
            cache.save_tree(meta, &raw)?;
            self.cvs = Some(cvs);
        }
        let cvs = self.cvs.as_deref().ok_or("audio chunk tree missing")?;
        let mut buffer = vec![0; CHUNK];
        let mut read = 0;
        while self.partial.verified < meta.bytes && read < chunks {
            stopped(stop)?;
            let offset = self.partial.verified;
            io.set_deadline(Instant::now() + STALL);
            let (at, len) = (parse.chunk)(protocol::read(io)?).ok_or("invalid audio chunk")?;
            if at != offset || len != (meta.bytes - offset).min(CHUNK) {
                return Err("invalid audio chunk offset or size".into());
            }
            let part = buffer.get_mut(..len).ok_or("invalid chunk")?;
            io.read_exact(part).map_err(|e| format!("incomplete audio chunk: {e}"))?;
            verify_chunk(meta, cvs, offset, part)?;
            self.partial.file.write_all(part).map_err(|e| format!("cannot store shared sound: {e}"))?;
            self.partial.verified += len;
            read += 1;
            progress(len);
        }
        Ok(())
    }
    /// The verified, complete sound becomes the cached file.
    pub fn finish(self, cache: &Cache) -> Result<PathBuf, String> {
        if !self.complete() {
            return Err("incomplete shared sound".into());
        }
        cache.complete(&self.meta, self.partial.file)
    }
}

/// Receive one sound into the cache, resuming a verified partial copy. `request` asks the peer
/// for the bytes from an offset (and for the tree when the cache has none). Returns the cached file.
pub(crate) fn receive<M: DeserializeOwned, L: Link>(
    io: &mut L,
    meta: &Asset,
    cache: &Cache,
    request: impl FnOnce(&mut L, usize, bool) -> Result<(), String>,
    parse: Incoming<impl Fn(M) -> Option<usize>, impl Fn(M) -> Option<(usize, usize)>>,
    stop: &AtomicBool,
    mut progress: impl FnMut(usize),
) -> Result<PathBuf, String> {
    let mut download = Download::start(meta, cache, stop)?;
    let (offset, tree) = download.next();
    request(io, offset, tree)?;
    if offset > 0 {
        progress(offset);
    }
    download.read(io, cache, &parse, usize::MAX, stop, progress)?;
    download.finish(cache)
}

#[cfg(test)]
#[path = "sound_tests.rs"]
mod tests;
