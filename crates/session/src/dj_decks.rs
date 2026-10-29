// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{CommandError, ProjectId, Projects, Session};
use serde_json::{Value, json};
use std::sync::Arc;
use suhina_engine::{AudioData, PreviewMonitor};

pub(crate) const MAX_SAMPLES: usize = 128 * 1024 * 1024 / 4;
pub(crate) type Loaded = (usize, u64, Result<Arc<AudioData>, String>);
#[derive(Clone)]
struct Binding {
    target: Option<ProjectId>,
    monitor: Arc<PreviewMonitor>,
}
struct Source {
    track: suhina_dj::Track,
    data: Arc<AudioData>,
}
#[derive(Default)]
struct Deck {
    source: Option<Source>,
    pending: Option<(u64, suhina_dj::Track)>,
    binding: Option<Binding>,
    from: f64,
    error: Option<String>,
}
#[derive(Clone, Copy)]
struct Mixer {
    crossfade: f32,
    levels: [f32; 2],
    cue: [bool; 2],
}
impl Default for Mixer {
    fn default() -> Self {
        Self { crossfade: 0.5, levels: [1.0; 2], cue: [false; 2] }
    }
}
#[derive(Default)]
pub(crate) struct Decks {
    states: [Deck; 2],
    serial: u64,
    loaded: Option<Loaded>,
    stopping: bool,
    clear_after: bool,
    mixer: Mixer,
}
impl Decks {
    pub(crate) fn apply(&mut self, result: Loaded) {
        self.loaded = Some(result);
    }
}
fn index(v: &Value) -> Result<usize, CommandError> {
    match v["deck"].as_str() {
        Some("A") => Ok(0),
        Some("B") => Ok(1),
        _ => Err("choose deck A or B".into()),
    }
}
fn bound_session<'a>(p: &'a mut Projects, b: &Binding, i: usize) -> Option<&'a mut Session> {
    let s = if let Some(id) = b.target {
        p.get_mut(id)
    } else if p.active().is_none() && p.preview_session().and_then(|s| s.player()).is_some() {
        p.preview_session_mut().ok()
    } else {
        None
    }?;
    let monitor = s.player()?.deck_monitor(i)?;
    Arc::ptr_eq(&monitor, &b.monitor).then_some(s)
}
fn stop_one(p: &mut Projects, i: usize, clear: bool) -> Result<(), CommandError> {
    p.dj.decks.states[i].pending = None;
    let binding = p.dj.decks.states[i].binding.clone();
    if let Some(b) = binding
        && let Some(s) = bound_session(p, &b, i)
    {
        s.player_mut()?.controller_mut().deck_stop(i).map_err(CommandError)?;
    }
    let d = &mut p.dj.decks.states[i];
    d.binding = None;
    d.from = 0.0;
    if clear {
        d.source = None;
        d.error = None;
    }
    Ok(())
}
pub(crate) fn stop_all(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    p.dj.decks.stopping = true;
    for d in &mut p.dj.decks.states {
        d.pending = None;
    }
    let mut error = None;
    for i in 0..2 {
        if let Err(e) = stop_one(p, i, p.dj.decks.clear_after) {
            error = Some(e);
        }
    }
    if let Some(e) = error {
        return Err(e);
    }
    p.dj.decks.stopping = false;
    p.dj.decks.clear_after = false;
    Ok(json!({"stopped":true}))
}
pub(crate) fn invalidate(p: &mut Projects) {
    p.dj.decks.clear_after = true;
    let _ = stop_all(p, &json!({}));
}
pub(crate) fn sync(p: &mut Projects) {
    if p.output_choice().cue_pair.is_none() {
        p.dj.decks.mixer.cue = [false; 2];
    }
    if p.dj.decks.stopping
        && let Err(e) = stop_all(p, &json!({}))
    {
        p.dj.error = Some(e.to_string());
    }
    if let Some((i, token, result)) = p.dj.decks.loaded.take() {
        let d = &mut p.dj.decks.states[i];
        if d.pending.as_ref().is_some_and(|(t, _)| *t == token)
            && let Some((_, track)) = d.pending.take()
        {
            match result {
                Ok(data) => {
                    d.source = Some(Source { track, data });
                    d.binding = None;
                    d.from = 0.0;
                    d.error = None;
                }
                Err(e) => d.error = Some(e),
            }
        }
    }
    for i in 0..2 {
        let binding = p.dj.decks.states[i].binding.clone();
        if let Some(b) = binding
            && (b.target != p.active() || bound_session(p, &b, i).is_none())
        {
            p.dj.decks.states[i].binding = None;
            p.dj.decks.states[i].from = 0.0;
        }
    }
}
pub(crate) fn load(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    if p.dj.decks.stopping {
        return Err("wait for both DJ decks to stop before loading".into());
    }
    if p.dj.busy {
        return Err("wait for the current DJ loading job".into());
    }
    let i = index(v)?;
    let track = super::dj::id(v, "track")?;
    let track = p.dj.catalog.tracks.iter().find(|t| t.id == track).cloned().ok_or("unknown DJ track")?;
    let token = p.dj.decks.serial.checked_add(1).ok_or("DJ deck request ids exhausted")?;
    stop_one(p, i, false)?;
    let result = p.dj.queue(super::dj::Job::Deck(i, token, track.id))?;
    p.dj.decks.serial = token;
    p.dj.decks.states[i].pending = Some((token, track));
    p.dj.decks.states[i].error = None;
    Ok(result)
}
pub(crate) fn play(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    if p.dj.decks.stopping {
        return Err("wait for both DJ decks to stop before playing".into());
    }
    let i = index(v)?;
    let d = &p.dj.decks.states[i];
    if d.pending.is_some() {
        return Err("wait for this deck to finish loading".into());
    }
    let source = d.source.as_ref().ok_or("load a track into this deck first")?;
    let data = Arc::clone(&source.data);
    let from = d.from;
    let mix = p.dj.decks.mixer;
    let target = p.active();
    let player = p.preview_session_mut()?.player_mut()?;
    let ctl = player.controller_mut();
    ctl.deck_mix(mix.crossfade, mix.levels, mix.cue).map_err(CommandError)?;
    ctl.deck_play(i, data, from).map_err(CommandError)?;
    let monitor = ctl.deck_monitor(i).ok_or("missing DJ deck monitor")?;
    p.dj.decks.states[i].binding = Some(Binding { target, monitor });
    Ok(json!({"playing":true}))
}
pub(crate) fn pause(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let binding = p.dj.decks.states[i].binding.clone();
    if let Some(b) = binding
        && let Some(s) = bound_session(p, &b, i)
    {
        s.player_mut()?.controller_mut().deck_pause(i).map_err(CommandError)?;
    }
    Ok(json!({"paused":true}))
}
pub(crate) fn stop(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    stop_one(p, index(v)?, false)?;
    Ok(json!({"stopped":true}))
}
pub(crate) fn clear(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    stop_one(p, index(v)?, true)?;
    Ok(json!({"cleared":true}))
}
pub(crate) fn seek(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let source = p.dj.decks.states[i].source.as_ref().ok_or("load a deck first")?;
    let end = source.data.frames() as f64 / f64::from(source.data.sample_rate);
    let from = v["from"]
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.0 && *s < end)
        .ok_or("choose a deck position before the decoded end")?;
    let binding = p.dj.decks.states[i].binding.clone();
    if let Some(b) = binding
        && let Some(s) = bound_session(p, &b, i)
    {
        s.player_mut()?.controller_mut().deck_seek(i, from).map_err(CommandError)?;
    }
    p.dj.decks.states[i].from = from;
    Ok(json!({"position":from}))
}
pub(crate) fn mixer(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let number = |key: &str| {
        v[key]
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .map(|n| n as f32)
            .ok_or_else(|| CommandError(format!("{key} must be 0–1")))
    };
    let boolean = |key: &str| v[key].as_bool().ok_or_else(|| CommandError(format!("{key} must be a boolean")));
    let mix = Mixer {
        crossfade: number("crossfade")?,
        levels: [number("levelA")?, number("levelB")?],
        cue: [boolean("cueA")?, boolean("cueB")?],
    };
    if mix.cue.iter().any(|c| *c) && p.output_choice().cue_pair.is_none() {
        return Err("configure a separate headphone cue pair in Audio settings first".into());
    }
    if p.dj.decks.states.iter().any(|s| s.binding.is_some()) && p.preview_session().and_then(|s| s.player()).is_some() {
        p.preview_session_mut()?
            .player_mut()?
            .controller_mut()
            .deck_mix(mix.crossfade, mix.levels, mix.cue)
            .map_err(CommandError)?;
    }
    p.dj.decks.mixer = mix;
    Ok(json!({"changed":true}))
}
pub(crate) fn snapshot(p: &Projects) -> Value {
    let states:Vec<_>=p.dj.decks.states.iter().enumerate().map(|(i,d)| {
        let active=d.binding.as_ref().filter(|b|b.target==p.active());
        json!({"deck":if i==0 {"A"} else {"B"},"track":d.source.as_ref().map(|s|s.track.id),
            "title":d.source.as_ref().map(|s|s.track.title.as_str()),"seconds":d.source.as_ref().map(|s|s.data.frames() as f64/f64::from(s.data.sample_rate)),
            "position":active.map_or(d.from,|b|b.monitor.seconds()),"playing":active.is_some_and(|b|b.monitor.playing()),"loading":d.pending.is_some(),"error":d.error})
    }).collect();
    let m = p.dj.decks.mixer;
    json!({"decks":states,"crossfade":m.crossfade,"levelA":m.levels[0],"levelB":m.levels[1],"cueA":m.cue[0],"cueB":m.cue[1],"separate_cue":p.output_choice().cue_pair.is_some()})
}
