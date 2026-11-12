// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording the DJ mix (`dj.record.*`): what the master sends, written as a 16-bit stereo WAV
//! file by a writer thread while the decks play. Quiet moments are kept, so the file runs as
//! long as the recording did.

use crate::{CommandError, Projects};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use suhina_engine::DeckRecording;

/// How often the writer empties the master's ring (it holds four seconds).
const EVERY: Duration = Duration::from_millis(25);
/// A WAV file stops a little short of 4 GiB: its sizes are 32-bit.
const MAX_BYTES: u64 = u32::MAX as u64 - (1 << 20);

type Done = (DeckRecording, Result<(), String>);

struct Run {
    path: PathBuf,
    rate: u32,
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    thread: Option<JoinHandle<Done>>,
}

impl Run {
    fn finish(&mut self) -> Option<Done> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().and_then(|t| t.join().ok())
    }
}

#[derive(Default)]
pub(crate) struct Recorder {
    run: Option<Run>,
    /// The last file a recording was saved to.
    last: Option<PathBuf>,
    error: Option<String>,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Finish the file, so closing the app keeps what was recorded.
        if let Some(run) = self.run.as_mut() {
            run.finish();
        }
    }
}

/// A file name in `folder` for a recording started now, not already taken.
fn file_in(folder: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string();
    let mut path = folder.join(format!("Mix {stamp}.wav"));
    let mut n = 2;
    while path.exists() && n < 1000 {
        path = folder.join(format!("Mix {stamp} ({n}).wav"));
        n += 1;
    }
    path
}

/// Empties the ring into `writer` until asked to stop, then once more.
fn write(
    mut rec: DeckRecording,
    mut writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    stop: &AtomicBool,
    frames: &AtomicU64,
    dropped: &AtomicU64,
) -> Done {
    let mut block = Vec::with_capacity(rec.sample_rate() as usize * 2);
    let mut result = Ok(());
    loop {
        let last = stop.load(Ordering::Relaxed);
        if last {
            rec.stop();
        }
        block.clear();
        rec.drain(&mut block);
        dropped.store(rec.dropped_frames(), Ordering::Relaxed);
        let written = frames.load(Ordering::Relaxed);
        let room = MAX_BYTES / 4;
        let take = (block.len() as u64 / 2).min(room.saturating_sub(written)) as usize;
        for s in block.iter().take(take * 2) {
            let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            if let Err(e) = writer.write_sample(v) {
                result = Err(e.to_string());
                break;
            }
        }
        frames.fetch_add(take as u64, Ordering::Relaxed);
        if result.is_ok() && take * 2 < block.len() {
            result = Err("the recording reached the 4 GB a WAV file can hold, and stopped there".into());
        }
        if last || result.is_err() {
            break;
        }
        if block.is_empty() {
            std::thread::sleep(EVERY);
        }
    }
    rec.stop();
    let closed = writer.finalize().map_err(|e| e.to_string());
    (rec, result.and(closed))
}

pub(crate) fn start(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    if p.dj.decks.recorder.run.is_some() {
        return Err("the mix is already being recorded".into());
    }
    let folder =
        v["folder"].as_str().filter(|f| !f.is_empty()).map(PathBuf::from).ok_or("choose a folder for the recording")?;
    std::fs::create_dir_all(&folder).map_err(|e| CommandError(format!("cannot make {}: {e}", folder.display())))?;
    let ctl = p.preview_session_mut()?.player_mut()?.controller_mut();
    let mut rec = ctl.take_deck_recording().ok_or("this audio output is already recording")?;
    let rate = rec.sample_rate();
    let spec =
        hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let path = file_in(&folder);
    let writer = match hound::WavWriter::create(&path, spec) {
        Ok(w) => w,
        Err(e) => {
            ctl.return_deck_recording(rec);
            return Err(CommandError(format!("cannot write {}: {e}", path.display())));
        }
    };
    let (stop, frames, dropped) =
        (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    rec.start();
    let (s, f, d) = (Arc::clone(&stop), Arc::clone(&frames), Arc::clone(&dropped));
    let thread = std::thread::Builder::new().name("dj-record".into()).spawn(move || write(rec, writer, &s, &f, &d));
    let thread = thread.map_err(|e| CommandError(format!("cannot start recording: {e}")))?;
    let r = &mut p.dj.decks.recorder;
    r.error = None;
    r.run = Some(Run { path: path.clone(), rate, stop, frames, dropped, thread: Some(thread) });
    Ok(json!({"recording": true, "path": path}))
}

pub(crate) fn stop(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    let Some(mut run) = p.dj.decks.recorder.run.take() else {
        return Ok(json!({"recording": false}));
    };
    let done = run.finish();
    let seconds = run.frames.load(Ordering::Relaxed) as f64 / f64::from(run.rate.max(1));
    let r = &mut p.dj.decks.recorder;
    r.last = Some(run.path.clone());
    r.error = match &done {
        Some((_, Err(e))) => Some(e.clone()),
        None => Some("the recording stopped unexpectedly".into()),
        _ => None,
    };
    // Back to the audio it came from; a newer output has a recording of its own, kept.
    if let Some((rec, _)) = done
        && let Ok(ctl) = p.preview_session_mut().and_then(|s| s.player_mut()).map(|pl| pl.controller_mut())
    {
        let own = ctl.take_deck_recording().unwrap_or(rec);
        ctl.return_deck_recording(own);
    }
    Ok(json!({"recording": false, "path": run.path, "seconds": seconds}))
}

/// The recording as the page shows it.
pub(crate) fn snapshot(p: &Projects) -> Value {
    let r = &p.dj.decks.recorder;
    let run = r.run.as_ref();
    json!({
        "on": run.is_some(),
        "seconds": run.map_or(0.0, |run| run.frames.load(Ordering::Relaxed) as f64 / f64::from(run.rate.max(1))),
        "path": run.map(|run| &run.path),
        "dropped": run.map_or(0, |run| run.dropped.load(Ordering::Relaxed)),
        "last": r.last,
        "error": r.error,
    })
}
