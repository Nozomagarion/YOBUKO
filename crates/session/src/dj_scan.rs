// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ library's music folders (`dj.folders.*`), scanned in the background like Mixxx's:
//! a scanner thread walks the folders and reads each new file's tags, and an analyser thread
//! runs rbxport's analysis (tempo and key) on each new track, one at a time. What they find
//! is handed to the library's worker in batches whenever it is free, so a large collection
//! never blocks the page or the decks. Files dropped on the page come in the same way.

use crate::dj::Job;
use crate::{CommandError, Projects};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use suhina_dj::{Analysed, Found};

/// Files read before a batch goes to the library (more in a large library).
const BATCH: usize = 250;
/// Analysed tracks kept before they go to the library (each write rewrites the library file).
const ANALYSED_BATCH: usize = 25;

/// A change the scanner or the analyser hands to the library's worker.
#[derive(Debug)]
pub(crate) enum LibraryEdit {
    Found(Vec<Found>),
    /// A whole scan ended: the files it saw, to mark the others missing.
    Seen(HashSet<PathBuf>),
    Analysed(Vec<(u32, Result<Analysed, String>)>),
    Folders(Vec<PathBuf>),
    /// (revision, track, stars).
    Rating(u64, u32, u8),
}

impl LibraryEdit {
    pub(crate) fn apply(self, catalog: &mut suhina_dj::Catalog) -> Result<(), String> {
        match self {
            Self::Found(found) => catalog.add_found(found).map(|_| ()),
            Self::Seen(seen) => catalog.mark_missing(&seen).map(|_| ()),
            Self::Analysed(results) => catalog.set_analysis(results),
            Self::Folders(folders) => catalog.set_folders(folders),
            Self::Rating(revision, track, stars) => catalog.set_rating(revision, track, stars),
        }
    }
}

enum Message {
    Batch(Vec<Found>),
    /// The scan ended; the files seen when it covered the whole of every folder.
    Done(Option<HashSet<PathBuf>>),
}

/// What to scan: the music folders, or files dropped on the page.
enum What {
    Folders(Vec<PathBuf>),
    Files(Vec<PathBuf>),
}

struct Scan {
    stop: Arc<AtomicBool>,
    messages: Receiver<Message>,
    /// Audio files looked at, and of them new to the library.
    looked: Arc<AtomicUsize>,
    new: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Analyser {
    jobs: Sender<(u32, PathBuf, f64)>,
    results: Receiver<(u32, Result<Analysed, String>)>,
    /// The track being analysed.
    busy: Option<u32>,
    title: Arc<Mutex<String>>,
}

#[derive(Default)]
pub(crate) struct Scanner {
    scan: Option<Scan>,
    /// Scans asked for while one runs.
    waiting: Vec<What>,
    /// Found files and analysis waiting for the worker.
    found: Vec<Found>,
    seen: Option<HashSet<PathBuf>>,
    analysed: Vec<(u32, Result<Analysed, String>)>,
    analyser: Option<Analyser>,
    /// Analysis paused by the person.
    pub(crate) paused: bool,
    /// Tracks the analyser could not be handed this session (kept from trying again).
    skipped: HashSet<u32>,
    /// The folders scanned since the library was opened, so each is scanned once by itself.
    scanned_once: bool,
    error: Option<String>,
    /// The last scan's outcome: files looked at and new.
    last: Option<(usize, usize)>,
}

impl Scanner {
    /// Keeps what the scan sent; true when it ended.
    fn take(&mut self, message: Message) -> bool {
        match message {
            Message::Batch(found) => {
                self.found.extend(found);
                false
            }
            Message::Done(seen) => {
                self.seen = seen;
                if let Some(scan) = &self.scan {
                    self.last = Some((scan.looked.load(Ordering::Relaxed), scan.new.load(Ordering::Relaxed)));
                }
                true
            }
        }
    }

    /// The background work has nothing left to do (analysis paused counts as nothing left).
    fn settled(&self, catalog: &suhina_dj::Catalog) -> bool {
        self.scan.is_none()
            && self.waiting.is_empty()
            && self.found.is_empty()
            && self.seen.is_none()
            && self.analysed.is_empty()
            && self.analyser.as_ref().is_none_or(|a| a.busy.is_none())
            && (self.paused || !catalog.tracks.iter().any(|t| t.needs_analysis() && !self.skipped.contains(&t.id)))
    }

