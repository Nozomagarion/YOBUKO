// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Auto DJ (`dj.autodj.*`): a queue of library tracks decks A and B play one after the other
//! on their own. While one deck plays, the next track loads on the other; at the playing
//! track's outro (its cue named "Outro", else a fade's length before its end) the other deck
//! starts at its intro (its cue named "Intro", else its start) and the crossfader moves across
//! over the fade. Auto DJ steps each time the page reads the library ([`step`]), so it follows
//! the positions the audio reports.

use crate::{CommandError, Projects};
use serde_json::{Value, json};
use std::collections::VecDeque;

/// The fade's length when none is set, in seconds.
const FADE: f64 = 8.0;
/// The longest fade, and the most tracks queued.
const MAX_FADE: f64 = 30.0;
const MAX_QUEUE: usize = 1000;
/// The cue names that mark a track's intro and outro.
pub(crate) const INTRO: &str = "Intro";
pub(crate) const OUTRO: &str = "Outro";

pub(crate) struct AutoDj {
    on: bool,
    queue: VecDeque<u32>,
    fade: f64,
    /// The track loading or loaded on a deck to play next: (deck, track).
    next: Option<(usize, u32)>,
    /// The fade under way: from deck, to deck, the from deck's place when it began, its length.
    fading: Option<(usize, usize, f64, f64)>,
    /// The deck Auto DJ last started, and whether the audio has said it plays yet: until it
    /// has, it counts as playing.
    live: Option<(usize, bool)>,
    error: Option<String>,
}

impl Default for AutoDj {
    fn default() -> Self {
        Self { on: false, queue: VecDeque::new(), fade: FADE, next: None, fading: None, live: None, error: None }
    }
}

/// A cue of `track` named `name`, in seconds.
fn cue(p: &Projects, track: u32, name: &str) -> Option<f64> {
    let set = p.dj.catalog.cues.iter().find(|s| s.track == track)?;
    set.points.iter().find(|c| c.name == name).map(|c| c.seconds)
}

fn deck(i: usize) -> Value {
    json!({"deck": if i == 0 { "A" } else { "B" }})
}

/// Asks a deck to load the queue's next track.
fn load_next(p: &mut Projects, i: usize) {
    let Some(&track) = p.dj.decks.auto.queue.front() else { return };
    let mut params = deck(i);
    params["track"] = json!(track);
    match crate::dj_decks::load(p, &params) {
        Ok(_) => {
            p.dj.decks.auto.queue.pop_front();
            p.dj.decks.auto.next = Some((i, track));
        }
        // Busy, or the deck is stopping: the next step tries again; a track gone is skipped.
        Err(e) if e.0.contains("unknown") => {
            p.dj.decks.auto.queue.pop_front();
        }
        Err(_) => {}
    }
}

/// Starts deck `i` at its track's intro.
fn start(p: &mut Projects, i: usize, track: u32) -> Result<(), CommandError> {
    let intro = cue(p, track, INTRO).unwrap_or(0.0);
    let mut params = deck(i);
    params["from"] = json!(intro);
    crate::dj_decks::seek(p, &params)?;
    crate::dj_decks::play(p, &deck(i))?;
    p.dj.decks.auto.live = Some((i, false));
    Ok(())
}

/// One step of Auto DJ: called each time the page reads the library.
pub(crate) fn step(p: &mut Projects) {
    if !p.dj.decks.auto.on {
        return;
    }
    // A deck that cannot play (no audio output, a file gone) stops Auto DJ rather than have it
    // load track after track.
    if let Err(e) = advance(p) {
        let a = &mut p.dj.decks.auto;
        a.error = Some(e.0);
        a.on = false;
        a.fading = None;
        a.live = None;
    }
}

/// Both decks, with the one Auto DJ just started counted as playing until the audio reports it.
fn decks(p: &mut Projects) -> [crate::dj_decks::DeckState; 2] {
    let mut decks = [crate::dj_decks::state(p, 0), crate::dj_decks::state(p, 1)];
    if let Some((i, seen)) = p.dj.decks.auto.live.as_mut()
        && let Some(d) = decks.get_mut(*i)
    {
        *seen |= d.playing;
        d.playing |= !*seen;
    }
    decks
}

