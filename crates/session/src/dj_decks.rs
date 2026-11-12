// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{CommandError, ProjectId, Projects, Session};
use serde_json::{Value, json};
use std::sync::Arc;
use suhina_engine::{
    AudioData, DECK_BOOST_DB, DECK_FX_MAX_BEATS, DECK_FX_MIN_BEATS, DECK_FX_SLOTS, DECK_FX_UNITS, DECK_KILL_DB,
    DECK_TRIM_DB, DJ_DECKS, DeckFxKind, DeckFxUnit, DeckMeter, DeckMix, DeckMonitor, DeckSide, DeckTone,
};
use suhina_media::waveform::Peaks;

/// The most samples a deck holds decoded: 384 MiB, a 16-minute stereo track at 48 kHz.
pub(crate) const MAX_SAMPLES: usize = suhina_engine::DECK_MAX_SAMPLES;
/// A loaded track: its audio, its peaks and its colours.
pub(crate) type Track = (Arc<AudioData>, Arc<Peaks>, Arc<crate::WaveColors>);
pub(crate) type Loaded = (usize, u64, Result<Track, String>);
#[derive(Clone)]
struct Binding {
    target: Option<ProjectId>,
    monitor: Arc<DeckMonitor>,
}
struct Source {
    track: suhina_dj::Track,
    data: Arc<AudioData>,
    peaks: Arc<Peaks>,
    colors: Arc<crate::WaveColors>,
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
    /// Whether the key stays when the speed changes, and how far it is moved, in semitones.
    key_lock: bool,
    key: f64,
    /// Whether this load of the track is in the day's history yet.
    logged: bool,
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
            key_lock: false,
            key: 0.0,
            logged: false,
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
    states: [Deck; DJ_DECKS],
    pub(crate) serial: u64,
    loaded: Option<Loaded>,
    stopping: bool,
    clear_after: bool,
    mixer: DeckMix,
    /// The decks' master meter, from the audio that plays them.
    master: Option<Arc<DeckMeter>>,
    /// The two effect units.
    fx: [DeckFxUnit; DECK_FX_UNITS],
    /// The sampler's slots.
    pub(crate) samplers: crate::dj_sampler::Samplers,
    /// The mix's recording.
    pub(crate) recorder: crate::dj_record::Recorder,
    /// The tracks played, by day.
    pub(crate) history: crate::dj_history::History,
    /// Auto DJ: its queue and where it is.
    pub(crate) auto: crate::dj_auto::AutoDj,
    /// The microphone, and its meter in the audio that last heard it.
    mic: suhina_engine::DeckMic,
    mic_meter: Option<Arc<DeckMeter>>,
}
impl Decks {
    pub(crate) fn apply(&mut self, result: Loaded) {
        self.loaded = Some(result);
    }
}
/// The decks' names, in order.
pub(crate) const NAMES: [&str; DJ_DECKS] = ["A", "B", "C", "D"];
fn index(v: &Value) -> Result<usize, CommandError> {
    v["deck"].as_str().and_then(|d| NAMES.iter().position(|n| *n == d)).ok_or_else(|| "choose deck A, B, C or D".into())
}
/// A crossfader side's name.
fn side_name(side: DeckSide) -> &'static str {
    match side {
        DeckSide::Left => "left",
        DeckSide::Thru => "thru",
        DeckSide::Right => "right",
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
    for i in 0..DJ_DECKS {
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
        p.dj.decks.mixer.cue = [false; DJ_DECKS];
    }
    if p.dj.decks.stopping
        && let Err(e) = stop_all(p, &json!({}))
    {
        p.dj.error = Some(e.to_string());
    }
    let loaded = p.dj.decks.loaded.take();
    if let Some((i, token, result)) = loaded {
        // Above the decks, the load is for the sampler.
        match (i.checked_sub(crate::dj_sampler::FIRST), p.dj.decks.states.get_mut(i)) {
            (Some(slot), _) => p.dj.decks.samplers.loaded(slot, token, result.map(|(data, ..)| data)),
            (None, Some(d)) => deck_loaded(d, token, result),
            (None, None) => {}
        }
    }
    for i in 0..DJ_DECKS {
        let binding = p.dj.decks.states[i].binding.clone();
        if let Some(b) = binding
            && (b.target != p.active() || bound_session(p, &b, i).is_none())
        {
            p.dj.decks.states[i].binding = None;
            p.dj.decks.states[i].from = 0.0;
        }
    }
}
/// A load the DJ worker finished for a deck, kept when it is still the one asked for.
fn deck_loaded(d: &mut Deck, token: u64, result: Result<Track, String>) {
    {
        if d.pending.as_ref().is_some_and(|(t, _)| *t == token)
            && let Some((_, track)) = d.pending.take()
        {
            match result {
                Ok((data, peaks, colors)) => {
                    d.source = Some(Source { track, data, peaks, colors });
                    d.looping = None;
                    d.loop_on = false;
                    d.binding = None;
                    d.from = 0.0;
                    d.error = None;
                    d.logged = false;
                }
                Err(e) => d.error = Some(e),
            }
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
    let fx = p.dj.decks.fx;
    let mic = p.dj.decks.mic;
    let deck = &p.dj.decks.states[i];
    let (rate, tone, looping, key_lock, key) = (deck.rate, deck.tone, deck.playing_loop(), deck.key_lock, deck.key);
    let target = p.active();
    let player = p.preview_session_mut()?.player_mut()?;
    let ctl = player.controller_mut();
    ctl.deck_mix(mix).map_err(CommandError)?;
    ctl.deck_mic(mic).map_err(CommandError)?;
    let mic_meter = ctl.deck_mic_meter();
    for (unit, settings) in fx.into_iter().enumerate() {
        ctl.deck_fx(unit, settings).map_err(CommandError)?;
    }
    ctl.deck_rate(i, rate).map_err(CommandError)?;
    ctl.deck_tone(i, tone).map_err(CommandError)?;
    ctl.deck_loop(i, looping).map_err(CommandError)?;
    ctl.deck_key_lock(i, key_lock).map_err(CommandError)?;
    ctl.deck_key(i, key).map_err(CommandError)?;
    ctl.deck_play(i, data, from).map_err(CommandError)?;
    let monitor = ctl.deck_monitor(i).ok_or("missing DJ deck monitor")?;
    let master = ctl.deck_master_meter();
    let d = &mut p.dj.decks.states[i];
    d.binding = Some(Binding { target, monitor });
    if !d.logged
        && let Some(source) = d.source.as_ref()
    {
        d.logged = true;
        let track = source.track.clone();
        crate::dj_history::played(p, &track);
    }
    p.dj.decks.master = Some(master);
    p.dj.decks.mic_meter = Some(mic_meter);
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
    if !v["by"].is_null() {
        return shift(p, i, v, end);
    }
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
/// Moves a deck on (or back) by `by` seconds from where the audio has it when this lands.
fn shift(p: &mut Projects, i: usize, v: &Value, end: f64) -> Result<Value, CommandError> {
    let by =
        v["by"].as_f64().filter(|n| n.is_finite() && n.abs() <= end).ok_or("by must be seconds within the track")?;
    if !v["from"].is_null() {
        return Err("give from or by, not both".into());
    }
    match bound(p, i) {
        Some(s) => s.player_mut()?.controller_mut().deck_shift(i, by).map_err(CommandError)?,
        None => {
            let d = &mut p.dj.decks.states[i];
            d.from = (d.from + by).clamp(0.0, (end - 0.001).max(0.0));
        }
    }
    Ok(json!({"by":by}))
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
/// Locks or frees a deck's key, and moves it up or down: `lock` and `shift` (semitones,
/// −12 … 12), either or both.
pub(crate) fn key(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let i = index(v)?;
    let lock = match &v["lock"] {
        Value::Null => p.dj.decks.states[i].key_lock,
        lock => lock.as_bool().ok_or("lock must be true or false")?,
    };
    let shift = match &v["shift"] {
        Value::Null => p.dj.decks.states[i].key,
        shift => {
            shift.as_f64().filter(|n| n.is_finite() && n.abs() <= 12.0).ok_or("shift must be −12 to 12 semitones")?
        }
    };
    if let Some(s) = bound(p, i) {
        let ctl = s.player_mut()?.controller_mut();
        ctl.deck_key_lock(i, lock).map_err(CommandError)?;
        ctl.deck_key(i, shift).map_err(CommandError)?;
    }
    let d = &mut p.dj.decks.states[i];
    d.key_lock = lock;
    d.key = shift;
    Ok(json!({"lock":lock,"shift":shift}))
}
impl Projects {
    /// The waveform of what deck `deck` (0 for A, 1 for B) holds, to draw it; `None` while it
    /// holds nothing.
    pub fn dj_deck_peaks(&self, deck: usize) -> Option<Arc<Peaks>> {
        self.dj.decks.states.get(deck)?.source.as_ref().map(|s| Arc::clone(&s.peaks))
    }

    /// The colours of deck `deck`'s waveform: its lows, mids and highs over time.
    pub fn dj_deck_colors(&self, deck: usize) -> Option<Arc<crate::WaveColors>> {
        self.dj.decks.states.get(deck)?.source.as_ref().map(|s| Arc::clone(&s.colors))
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
    let boolean = |key: &str, old: Option<bool>| match (&v[key], old) {
        (Value::Null, Some(old)) => Ok(old),
        (b, _) => b.as_bool().ok_or_else(|| CommandError(format!("{key} must be a boolean"))),
    };
    // Decks A and B are always given; C and D, and the sides, keep what they were when left out.
    let mut levels = old.levels;
    let mut cue = old.cue;
    let mut sides = old.sides;
    for (i, name) in NAMES.iter().enumerate() {
        let required = i < 2;
        levels[i] = number(&format!("level{name}"), (!required).then_some(old.levels[i]))?;
        cue[i] = boolean(&format!("cue{name}"), (!required).then_some(old.cue[i]))?;
        let key = format!("side{name}");
        sides[i] = match &v[&key] {
            Value::Null => old.sides[i],
            side => match side.as_str() {
                Some("left") => DeckSide::Left,
                Some("thru") => DeckSide::Thru,
                Some("right") => DeckSide::Right,
                _ => return Err(CommandError(format!("{key} must be left, thru or right"))),
            },
        };
    }
    let mix = DeckMix {
        crossfade: number("crossfade", None)?,
        levels,
        cue,
        sides,
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
/// `dj.mic.set {on?, channel?, gain?, duck?}`: the microphone on the master. Switched on, it
/// opens the audio output like Play.
pub(crate) fn mic(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let mut mic = p.dj.decks.mic;
    let unit = |key: &str| -> Result<Option<f32>, CommandError> {
        match &v[key] {
            Value::Null => Ok(None),
            n => n
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                .map(|n| Some(n as f32))
                .ok_or_else(|| CommandError(format!("{key} must be 0–1"))),
        }
    };
    if let Some(gain) = unit("gain")? {
        mic.gain = gain;
    }
    if let Some(duck) = unit("duck")? {
        mic.duck = duck;
    }
    if !v["channel"].is_null() {
        let channel = v["channel"].as_u64().filter(|c| (1..=64).contains(c)).ok_or("channel must be 1–64")?;
        mic.channel = usize::try_from(channel - 1).unwrap_or(0);
    }
    if !v["on"].is_null() {
        mic.on = v["on"].as_bool().ok_or("on must be true or false")?;
    }
    let open = p.preview_session().and_then(|s| s.player()).is_some();
    if mic.on || open {
        let ctl = p.preview_session_mut()?.player_mut()?.controller_mut();
        ctl.deck_mic(mic).map_err(CommandError)?;
        let meter = ctl.deck_mic_meter();
        p.dj.decks.mic_meter = Some(meter);
    }
    p.dj.decks.mic = mic;
    Ok(json!({"on": mic.on}))
}
/// Moves the crossfader (0 = left, 1 = right), keeping the rest of the mixer.
pub(crate) fn crossfade(p: &mut Projects, x: f64) -> Result<(), CommandError> {
    let m = p.dj.decks.mixer;
    let params = json!({"crossfade": x.clamp(0.0, 1.0), "levelA": m.levels[0], "levelB": m.levels[1], "cueA": m.cue[0], "cueB": m.cue[1]});
    mixer(p, &params).map(|_| ())
}
/// Where the crossfader goes to hear deck `i` alone on its side: 0 left, 1 right; `None` for a
/// deck heard through the middle.
pub(crate) fn side_x(p: &Projects, i: usize) -> Option<f64> {
    match p.dj.decks.mixer.sides.get(i)? {
        DeckSide::Left => Some(0.0),
        DeckSide::Right => Some(1.0),
        DeckSide::Thru => None,
    }
}
/// What a deck holds and where it is, for Auto DJ.
pub(crate) struct DeckState {
    pub(crate) track: Option<u32>,
    pub(crate) loading: bool,
    pub(crate) playing: bool,
    pub(crate) position: f64,
    pub(crate) seconds: f64,
}
pub(crate) fn state(p: &Projects, i: usize) -> DeckState {
    let Some(d) = p.dj.decks.states.get(i) else {
        return DeckState { track: None, loading: false, playing: false, position: 0.0, seconds: 0.0 };
    };
    let active = d.binding.as_ref().filter(|b| b.target == p.active());
    let source = d.source.as_ref();
    DeckState {
        track: source.map(|s| s.track.id),
        loading: d.pending.is_some(),
        playing: active.is_some_and(|b| b.monitor.playing()),
        position: active.map_or(d.from, |b| b.monitor.seconds()),
        seconds: source.map_or(0.0, |s| s.data.frames() as f64 / f64::from(s.data.sample_rate.max(1))),
    }
}
/// Changes an effect unit: one slot's effect, on button or knob, and the unit's mix, length,
/// tempo or places, whichever are given.
pub(crate) fn fx(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let unit = v["unit"]
        .as_u64()
        .and_then(|u| usize::try_from(u).ok())
        .filter(|u| (1..=DECK_FX_UNITS).contains(u))
        .ok_or("unit must be 1 or 2")?
        - 1;
    let mut settings = p.dj.decks.fx[unit];
    let unit_number = |key: &str, range: std::ops::RangeInclusive<f64>| -> Result<Option<f32>, CommandError> {
        match &v[key] {
            Value::Null => Ok(None),
            n => n
                .as_f64()
                .filter(|n| n.is_finite() && range.contains(n))
                .map(|n| Some(n as f32))
                .ok_or_else(|| CommandError(format!("{key} must be {}–{}", range.start(), range.end()))),
        }
    };
    let flag = |key: &str| -> Result<Option<bool>, CommandError> {
        match &v[key] {
            Value::Null => Ok(None),
            b => b.as_bool().map(Some).ok_or_else(|| CommandError(format!("{key} must be true or false"))),
        }
    };
    if let Some(mix) = unit_number("mix", 0.0..=1.0)? {
        settings.mix = mix;
    }
    let beats = f64::from(DECK_FX_MIN_BEATS)..=f64::from(DECK_FX_MAX_BEATS);
    if let Some(beats) = unit_number("beats", beats)? {
        settings.beats = beats;
    }
    if let Some(bpm) = unit_number("bpm", 20.0..=400.0)? {
        settings.bpm = bpm;
    }
    for (place, key) in FX_PLACES.into_iter().enumerate() {
        if let Some(on) = flag(key)? {
            settings.targets[place] = on;
        }
    }
    let slot_fields = ["effect", "on", "meta"].iter().any(|k| !v[*k].is_null());
    match &v["slot"] {
        Value::Null if slot_fields => return Err("choose the slot (1–3) the effect, on or meta is for".into()),
        Value::Null => {}
        slot => {
            let slot = slot
                .as_u64()
                .and_then(|s| usize::try_from(s).ok())
                .filter(|s| (1..=DECK_FX_SLOTS).contains(s))
                .ok_or("slot must be 1, 2 or 3")?
                - 1;
            let s = &mut settings.slots[slot];
            match &v["effect"] {
                Value::Null => {}
                Value::String(id) if id == "none" => {
                    s.kind = None;
                    s.on = false;
                }
                effect => {
                    let id = effect.as_str().unwrap_or_default();
                    s.kind = Some(DeckFxKind::from_id(id).ok_or_else(|| {
                        let ids: Vec<_> = DeckFxKind::ALL.iter().map(|k| k.id()).collect();
                        CommandError(format!("effect must be none or one of {}", ids.join(", ")))
                    })?);
                }
            }
            if let Some(on) = flag("on")? {
                s.on = on && s.kind.is_some();
            }
            if let Some(meta) = unit_number("meta", 0.0..=1.0)? {
                s.meta = meta;
            }
        }
    }
    if p.dj.decks.states.iter().any(|s| s.binding.is_some()) && p.preview_session().and_then(|s| s.player()).is_some() {
        p.preview_session_mut()?.player_mut()?.controller_mut().deck_fx(unit, settings).map_err(CommandError)?;
    }
    p.dj.decks.fx[unit] = settings;
    Ok(fx_json(&settings))
}
/// Where an effect unit goes, in the engine's order: the decks, then the master.
const FX_PLACES: [&str; DJ_DECKS + 1] = ["deckA", "deckB", "deckC", "deckD", "master"];
fn fx_json(unit: &DeckFxUnit) -> Value {
    let mut places = serde_json::Map::new();
    for (key, on) in FX_PLACES.iter().zip(unit.targets) {
        places.insert((*key).to_owned(), json!(on));
    }
    let mut v = json!({
        "mix": unit.mix,
        "beats": unit.beats,
        "bpm": unit.bpm,
        "slots": unit.slots.iter().map(|s| json!({
            "effect": s.kind.map(|k| k.id()),
            "on": s.on,
            "meta": s.meta,
        })).collect::<Vec<_>>(),
    });
    if let Some(o) = v.as_object_mut() {
        o.extend(places);
    }
    v
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
                    "deck": NAMES.get(i),
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
                    "keyLock": d.key_lock,
                    "keyShift": d.key,
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
        "levelC": m.levels[2],
        "levelD": m.levels[3],
        "cueA": m.cue[0],
        "cueB": m.cue[1],
        "cueC": m.cue[2],
        "cueD": m.cue[3],
        "sideA": side_name(m.sides[0]),
        "sideB": side_name(m.sides[1]),
        "sideC": side_name(m.sides[2]),
        "sideD": side_name(m.sides[3]),
        "master": m.master,
        "phones": m.phones,
        "phonesMix": m.phones_mix,
        "masterMeter": master.map_or([0.0; 2], |m| m.take()),
        "separate_cue": p.output_choice().cue_pair.is_some(),
        "fx": p.dj.decks.fx.iter().map(fx_json).collect::<Vec<_>>(),
        "samplers": crate::dj_sampler::snapshot(p),
        "recording": crate::dj_record::snapshot(p),
        "autoDj": crate::dj_auto::snapshot(p),
        "mic": {
            "on": p.dj.decks.mic.on,
            "channel": p.dj.decks.mic.channel + 1,
            "gain": p.dj.decks.mic.gain,
            "duck": p.dj.decks.mic.duck,
            "meter": p.dj.decks.mic_meter.as_ref().map_or([0.0; 2], |m| m.take()),
        },
        "effects": DeckFxKind::ALL.iter().map(|k| json!({"id": k.id(), "name": k.name()})).collect::<Vec<_>>(),
    })
}