    /// A background change the worker could not make.
    pub(crate) fn failed(&mut self, error: String) {
        self.error = Some(error);
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        // The analyser ends when its channel closes; the scan stops in its own drop.
        self.analyser = None;
    }
}

fn start(what: What, known: HashSet<PathBuf>) -> Result<Scan, String> {
    let (send, messages) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let looked = Arc::new(AtomicUsize::new(0));
    let new = Arc::new(AtomicUsize::new(0));
    let (halt, looked_at, added) = (stop.clone(), looked.clone(), new.clone());
    let thread = std::thread::Builder::new()
        .name("suhina-dj-scan".into())
        .spawn(move || {
            let mut batch = Vec::new();
            let mut seen = HashSet::new();
            let mut visit = |path: PathBuf| -> bool {
                looked_at.fetch_add(1, Ordering::Relaxed);
                if !known.contains(&path)
                    && let Ok(found) = suhina_dj::scan::read(&path)
                {
                    added.fetch_add(1, Ordering::Relaxed);
                    batch.push(found);
                    if batch.len() >= BATCH && send.send(Message::Batch(std::mem::take(&mut batch))).is_err() {
                        return false;
                    }
                }
                seen.insert(path);
                true
            };
            let whole = match what {
                What::Folders(folders) => {
                    for folder in folders {
                        suhina_dj::scan::walk(&folder, &halt, &mut visit);
                    }
                    !halt.load(Ordering::Relaxed) && looked_at.load(Ordering::Relaxed) < suhina_dj::scan::MAX_VISITED
                }
                What::Files(files) => {
                    for file in files {
                        if halt.load(Ordering::Relaxed) || !visit(file) {
                            break;
                        }
                    }
                    false
                }
            };
            if !batch.is_empty() {
                let _ = send.send(Message::Batch(batch));
            }
            let _ = send.send(Message::Done(whole.then_some(seen)));
        })
        .map_err(|e| format!("could not start the DJ scan: {e}"))?;
    Ok(Scan { stop, messages, looked, new, thread: Some(thread) })
}

fn analyser() -> Result<Analyser, String> {
    let (jobs, inbox) = mpsc::channel::<(u32, PathBuf, f64)>();
    let (done, results) = mpsc::channel();
    let title = Arc::new(Mutex::new(String::new()));
    std::thread::Builder::new()
        .name("suhina-dj-analyse".into())
        .spawn(move || {
            while let Ok((id, path, seconds)) = inbox.recv() {
                let result = suhina_dj::scan::analyse(&path, seconds);
                if done.send((id, result)).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| format!("could not start the DJ analysis: {e}"))?;
    Ok(Analyser { jobs, results, busy: None, title })
}

/// Paths of the library's tracks, which a scan skips.
fn known(p: &Projects) -> HashSet<PathBuf> {
    p.dj.catalog.tracks.iter().filter(|t| !t.missing).map(|t| t.path.clone()).collect()
}

/// Starts a scan, or keeps it for when the running one ends.
fn scan(p: &mut Projects, what: What) -> Result<(), CommandError> {
    let s = &mut p.dj.scanner;
    if s.scan.is_some() {
        // Files dropped while a scan runs join the files already waiting.
        match (s.waiting.last_mut(), what) {
            (Some(What::Files(files)), What::Files(more)) => {
                files.extend(more);
                files.truncate(suhina_dj::MAX_TRACKS);
            }
            (_, what) => s.waiting.push(what),
        }
        return Ok(());
    }
    let known = known(p);
    let s = &mut p.dj.scanner;
    s.scan = Some(start(what, known).map_err(CommandError::from)?);
    s.error = None;
    Ok(())
}

/// One step of the background work, each time the library is read: takes what the scan and
/// the analyser found, hands it to the worker when it is free, and feeds the analyser.
pub(crate) fn tick(p: &mut Projects) {
    // The music folders are scanned once when the library opens, for files added since.
    if !p.dj.scanner.scanned_once && p.dj.catalog_read() {
        p.dj.scanner.scanned_once = true;
        let folders = p.dj.catalog.folders.clone();
        if !folders.is_empty() {
            let _ = scan(p, What::Folders(folders));
        }
    }
    let mut ended = false;
    if let Some(scan) = &p.dj.scanner.scan {
        let mut messages = Vec::new();
        while let Ok(message) = scan.messages.try_recv() {
            messages.push(message);
        }
        for message in messages {
            ended |= p.dj.scanner.take(message);
        }
    }
    if ended {
        next_scan(p);
    }
    analyse(p);
    hand_over(p);
}

/// Starts the scan that waited for the one that ended.
fn next_scan(p: &mut Projects) {
    let s = &mut p.dj.scanner;
    s.scan = None;
    if !s.waiting.is_empty() {
        let what = s.waiting.remove(0);
        let _ = scan(p, what);
    }
}

impl Projects {
    /// Waits until the DJ library's background work is done: the scans ended, the new tracks
    /// are analysed (unless the analysis is paused) and the library holds all of it. The
    /// explicit signal the tests wait on instead of a clock.
    pub fn sync_dj_library(&mut self) -> Result<(), CommandError> {
        crate::dj::ready(self)?;
        // Each round finishes one step: a reply, a scan message or an analysed track.
        for _ in 0..10_000_000 {
            if !self.dj.idle() {
                self.dj.wait();
            } else if let Some(scan) = &self.dj.scanner.scan {
                let ended = match scan.messages.recv() {
                    Ok(message) => self.dj.scanner.take(message),
                    Err(_) => true,
                };
                if ended {
                    next_scan(self);
                }
            } else if let Some(a) = self.dj.scanner.analyser.as_mut().filter(|a| a.busy.is_some()) {
                match a.results.recv() {
                    Ok(result) => {
                        a.busy = None;
                        self.dj.scanner.analysed.push(result);
                    }
                    Err(_) => self.dj.scanner.analyser = None,
                }
            } else if self.dj.scanner.settled(&self.dj.catalog) {
                return Ok(());
            }
            crate::dj::ready(self)?;
        }
        Err("the DJ library's background work did not finish".into())
    }
}

/// Feeds the analyser the next track the library has not analysed yet.
fn analyse(p: &mut Projects) {
    let s = &mut p.dj.scanner;
    if let Some(a) = &mut s.analyser {
        while let Ok(result) = a.results.try_recv() {
            a.busy = None;
            s.analysed.push(result);
        }
    }
    if s.paused || s.analyser.as_ref().is_some_and(|a| a.busy.is_some()) {
        return;
    }
    let waiting: HashSet<u32> = s.analysed.iter().map(|(id, _)| *id).collect();
    let next =
        p.dj.catalog
            .tracks
            .iter()
            .find(|t| t.needs_analysis() && !waiting.contains(&t.id) && !p.dj.scanner.skipped.contains(&t.id))
            .map(|t| (t.id, t.path.clone(), t.seconds, t.title.clone()));
    let Some((id, path, seconds, title)) = next else { return };
    let s = &mut p.dj.scanner;
    if s.analyser.is_none() {
        match analyser() {
            Ok(a) => s.analyser = Some(a),
            Err(e) => {
                s.error = Some(e);
                s.paused = true;
                return;
            }
        }
    }
    if let Some(a) = &mut s.analyser {
        if a.jobs.send((id, path, seconds)).is_ok() {
            a.busy = Some(id);
            if let Ok(mut shown) = a.title.lock() {
                *shown = title;
            }
        } else {
            s.skipped.insert(id);
            s.analyser = None;
        }
    }
}

/// Hands what is waiting to the library's worker when it is free: found files first, then the
/// end of a scan, then analysis in batches (all of it once nothing is left to analyse).
fn hand_over(p: &mut Projects) {
    if p.dj.busy {
        return;
    }
    // Each change rewrites the library file, so a large library takes larger batches.
    let tracks = p.dj.catalog.tracks.len();
    let s = &mut p.dj.scanner;
    let analysing = s.analyser.as_ref().is_some_and(|a| a.busy.is_some());
    let edit = if s.found.len() >= BATCH.max(tracks / 4) || (!s.found.is_empty() && s.scan.is_none()) {
        LibraryEdit::Found(std::mem::take(&mut s.found))
    } else if s.scan.is_none() && s.seen.is_some() {
        LibraryEdit::Seen(s.seen.take().unwrap_or_default())
    } else if s.analysed.len() >= ANALYSED_BATCH.max(tracks / 200)
        || (!s.analysed.is_empty() && (!analysing || s.paused))
    {
        LibraryEdit::Analysed(std::mem::take(&mut s.analysed))
    } else {
        return;
    };
    if let Err(e) = p.dj.queue_background(Job::Library(edit)) {
        p.dj.scanner.error = Some(e.to_string());
    }
}

/// Ready, with the library read from disk: its music folders are what the edit starts from.
fn loaded(p: &mut Projects) -> Result<(), CommandError> {
    crate::dj::ready(p)?;
    if !p.dj.catalog_read() {
        // Reading the library file is the worker's first job, and a short one.
        p.dj.wait();
        crate::dj::ready(p)?;
    }
    if !p.dj.catalog_read() {
        return Err(p.dj.error.clone().unwrap_or_else(|| "the DJ library could not be read".into()).into());
    }
    Ok(())
}

/// `dj.folders.add {folder}`: a music folder, scanned now and each time the library opens.
pub(crate) fn add(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    loaded(p)?;
    let folder = v["folder"].as_str().filter(|f| !f.is_empty()).ok_or("choose a music folder")?;
    let folder = PathBuf::from(folder).canonicalize().map_err(|e| CommandError(format!("music folder: {e}")))?;
    if !folder.is_dir() {
        return Err("choose a folder".into());
    }
    let mut folders = p.dj.catalog.folders.clone();
    if folders.iter().any(|f| folder.starts_with(f)) {
        return Err("this folder is already in a music folder of the library".into());
    }
    // A folder that holds music folders takes their place.
    folders.retain(|f| !f.starts_with(&folder));
    if folders.len() >= suhina_dj::MAX_FOLDERS {
        return Err(format!("the library takes at most {} music folders", suhina_dj::MAX_FOLDERS).into());
    }
    folders.push(folder.clone());
    p.dj.queue(Job::Library(LibraryEdit::Folders(folders)))?;
    scan(p, What::Folders(vec![folder]))?;
    Ok(json!({"queued": true}))
}

/// `dj.folders.remove {folder}`: the folder is no longer scanned; its tracks stay.
pub(crate) fn remove(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    loaded(p)?;
    let folder = PathBuf::from(v["folder"].as_str().ok_or("choose a music folder")?);
    let mut folders = p.dj.catalog.folders.clone();
    let before = folders.len();
    folders.retain(|f| *f != folder);
    if folders.len() == before {
        return Err("not a music folder of the library".into());
    }
    p.dj.queue(Job::Library(LibraryEdit::Folders(folders)))
}

/// `dj.folders.scan`: looks through the music folders again now.
pub(crate) fn rescan(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    loaded(p)?;
    let folders = p.dj.catalog.folders.clone();
    if folders.is_empty() {
        return Err("add a music folder first".into());
    }
    if p.dj.scanner.scan.is_some() {
        return Err("the music folders are being scanned".into());
    }
    scan(p, What::Folders(folders))?;
    Ok(json!({"queued": true}))
}

/// Audio files dropped on the page: read in the background, analysed after.
pub(crate) fn add_files(p: &mut Projects, files: Vec<PathBuf>) -> Result<Value, CommandError> {
    let files: Vec<PathBuf> = files
        .into_iter()
        .filter(|f| suhina_dj::is_dj_audio(f))
        .filter_map(|f| f.canonicalize().ok())
        .take(suhina_dj::MAX_TRACKS)
        .collect();
    if files.is_empty() {
        return Err("choose an M4A, AAC, MP3, FLAC, WAV, AIFF or Ogg audio file".into());
    }
    scan(p, What::Files(files))?;
    Ok(json!({"queued": true}))
}

/// `dj.analysis.set {on}`: pauses or resumes the background analysis.
pub(crate) fn set_analysis(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj::ready(p)?;
    let on = v["on"].as_bool().ok_or("on must be true or false")?;
    p.dj.scanner.paused = !on;
    tick(p);
    Ok(json!({"on": on}))
}

/// `dj.track.rate {track, revision, stars}`.
pub(crate) fn rate(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj::ready(p)?;
    let track = crate::dj::id(v, "track")?;
    let stars = v["stars"].as_u64().filter(|s| *s <= 5).ok_or("stars must be 0–5")? as u8;
    let revision = crate::dj::revision(p, v)?;
    p.dj.queue(Job::Library(LibraryEdit::Rating(revision, track, stars)))
}

/// The music folders, the scan and the analysis, as the page shows them.
pub(crate) fn snapshot(p: &Projects) -> Value {
    let s = &p.dj.scanner;
    let scan = s
        .scan
        .as_ref()
        .map(|scan| json!({"looked": scan.looked.load(Ordering::Relaxed), "new": scan.new.load(Ordering::Relaxed)}));
    let left = p.dj.catalog.tracks.iter().filter(|t| t.needs_analysis()).count();
    let title = s.analyser.as_ref().filter(|a| a.busy.is_some()).and_then(|a| a.title.lock().ok().map(|t| t.clone()));
    json!({
        "folders": p.dj.catalog.folders,
        "scan": scan,
        "lastScan": s.last.map(|(looked, new)| json!({"looked": looked, "new": new})),
        "waiting": s.found.len() + s.waiting.len(),
        "analysis": {"on": !s.paused, "left": left, "title": title},
        "missing": p.dj.catalog.tracks.iter().filter(|t| t.missing).count(),
        "error": s.error,
    })
}
