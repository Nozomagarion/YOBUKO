// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ sampler: eight slots, each holding a track of the DJ library, played from its start
//! by a pad straight into the master (`dj.sampler.*`). A slot is loaded by the DJ worker like a
//! deck; it plays on the same audio as the decks.

use crate::{CommandError, Projects};
use serde_json::{Value, json};
use std::sync::Arc;
use suhina_engine::{AudioData, DJ_SAMPLERS, DeckMonitor};

/// Where the DJ worker's loads for the sampler start: deck numbers above the two decks.
pub(crate) const FIRST: usize = suhina_engine::DJ_DECKS;

#[derive(Clone)]
struct Slot {
    track: Option<(suhina_dj::Track, Arc<AudioData>)>,
    pending: Option<(u64, suhina_dj::Track)>,
    error: Option<String>,
    gain: f32,
    /// The slot's monitor in the audio that last played it.
    monitor: Option<Arc<DeckMonitor>>,
}

impl Default for Slot {
    fn default() -> Self {
        Self { track: None, pending: None, error: None, gain: 1.0, monitor: None }
    }
}

#[derive(Default)]
pub(crate) struct Samplers {
    slots: [Slot; DJ_SAMPLERS],
}

impl Samplers {
    /// A load the DJ worker finished for slot `slot`, kept when it is still the one asked for.
    pub(crate) fn loaded(&mut self, slot: usize, token: u64, result: Result<Arc<AudioData>, String>) {
        let Some(s) = self.slots.get_mut(slot) else { return };
        if s.pending.as_ref().is_some_and(|(t, _)| *t == token)
            && let Some((_, track)) = s.pending.take()
        {
            match result {
                Ok(data) => {
                    s.track = Some((track, data));
                    s.error = None;
                }
                Err(e) => s.error = Some(e),
            }
        }
    }
}

fn slot(v: &Value) -> Result<usize, CommandError> {
    v["slot"]
        .as_u64()
        .and_then(|s| usize::try_from(s).ok())
        .filter(|s| (1..=DJ_SAMPLERS).contains(s))
        .map(|s| s - 1)
        .ok_or_else(|| CommandError(format!("slot must be 1–{DJ_SAMPLERS}")))
}

pub(crate) fn load(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    if p.dj.busy {
        return Err("wait for the current DJ loading job".into());
    }
    let i = slot(v)?;
    let track = super::dj::id(v, "track")?;
    let track = p.dj.catalog.tracks.iter().find(|t| t.id == track).cloned().ok_or("unknown DJ track")?;
    let token = p.dj.decks.serial.checked_add(1).ok_or("DJ deck request ids exhausted")?;
    let result = p.dj.queue(super::dj::Job::Deck(FIRST + i, token, track.id))?;
    p.dj.decks.serial = token;
    let s = &mut p.dj.decks.samplers.slots[i];
    s.pending = Some((token, track));
    s.error = None;
    Ok(result)
}

pub(crate) fn play(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    let i = slot(v)?;
    let s = &p.dj.decks.samplers.slots[i];
    if s.pending.is_some() {
        return Err("wait for this sampler slot to finish loading".into());
    }
    let data = s.track.as_ref().map(|(_, data)| Arc::clone(data)).ok_or("load a track into this sampler slot first")?;
    let gain = s.gain;
    let ctl = p.preview_session_mut()?.player_mut()?.controller_mut();
    ctl.sampler_gain(i, gain).map_err(CommandError)?;
    ctl.sampler_play(i, data).map_err(CommandError)?;
    let monitor = ctl.sampler_monitor(i);
    p.dj.decks.samplers.slots[i].monitor = monitor;
    Ok(json!({"playing":true}))
}

/// The session whose audio the slot last played in, while it is still there.
fn playing_controller(p: &mut Projects, i: usize) -> Option<&mut suhina_engine::Controller> {
    let monitor = p.dj.decks.samplers.slots.get(i)?.monitor.clone()?;
    let player = p.preview_session_mut().ok()?.player_mut().ok()?;
    let same = player.sampler_monitor(i).is_some_and(|m| Arc::ptr_eq(&m, &monitor));
    same.then(|| player.controller_mut())
}

pub(crate) fn stop(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = slot(v)?;
    if let Some(ctl) = playing_controller(p, i) {
        ctl.sampler_stop(i).map_err(CommandError)?;
    }
    Ok(json!({"stopped":true}))
}

pub(crate) fn clear(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = slot(v)?;
    if let Some(ctl) = playing_controller(p, i) {
        ctl.sampler_stop(i).map_err(CommandError)?;
    }
    let s = &mut p.dj.decks.samplers.slots[i];
    let gain = s.gain;
    *s = Slot { gain, ..Slot::default() };
    Ok(json!({"cleared":true}))
}

pub(crate) fn gain(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = slot(v)?;
    let gain = v["gain"]
        .as_f64()
        .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
        .map(|n| n as f32)
        .ok_or("gain must be 0–1")?;
    if let Some(ctl) = playing_controller(p, i) {
        ctl.sampler_gain(i, gain).map_err(CommandError)?;
    }
    p.dj.decks.samplers.slots[i].gain = gain;
    Ok(json!({"gain":gain}))
}

/// The slots as the page shows them.
pub(crate) fn snapshot(p: &Projects) -> Value {
    let slots: Vec<Value> =
        p.dj.decks
            .samplers
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let track = s.track.as_ref().map(|(t, _)| t);
                let monitor = s.monitor.as_ref().filter(|_| track.is_some());
                json!({
                    "slot": i + 1,
                    "track": track.map(|t| t.id),
                    "title": track.map(|t| t.title.as_str()),
                    "seconds": s.track.as_ref().map(|(_, d)| d.frames() as f64 / f64::from(d.sample_rate.max(1))),
                    "loading": s.pending.is_some(),
                    "playing": monitor.is_some_and(|m| m.playing()),
                    "position": monitor.map_or(0.0, |m| m.seconds()),
                    "gain": s.gain,
                    "error": s.error,
                })
            })
            .collect();
    json!(slots)
}