fn advance(p: &mut Projects) -> Result<(), CommandError> {
    let decks = decks(p);
    // A fade under way: move the crossfader with the outgoing deck's place.
    if let Some((from, to, began, length)) = p.dj.decks.auto.fading {
        let gone = decks[from].playing.then(|| ((decks[from].position - began) / length.max(0.01)).clamp(0.0, 1.0));
        let progress = gone.unwrap_or(1.0);
        // From the outgoing deck's side of the crossfader to the incoming one's.
        let end = crate::dj_decks::side_x(p, to).unwrap_or(if to == 1 { 1.0 } else { 0.0 });
        let start = crate::dj_decks::side_x(p, from).unwrap_or(1.0 - end);
        crate::dj_decks::crossfade(p, start + (end - start) * progress)?;
        if progress >= 1.0 {
            p.dj.decks.auto.fading = None;
            crate::dj_decks::stop(p, &deck(from))?;
        }
        return Ok(());
    }
    let playing = (0..2).find(|&i| decks[i].playing);
    let Some(now) = playing else {
        // Nothing plays: start the deck that holds the next track, else load one.
        match p.dj.decks.auto.next {
            Some((i, track)) if decks[i].track == Some(track) && !decks[i].loading => {
                p.dj.decks.auto.next = None;
                let x = crate::dj_decks::side_x(p, i).unwrap_or(if i == 1 { 1.0 } else { 0.0 });
                crate::dj_decks::crossfade(p, x)?;
                start(p, i, track)?;
            }
            Some(_) => {}
            None if p.dj.decks.auto.queue.is_empty() => p.dj.decks.auto.on = false,
            None => load_next(p, 0),
        }
        return Ok(());
    };
    let other = 1 - now;
    if p.dj.decks.auto.next.is_none() && !decks[other].loading {
        load_next(p, other);
    }
    let Some(track) = decks[now].track else { return Ok(()) };
    let fade = p.dj.decks.auto.fade;
    let outro = cue(p, track, OUTRO).unwrap_or(decks[now].seconds - fade).max(0.0);
    if decks[now].position < outro {
        return Ok(());
    }
    match p.dj.decks.auto.next {
        Some((i, next)) if i == other && decks[other].track == Some(next) && !decks[other].loading => {
            p.dj.decks.auto.next = None;
            let length = fade.min((decks[now].seconds - decks[now].position).max(0.1));
            start(p, other, next)?;
            p.dj.decks.auto.fading = Some((now, other, decks[now].position, length));
        }
        // Nothing to go to: the track plays out, and Auto DJ stops with it.
        None if p.dj.decks.auto.queue.is_empty() => p.dj.decks.auto.on = false,
        _ => {}
    }
    Ok(())
}

/// `dj.autodj.set {on?, fade?}`.
pub(crate) fn set(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    if let Some(fade) = v.get("fade").filter(|f| !f.is_null()) {
        let fade = fade
            .as_f64()
            .filter(|f| f.is_finite() && (0.0..=MAX_FADE).contains(f))
            .ok_or("the fade must be 0–30 seconds")?;
        p.dj.decks.auto.fade = fade;
    }
    if let Some(on) = v.get("on").filter(|f| !f.is_null()) {
        let on = on.as_bool().ok_or("on must be true or false")?;
        let a = &mut p.dj.decks.auto;
        if on && a.queue.is_empty() && a.next.is_none() {
            return Err("add tracks to the Auto DJ queue first".into());
        }
        a.on = on;
        a.error = None;
        if !on {
            a.fading = None;
            a.live = None;
        }
        step(p);
    }
    Ok(json!({"on": p.dj.decks.auto.on, "fade": p.dj.decks.auto.fade}))
}

/// `dj.autodj.add {track}`: to the end of the queue.
pub(crate) fn add(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    let track = super::dj::id(v, "track")?;
    if !p.dj.catalog.tracks.iter().any(|t| t.id == track) {
        return Err("unknown DJ track".into());
    }
    let queue = &mut p.dj.decks.auto.queue;
    if queue.len() >= MAX_QUEUE {
        return Err("the Auto DJ queue is full".into());
    }
    queue.push_back(track);
    Ok(json!({"queued": queue.len()}))
}

/// `dj.autodj.remove {position}`: one-based, from the queue.
pub(crate) fn remove(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let position = v["position"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n >= 1)
        .ok_or("choose a position from 1")?;
    p.dj.decks.auto.queue.remove(position - 1).ok_or("no track at that place in the queue")?;
    Ok(json!({"queued": p.dj.decks.auto.queue.len()}))
}

/// `dj.autodj.clear`: empties the queue.
pub(crate) fn clear(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    p.dj.decks.auto.queue.clear();
    Ok(json!({"queued": 0}))
}

/// `dj.autodj.skip`: the next track fades in now.
pub(crate) fn skip(p: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    super::dj::ready(p)?;
    let decks = decks(p);
    let a = &p.dj.decks.auto;
    let Some(now) = (0..2).find(|&i| decks[i].playing).filter(|_| a.on && a.fading.is_none()) else {
        return Err("Auto DJ is not playing a track".into());
    };
    match a.next {
        Some((i, next)) if decks[i].track == Some(next) && !decks[i].loading => {
            let length = a.fade.max(0.1);
            p.dj.decks.auto.next = None;
            start(p, i, next)?;
            p.dj.decks.auto.fading = Some((now, i, decks[now].position, length));
            Ok(json!({"skipped": true}))
        }
        _ => Err("the next track is not loaded yet".into()),
    }
}

/// Auto DJ as the page shows it.
pub(crate) fn snapshot(p: &Projects) -> Value {
    let a = &p.dj.decks.auto;
    json!({
        "on": a.on,
        "fade": a.fade,
        "queue": a.queue,
        "next": a.next.map(|(_, t)| t),
        "fading": a.fading.is_some(),
        "error": a.error,
    })
}
