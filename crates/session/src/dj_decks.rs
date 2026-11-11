// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{CommandError, ProjectId, Projects, Session};
use serde_json::{Value, json};
use std::sync::Arc;
use suhina_engine::{AudioData, DECK_BOOST_DB, DECK_KILL_DB, DECK_TRIM_DB, DeckMeter, DeckMix, DeckMonitor, DeckTone};
use suhina_media::waveform::Peaks;

/// The most samples a deck holds decoded: 384 MiB, a 16-minute stereo track at 48 kHz.
pub(crate) const MAX_SAMPLES: usize = suhina_engine::DECK_MAX_SAMPLES;
pub(crate) type Loaded = (usize, u64, Result<(Arc<AudioData>, Arc<Peaks>), String>);
#[derive(Clone)]
struct Binding {
    target: Option<ProjectId>,
    monitor: Arc<DeckMonitor>,
}
struct Source {
    track: suhina_dj::Track,
    data: Arc<AudioData>,
    peaks: Arc<Peaks>,
}
struct Deck {
    source: Option<Source>,
    pending: Option<(u64, suhina_dj::Track)>,
    binding: Option<Binding>,
    from: f64,
    error: Option<String>,
    /// How fast it plays, 1 at the track's own speed.
    rate: f64,
    tone: DeckTone,
    /// The loop, from and to in seconds of the track, and whether it plays (a loop let go is
    /// kept, to play again with reloop).
    looping: Option<(f64, f64)>,
    loop_on: bool,
}
impl Default for Deck {
    fn default() -> Self {
        Self {
            source: None,
            pending: None,
            binding: None,
            from: 0.0,
            error: None,
            rate: 1.0,
            tone: DeckTone::default(),
            looping: None,
            loop_on: false,
        }
    }
}
impl Deck {
    /// The loop the audio plays, when it is on.
    fn playing_loop(&self) -> Option<(f64, f64)> {
        self.looping.filter(|_| self.loop_on)
    }
}
#[derive(Default)]
pub(crate) struct Decks {
    states: [Deck; 2],
    serial: u64,
    loaded: Option<Loaded>,
    stopping: bool,
    clear_after: bool,
    mixer: DeckMix,
    /// The decks' master meter, from the audio that plays them.
    master: Option<Arc<DeckMeter>>,
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
        d.looping = None;
        d.loop_on = false;
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
                Ok((data, peaks)) => {
                    d.source = Some(Source { track, data, peaks });
                    d.looping = None;
                    d.loop_on = false;
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
    let deck = &p.dj.decks.states[i];
    let (rate, tone, looping) = (deck.rate, deck.tone, deck.playing_loop());
    let target = p.active();
    let player = p.preview_session_mut()?.player_mut()?;
    let ctl = player.controller_mut();
    ctl.deck_mix(mix).map_err(CommandError)?;
    ctl.deck_rate(i, rate).map_err(CommandError)?;
    ctl.deck_tone(i, tone).map_err(CommandError)?;
    ctl.deck_loop(i, looping).map_err(CommandError)?;
    ctl.deck_play(i, data, from).map_err(CommandError)?;
    let monitor = ctl.deck_monitor(i).ok_or("missing DJ deck monitor")?;
    let master = ctl.deck_master_meter();
    p.dj.decks.states[i].binding = Some(Binding { target, monitor });
    p.dj.decks.master = Some(master);
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
pub(crate) fn rate(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let rate = v["rate"]
        .as_f64()
        .filter(|r| r.is_finite() && (0.5..=2.0).contains(r))
        .ok_or("rate must be 0.5–2 (1 is the track's own speed)")?;
    let binding = p.dj.decks.states[i].binding.clone();
    if let Some(b) = binding
        && let Some(s) = bound_session(p, &b, i)
    {
        s.player_mut()?.controller_mut().deck_rate(i, rate).map_err(CommandError)?;
    }
    p.dj.decks.states[i].rate = rate;
    Ok(json!({"rate":rate}))
}
pub(crate) fn tone(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let band = |key: &str| {
        v[key]
            .as_f64()
            .filter(|n| n.is_finite() && (f64::from(DECK_KILL_DB)..=f64::from(DECK_BOOST_DB)).contains(n))
            .map(|n| n as f32)
            .ok_or_else(|| CommandError(format!("{key} must be −40 to +6 dB")))
    };
    let filter = v["filter"]
        .as_f64()
        .filter(|n| n.is_finite() && (-1.0..=1.0).contains(n))
        .map(|n| n as f32)
        .ok_or("filter must be −1 to 1")?;
    let trim = match &v["trim"] {
        Value::Null => p.dj.decks.states[i].tone.trim,
        trim => trim
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() <= f64::from(DECK_TRIM_DB))
            .map(|n| n as f32)
            .ok_or("trim must be −12 to +12 dB")?,
    };
    let tone = DeckTone { eq: [band("low")?, band("mid")?, band("high")?], filter, trim };
    if let Some(s) = bound(p, i) {
        s.player_mut()?.controller_mut().deck_tone(i, tone).map_err(CommandError)?;
    }
    p.dj.decks.states[i].tone = tone;
    Ok(json!({"changed":true}))
}
/// The session playing deck `i`, while it still plays it.
fn bound(p: &mut Projects, i: usize) -> Option<&mut Session> {
    let binding = p.dj.decks.states[i].binding.clone()?;
    bound_session(p, &binding, i)
}
/// Sets, moves, plays or lets go of a deck's loop: `from` and `to` in seconds set it (both or
/// neither), `on` plays it or lets it go. A loop let go stays, for reloop.
pub(crate) fn set_loop(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let end = p.dj.decks.states[i]
        .source
        .as_ref()
        .map(|s| s.data.frames() as f64 / f64::from(s.data.sample_rate.max(1)))
        .ok_or("load a deck first")?;
    let region = match (&v["from"], &v["to"]) {
        (Value::Null, Value::Null) => p.dj.decks.states[i].looping,
        (from, to) => {
            let from = from.as_f64().filter(|n| n.is_finite() && *n >= 0.0 && *n < end);
            let to = to.as_f64().filter(|n| n.is_finite() && *n <= end);
            match (from, to) {
                (Some(from), Some(to)) if to - from >= 0.01 => Some((from, to)),
                _ => return Err("a loop starts in the track and ends at least 10 ms later".into()),
            }
        }
    };
    let on = match &v["on"] {
        Value::Null => region.is_some(),
        on => on.as_bool().ok_or("on must be true or false")?,
    };
    if on && region.is_none() {
        return Err("set a loop first: its start and end".into());
    }
    let playing = region.filter(|_| on);
    if let Some(s) = bound(p, i) {
        s.player_mut()?.controller_mut().deck_loop(i, playing).map_err(CommandError)?;
    }
    let d = &mut p.dj.decks.states[i];
    d.looping = region;
    d.loop_on = on;
    Ok(json!({"from":region.map(|r| r.0),"to":region.map(|r| r.1),"on":on}))
}
/// Nudges a playing deck faster or slower for as long as it is held (a pitch bend button, a
/// jog wheel's side): −0.25 … 0.25, 0 to let go.
pub(crate) fn bend(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let amount = v["amount"]
        .as_f64()
        .filter(|n| n.is_finite() && n.abs() <= 0.25)
        .ok_or("amount must be −0.25 to 0.25 (0 lets go)")?;
    if let Some(s) = bound(p, i) {
        s.player_mut()?.controller_mut().deck_bend(i, amount).map_err(CommandError)?;
    }
    Ok(json!({"amount":amount}))
}
impl Projects {
    /// The waveform of what deck `deck` (0 for A, 1 for B) holds, to draw it; `None` while it
    /// holds nothing.
    pub fn dj_deck_peaks(&self, deck: usize) -> Option<Arc<Peaks>> {
        self.dj.decks.states.get(deck)?.source.as_ref().map(|s| Arc::clone(&s.peaks))
    }
}
pub(crate) fn mixer(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let old = p.dj.decks.mixer;
    let number = |key: &str, old: Option<f32>| match (&v[key], old) {
        (Value::Null, Some(old)) => Ok(old),
        (n, _) => n
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .map(|n| n as f32)
            .ok_or_else(|| CommandError(format!("{key} must be 0–1"))),
    };
    let boolean = |key: &str| v[key].as_bool().ok_or_else(|| CommandError(format!("{key} must be a boolean")));
    let mix = DeckMix {
        crossfade: number("crossfade", None)?,
        levels: [number("levelA", None)?, number("levelB", None)?],
        cue: [boolean("cueA")?, boolean("cueB")?],
        master: number("master", Some(old.master))?,
        phones: number("phones", Some(old.phones))?,
        phones_mix: number("phonesMix", Some(old.phones_mix))?,
    };
    if mix.cue.iter().any(|c| *c) && p.output_choice().cue_pair.is_none() {
        return Err("configure a separate headphone cue pair in Audio settings first".into());
    }
    if p.dj.decks.states.iter().any(|s| s.binding.is_some()) && p.preview_session().and_then(|s| s.player()).is_some() {
        p.preview_session_mut()?.player_mut()?.controller_mut().deck_mix(mix).map_err(CommandError)?;
    }
    p.dj.decks.mixer = mix;
    Ok(json!({"changed":true}))
}
pub(crate) fn snapshot(p: &Projects) -> Value {
    let states: Vec<_> =
        p.dj.decks
            .states
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let active = d.binding.as_ref().filter(|b| b.target == p.active());
                let source = d.source.as_ref();
                // How loud since the last look: read once, here, for the meters.
                let meter = active.map_or([0.0; 2], |b| b.monitor.meter.take());
                json!({
                    "deck": if i == 0 { "A" } else { "B" },
                    "track": source.map(|s| s.track.id),
                    "title": source.map(|s| s.track.title.as_str()),
                    "seconds": source.map(|s| s.data.frames() as f64 / f64::from(s.data.sample_rate)),
                    "position": active.map_or(d.from, |b| b.monitor.seconds()),
                    "playing": active.is_some_and(|b| b.monitor.playing()),
                    "loading": d.pending.is_some(),
                    "error": d.error,
                    "bpm": source.and_then(|s| s.track.bpm),
                    "key": source.and_then(|s| s.track.key.as_deref()),
                    "rate": d.rate,
                    "low": d.tone.eq[0],
                    "mid": d.tone.eq[1],
                    "high": d.tone.eq[2],
                    "filter": d.tone.filter,
                    "trim": d.tone.trim,
                    "loopFrom": d.looping.map(|r| r.0),
                    "loopTo": d.looping.map(|r| r.1),
                    "loopOn": d.loop_on,
                    "meter": meter,
                })
            })
            .collect();
    let m = p.dj.decks.mixer;
    let master = p.dj.decks.master.as_ref().filter(|_| p.dj.decks.states.iter().any(|d| d.binding.is_some()));
    json!({
        "decks": states,
        "crossfade": m.crossfade,
        "levelA": m.levels[0],
        "levelB": m.levels[1],
        "cueA": m.cue[0],
        "cueB": m.cue[1],
        "master": m.master,
        "phones": m.phones,
        "phonesMix": m.phones_mix,
        "masterMeter": master.map_or([0.0; 2], |m| m.take()),
        "separate_cue": p.output_choice().cue_pair.is_some(),
    })
}
