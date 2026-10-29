// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A single prepared waveform, independently cancellable from audition.

use crate::{CommandError, Projects};
use serde_json::{Value, json};
use std::sync::Arc;
use suhina_media::waveform::Peaks;

pub(crate) type Loaded = (u64, Result<Arc<Peaks>, String>);
#[derive(Default)]
pub(crate) struct Waveform {
    serial: u64,
    pending: Option<u64>,
    track: Option<u32>,
    peaks: Option<Arc<Peaks>>,
    error: Option<String>,
}
impl Waveform {
    pub(crate) fn clear(&mut self) {
        self.pending = None;
        self.track = None;
        self.peaks = None;
        self.error = None;
    }
    pub(crate) fn apply(&mut self, (token, result): Loaded) {
        if self.pending != Some(token) {
            return;
        }
        self.pending = None;
        match result {
            Ok(peaks) => self.peaks = Some(peaks),
            Err(error) => self.error = Some(error),
        }
    }
    pub(crate) fn snapshot(&self) -> Value {
        json!({"track":self.track,"loading":self.pending.is_some(),"error":self.error,
            "frames":self.peaks.as_ref().map(|p|p.frames),
            "sample_rate":self.peaks.as_ref().map(|p|p.sample_rate),
            "lanes":self.peaks.as_ref().map(|p|p.lanes)})
    }
}
impl Projects {
    /// Immutable prepared DJ waveform. Loading/polling use the DJ Library commands.
    /// Never reads disk or opens audio output; the caller may draw it with its native toolkit.
    pub fn dj_waveform(&self, track: u32) -> Option<Arc<Peaks>> {
        let state = &self.dj.waveform;
        (state.track == Some(track)).then(|| state.peaks.clone()).flatten()
    }
}
pub(crate) fn load(p: &mut Projects, params: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    let track = super::dj::id(params, "track")?;
    if !p.dj.catalog.tracks.iter().any(|t| t.id == track) {
        return Err("unknown DJ track".into());
    }
    let token = p.dj.waveform.serial.checked_add(1).ok_or("DJ waveform request ids exhausted")?;
    let queued = p.dj.queue(super::dj::Job::Waveform(token, track))?;
    p.dj.waveform.clear();
    p.dj.waveform.serial = token;
    p.dj.waveform.pending = Some(token);
    p.dj.waveform.track = Some(track);
    Ok(queued)
}
pub(crate) fn clear(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    p.dj.waveform.clear();
    Ok(json!({"cleared":true}))
}
