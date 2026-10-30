// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound cache on each computer: verified sounds stored by content hash under the shared
//! project storage, reused across sessions, with resumable partial copies and a size cap.
//! File names come only from validated 64-hex hashes; nothing a peer sends names a path.

use crate::assets::{Asset, CHUNK, validate_asset};
use crate::sound;
use blake3::hazmat::ChainingValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

/// The cache folder's name inside the shared project storage.
pub(crate) const FOLDER: &str = "Sound cache";
/// Entries scanned when trimming; a folder with more is trimmed from what was listed.
const MAX_ENTRIES: usize = 3 * 65536;

pub(crate) struct Cache {
    dir: PathBuf,
    cap: u64,
}

/// A partial copy reopened for appending after its verified prefix.
pub(crate) struct Partial {
    pub file: File,
    pub verified: usize,
    pub tree: Option<Vec<ChainingValue>>,
}

fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Cache {
    /// Open (creating) the cache under an absolute storage root chosen locally.
    pub fn open(root: &Path, cap: u64) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("choose an application data folder before sharing audio".into());
        }
        let dir = root.join(FOLDER);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create the sound cache: {e}"))?;
        let info = std::fs::symlink_metadata(&dir).map_err(|e| e.to_string())?;
        if !info.is_dir() {
            return Err("the sound cache is not a folder".into());
        }
        Ok(Self { dir, cap })
    }
    fn path(&self, meta: &Asset, extension: &str) -> Result<PathBuf, String> {
        validate_asset(meta)?;
        Ok(self.dir.join(format!("{}.{extension}", meta.hash)))
    }
    /// The complete cached copy of a sound when one of the right size is there. The caller
    /// still verifies its contents while decoding and calls [`Cache::discard`] on a mismatch.
    pub fn find(&self, meta: &Asset) -> Option<PathBuf> {
        let path = self.path(meta, "wav").ok()?;
        let info = std::fs::symlink_metadata(&path).ok()?;
        (info.is_file() && info.len() == meta.bytes as u64).then_some(path)
    }
    /// Mark a cached sound as just used, for least-recently-used trimming.
    pub fn touch(&self, path: &Path) {
        if let Ok(file) = OpenOptions::new().write(true).open(path) {
            let _ = file.set_modified(SystemTime::now());
        }
    }
    /// Remove a cached sound (and any partial copy) that failed verification.
    pub fn discard(&self, meta: &Asset) {
        for extension in ["wav", "part", "tree"] {
            if let Ok(path) = self.path(meta, extension) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    pub fn save_tree(&self, meta: &Asset, raw: &[u8]) -> Result<(), String> {
        if raw.is_empty() {
            return Ok(());
        }
        std::fs::write(self.path(meta, "tree")?, raw).map_err(|e| format!("cannot write the sound cache: {e}"))
    }
    fn load_tree(&self, meta: &Asset) -> Option<Vec<ChainingValue>> {
        let path = self.path(meta, "tree").ok()?;
        let info = std::fs::symlink_metadata(&path).ok()?;
        let count = sound::leaves(meta.bytes);
        if count < 2 || !info.is_file() || info.len() != (count * 32) as u64 {
            return None;
        }
        let cvs = sound::tree_from(&std::fs::read(path).ok()?);
        sound::verify_tree(meta, &cvs).ok().map(|_| cvs)
    }
    /// Reopen a partial copy, keeping only the whole chunks that match the sound's tree. A tree
    /// is kept from the previous attempt only when its root equals the sound's hash.
    pub fn resume(&self, meta: &Asset, stop: &AtomicBool) -> Result<Partial, String> {
        let path = self.path(meta, "part")?;
        if std::fs::symlink_metadata(&path).is_ok_and(|i| !i.is_file()) {
            return Err("the sound cache holds an unexpected entry".into());
        }
        let tree = self.load_tree(meta);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("cannot write the sound cache: {e}"))?;
        let mut verified = 0usize;
        if let Some(cvs) = &tree {
            let length = file.metadata().map_err(|e| e.to_string())?.len();
            let mut buffer = vec![0; CHUNK];
            while (verified as u64).saturating_add(CHUNK as u64) <= length.min(meta.bytes as u64) {
                if stop.load(Ordering::Relaxed) {
                    return Err("session stopped".into());
                }
                file.read_exact(&mut buffer).map_err(|e| e.to_string())?;
                let mut hasher = blake3::Hasher::new();
                blake3::hazmat::HasherExt::set_input_offset(&mut hasher, verified as u64);
                hasher.update(&buffer);
                if cvs.get(verified / CHUNK) != Some(&blake3::hazmat::HasherExt::finalize_non_root(&hasher)) {
                    break;
                }
                verified += CHUNK;
            }
        }
        // A whole sound is never left as a partial copy: its last chunk is rechecked as it arrives.
        if verified >= meta.bytes {
            verified = (meta.bytes - 1) / CHUNK * CHUNK;
        }
        file.set_len(verified as u64).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(verified as u64)).map_err(|e| e.to_string())?;
        Ok(Partial { file, verified, tree })
    }
    /// Finish a fully verified partial copy: it becomes the cached sound.
    pub fn complete(&self, meta: &Asset, file: File) -> Result<PathBuf, String> {
        file.sync_all().map_err(|e| format!("cannot store shared sound: {e}"))?;
        drop(file);
        let done = self.path(meta, "wav")?;
        std::fs::rename(self.path(meta, "part")?, &done).map_err(|e| format!("cannot store shared sound: {e}"))?;
        if let Ok(tree) = self.path(meta, "tree") {
            let _ = std::fs::remove_file(tree);
        }
        Ok(done)
    }
    /// Remove the least recently used entries until the cache fits its cap, sparing `keep`.
    pub fn trim(&self, keep: &BTreeSet<String>) -> Result<(), String> {
        let mut entries = BTreeMap::<String, (SystemTime, u64, Vec<PathBuf>)>::new();
        let listing = std::fs::read_dir(&self.dir).map_err(|e| e.to_string())?;
        for entry in listing.take(MAX_ENTRIES).flatten() {
            let name = entry.file_name();
            let Some((hash, extension)) = name.to_str().and_then(|n| n.split_once('.')) else { continue };
            let Ok(info) = entry.metadata() else { continue };
            if !is_hash(hash) || !matches!(extension, "wav" | "part" | "tree") || !info.is_file() {
                continue;
            }
            let used = info.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let item = entries.entry(hash.to_owned()).or_insert((used, 0, Vec::new()));
            item.0 = item.0.max(used);
            item.1 = item.1.saturating_add(info.len());
            item.2.push(entry.path());
        }
        let mut total: u64 = entries.values().map(|e| e.1).fold(0, u64::saturating_add);
        let mut order: Vec<_> = entries.into_iter().filter(|(hash, _)| !keep.contains(hash)).collect();
        order.sort_by_key(|(_, (used, _, _))| *used);
        for (_, (_, size, paths)) in order {
            if total <= self.cap {
                break;
            }
            for path in paths {
                let _ = std::fs::remove_file(path);
            }
            total = total.saturating_sub(size);
        }
        Ok(())
    }
}

/// Place a file at a new name: a hard link (no extra disk) or, across volumes, a copy. Never
/// replaces an existing file.
pub(crate) fn link_or_copy(source: &Path, target: &Path) -> Result<(), String> {
    if std::fs::hard_link(source, target).is_ok() {
        return Ok(());
    }
    let mut from = File::open(source).map_err(|e| e.to_string())?;
    let mut to = OpenOptions::new().write(true).create_new(true).open(target).map_err(|e| e.to_string())?;
    std::io::copy(&mut from, &mut to)
        .and_then(|_| to.sync_all())
        .map_err(|e| format!("cannot copy shared sound: {e}"))?;
    Ok(())
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
