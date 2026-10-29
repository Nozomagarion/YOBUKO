// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::dj::{Job, id, ready};
use crate::{CommandError, ProjectId, Projects};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use suhina_media::AudioData;

pub(crate) type Loaded = (u64, Result<Arc<AudioData>, String>);
pub(crate) struct Request {
    token: u64,
    track: u32,
    from: f64,
    target: Option<ProjectId>,
}
#[derive(Default)]
pub(crate) struct Audition {
    next: u64,
    request: Option<Request>,
    active: Option<(u32, Option<ProjectId>)>,
    pub(crate) loaded: Option<Loaded>,
    stopping: bool,
}
pub(crate) struct Cached {
    path: PathBuf,
    length: u64,
    modified: Option<SystemTime>,
    data: Arc<AudioData>,
}

pub(crate) fn prepare(track: &suhina_dj::Track, cache: &mut Option<Cached>) -> Result<Arc<AudioData>, String> {
    prepare_limited(track, cache, usize::MAX)
}
pub(crate) fn prepare_limited(
    track: &suhina_dj::Track,
    cache: &mut Option<Cached>,
    max_samples: usize,
) -> Result<Arc<AudioData>, String> {
    let before = std::fs::metadata(&track.path).map_err(|e| format!("audio file: {e}"))?;
    if let Some(c) = cache
        .as_ref()
        .filter(|c| c.path == track.path && c.length == before.len() && c.modified == before.modified().ok())
    {
        let seconds = c.data.frames() as f64 / f64::from(c.data.sample_rate);
        if c.data.samples.len() <= max_samples && (seconds - track.seconds).abs() <= 0.05 {
            return Ok(Arc::clone(&c.data));
        }
    }
    *cache = None;
    let info = suhina_media::info::sample_info(&track.path)?;
    if !info.duration_seconds.is_some_and(|s| s > 0.0 && s <= 1200.0)
        || info.sample_rate.is_none_or(|r| !(1000..=96000).contains(&r))
        || !info.channels.is_some_and(|c| (1..=2).contains(&c))
    {
        return Err("DJ audition accepts up to 20 minutes of mono/stereo audio at up to 96 kHz".into());
    }
    let data = suhina_media::decode_file_bounded(&track.path, max_samples)?;
    if !(1..=2).contains(&data.channels)
        || !(1000..=96000).contains(&data.sample_rate)
        || data.samples.is_empty()
        || data.frames() as u64 > u64::from(data.sample_rate) * 1200
        || data.samples.iter().any(|s| !s.is_finite())
    {
        return Err("invalid or oversized DJ audio".into());
    }
    let after = std::fs::metadata(&track.path).map_err(|e| e.to_string())?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("audio changed during loading; reimport it".into());
    }
    let seconds = data.frames() as f64 / f64::from(data.sample_rate);
    if (seconds - track.seconds).abs() > 0.05 {
        return Err("audio duration changed; reimport before auditioning".into());
    }
    let data = Arc::new(data);
    *cache = Some(Cached {
        path: track.path.clone(),
        length: after.len(),
        modified: after.modified().ok(),
        data: Arc::clone(&data),
    });
    Ok(data)
}

pub(crate) fn stop(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    p.dj.audition.request = None;
    p.dj.audition.loaded = None;
    p.dj.audition.stopping = true;
    if let Some((_, target)) = p.dj.audition.active
        && target == p.active()
        && p.preview_session().and_then(|s| s.player()).is_some()
    {
        p.preview_session_mut()?.player_mut()?.controller_mut().preview_stop().map_err(CommandError)?;
    }
    p.dj.audition.active = None;
    p.dj.audition.stopping = false;
    Ok(json!({"stopped":true}))
}

pub(crate) fn play(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    ready(p)?;
    if p.dj.busy {
        return Err("the DJ library is busy; wait for loading to finish".into());
    }
    let track = id(v, "track")?;
    let t = p.dj.catalog.tracks.iter().find(|t| t.id == track).ok_or("unknown DJ track")?;
    let from = v["from"]
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.0 && *s < t.seconds)
        .ok_or("choose an audition position before the end of the track")?;
    let token = p.dj.audition.next.checked_add(1).ok_or("DJ audition request ids exhausted")?;
    stop(p, &json!({}))?;
    // Opening an output is reserved for an explicit Play. Loading/decode stays on the worker.
    p.preview_session_mut()?.player_mut()?;
    p.dj.queue(Job::Preview(token, track))?;
    p.dj.audition.next = token;
    p.dj.audition.request = Some(Request { token, track, from, target: p.active() });
    Ok(json!({"queued":true}))
}

pub(crate) fn sync(p: &mut Projects) {
    if p.dj.audition.stopping
        && let Err(e) = stop(p, &json!({}))
    {
        p.dj.error = Some(e.to_string());
    }
    if p.dj.audition.active.is_some_and(|(_, target)| target != p.active()) {
        p.dj.audition.active = None;
    }
    let Some((token, result)) = p.dj.audition.loaded.take() else { return };
    let Some(request) = p.dj.audition.request.take() else { return };
    if request.token != token || request.target != p.active() {
        return;
    }
    let result = result.and_then(|data| {
        let session = p.preview_session_mut().map_err(String::from)?;
        session.player_mut().map_err(String::from)?.controller_mut().preview_sample(data, request.from, 0.501_187_2)
    });
    match result {
        Ok(()) => p.dj.audition.active = Some((request.track, request.target)),
        Err(e) => p.dj.error = Some(e),
    }
}

pub(crate) fn snapshot(p: &Projects) -> Value {
    let monitor = p.preview_session().and_then(|s| s.player()).map(|p| p.preview_monitor());
    let track = p.dj.audition.request.as_ref().map(|r| r.track).or(p.dj.audition.active.map(|a| a.0));
    json!({"track":track,"loading":p.dj.audition.request.is_some(),
        "playing":p.dj.audition.active.is_some() && monitor.is_some_and(|m|m.playing()),
        "position":if p.dj.audition.active.is_some() {monitor.map_or(0.0, |m|m.seconds())} else {0.0}})
}
