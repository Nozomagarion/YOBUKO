// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One DJ deck's player, for [`crate::decks`]: a decoded track played at a speed (the pitch
//! follows, unless the key is locked), nudged a little faster or slower for a moment, moved up
//! or down in key, and looped between two points. It is YOBUKO's own, so Suhina's preview voice
//! stays as Suhina has it.
//!
//! With the key locked, or the key moved, the track goes through a time-stretcher
//! ([`suhina_stretch::Stretch`], built with the deck, off the audio thread): it is read at its
//! own pitch, as fast as the deck plays, and the stretcher keeps the pitch (or moves it by the
//! key). The stretcher is started afresh at each jump so its first sound is the jump's place.
//!
//! The controller sends what to play over a lock-free queue, and the track the deck lets go of
//! goes back over a second queue to be freed on the controller's thread. Nothing here
//! allocates, locks or frees on the audio thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};
use suhina_stretch::Stretch;

use crate::AudioData;
use crate::audio::CLICK_GUARD_SECONDS;

/// Commands waiting for the audio thread.
const QUEUE: usize = 16;
/// The slowest and fastest a deck plays, its tempo fader and nudge together.
pub(crate) const MIN_RATE: f64 = 0.5;
pub(crate) const MAX_RATE: f64 = 2.0;
/// The most a nudge (a pitch bend button, a jog wheel's side) changes the speed for a moment.
pub(crate) const MAX_BEND: f64 = 0.25;
/// The most a deck's key moves up or down, in semitones.
pub(crate) const MAX_KEY: f64 = 12.0;
/// Frames the stretcher makes per call.
const CHUNK: usize = 256;
/// A speed this close to the track's own, with the key where it is, plays without the
/// stretcher.
const UNITY: f64 = 1e-4;

/// What the controller asks a deck to do.
pub(crate) enum DeckCommand {
    /// Plays a track from `from` seconds, or carries on where it paused when it already holds
    /// this very track and has not reached its end.
    Play {
        data: Arc<AudioData>,
        from: f64,
    },
    Pause,
    /// Stops and lets the track go.
    Stop,
    Seek(f64),
    /// Moves by this many seconds of the track from wherever it is when the command lands (a
    /// beat alignment), forward above 0.
    Shift(f64),
    /// How fast it plays, 1 at the track's own speed.
    Rate(f64),
    /// A moment's nudge on top of the rate: 0 for none, 0.04 for 4 % faster.
    Bend(f64),
    /// The loop, from and to, in seconds of the track; `None` plays straight on.
    Loop(Option<(f64, f64)>),
    /// Whether the key stays where it is when the speed changes.
    KeyLock(bool),
    /// Moves the key up (above 0) or down, in semitones.
    Key(f64),
}

/// How loud something sounded, written by the audio thread and read (and reset) by whoever
/// draws its meter: the loudest sample since the last read, per side, 1 at full scale.
#[derive(Debug, Default)]
pub struct DeckMeter {
    left: AtomicU32,
    right: AtomicU32,
}

impl DeckMeter {
    /// The loudest left and right samples since the last call.
    pub fn take(&self) -> [f32; 2] {
        [f32::from_bits(self.left.swap(0, Ordering::Relaxed)), f32::from_bits(self.right.swap(0, Ordering::Relaxed))]
    }

    /// Raises the meter to these peaks. Non-negative floats order like their bits, so the most
    /// of the old and new peaks is a plain integer maximum.
    pub(crate) fn add(&self, left: f32, right: f32) {
        let clean = |p: f32| if p.is_finite() { p.clamp(0.0, 16.0) } else { 0.0 };
        self.left.fetch_max(clean(left).to_bits(), Ordering::Relaxed);
        self.right.fetch_max(clean(right).to_bits(), Ordering::Relaxed);
    }
}

/// Whether a deck plays, where, and how loud, written by the audio thread.
#[derive(Debug, Default)]
pub struct DeckMonitor {
    playing: AtomicBool,
    /// Seconds into the track (f64 bits).
    seconds: AtomicU64,
    /// After the deck's EQ, trim and channel fader, before the crossfader.
    pub meter: DeckMeter,
}

impl DeckMonitor {
    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn seconds(&self) -> f64 {
        f64::from_bits(self.seconds.load(Ordering::Relaxed))
    }
}

/// The controller's end.
pub(crate) struct DeckVoiceSender {
    commands: Producer<DeckCommand>,
    retired: Consumer<Arc<AudioData>>,
}

impl DeckVoiceSender {
    pub(crate) fn send(&mut self, command: DeckCommand) -> Result<(), String> {
        self.collect();
        self.commands.push(command).map_err(|_| "the audio engine has not taken the previous DJ deck commands".into())
    }

    /// Frees, here, the tracks the deck let go of.
    pub(crate) fn collect(&mut self) {
        while let Ok(data) = self.retired.pop() {
            drop(data);
        }
    }
}

/// The renderer's end.
pub(crate) struct DeckVoice {
    commands: Option<Consumer<DeckCommand>>,
    retired: Option<Producer<Arc<AudioData>>>,
    data: Option<Arc<AudioData>>,
    /// Where it plays, in the track's frames.
    position: f64,
    rate: f64,
    bend: f64,
    playing: bool,
    /// The loop, in seconds of the track.
    looping: Option<(f64, f64)>,
    /// Engine frames played since it started or jumped, for the fade in that keeps it from
    /// clicking.
    played: u64,
    sample_rate: u32,
    key_lock: bool,
    /// Semitones up or down.
    key: f64,
    stretch: Stretch,
    /// Whether the stretcher has been started at the place heard ([`Stretch::output_seek`]);
    /// false whenever the deck plays without it or jumps.
    primed: bool,
    /// Where in the track the stretcher is fed next, in the track's frames: ahead of what is
    /// heard by what the stretcher holds.
    feed: f64,
    /// The fraction of an input frame owed to the stretcher from the last call.
    carry: f64,
    /// The stretcher's input and output, interleaved stereo.
    input: Vec<f32>,
    output: Vec<f32>,
    pub(crate) monitor: Arc<DeckMonitor>,
}

impl DeckVoice {
    /// Builds the deck with its stretcher. Allocates: call it off the audio thread.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        let stretch = Stretch::new(2, sample_rate as f32);
        // A call's input at the fastest speed, or a start (`output_seek`) at it.
        let input = (CHUNK as f64 * MAX_RATE).ceil() as usize
            + stretch.input_latency()
            + (MAX_RATE * stretch.output_latency() as f64).ceil() as usize
            + 2;
        Self {
            commands: None,
            retired: None,
            data: None,
            position: 0.0,
            rate: 1.0,
            bend: 0.0,
            playing: false,
            looping: None,
            played: 0,
            sample_rate,
            key_lock: false,
            key: 0.0,
            stretch,
            primed: false,
            feed: 0.0,
            carry: 0.0,
            input: vec![0.0; input * 2],
            output: vec![0.0; CHUNK * 2],
            monitor: Arc::new(DeckMonitor::default()),
        }
    }

    /// Connects a new pair of queues; the controller gets the other ends.
    pub(crate) fn sender(&mut self) -> DeckVoiceSender {
        let (commands, incoming) = RingBuffer::new(QUEUE);
        let (outgoing, retired) = RingBuffer::new(QUEUE);
        self.commands = Some(incoming);
        self.retired = Some(outgoing);
        DeckVoiceSender { commands, retired }
    }

    pub(crate) fn playing(&self) -> bool {
        self.playing && self.data.is_some()
    }

    fn track_rate(&self) -> f64 {
        self.data.as_ref().map_or(1.0, |d| f64::from(d.sample_rate.max(1)))
    }

    /// Hands a track back to be freed on the controller's thread.
    fn retire(&mut self, data: Arc<AudioData>) {
        if let Some(retired) = self.retired.as_mut() {
            // There is always room: a command is only taken when there is.
            let _full = retired.push(data);
        }
    }

    /// Applies the commands that have arrived. A command waits while the queue back has no
    /// room for a track.
    pub(crate) fn receive(&mut self) {
        loop {
            if !self.retired.as_ref().is_some_and(|r| r.slots() > 0) {
                return;
            }
            let Some(command) = self.commands.as_mut().and_then(|c| c.pop().ok()) else { return };
            match command {
                DeckCommand::Play { data, from } => {
                    let same = self.data.as_ref().is_some_and(|d| Arc::ptr_eq(d, &data));
                    if same && self.position < data.frames() as f64 {
                        self.retire(data);
                    } else {
                        if let Some(old) = self.data.take() {
                            self.retire(old);
                        }
                        let rate = f64::from(data.sample_rate.max(1));
                        self.position = (from.max(0.0) * rate).min(data.frames() as f64);
                        self.data = Some(data);
                        self.played = 0;
                        self.primed = false;
                    }
                    self.playing = true;
                }
                DeckCommand::Pause => self.playing = false,
                DeckCommand::Stop => {
                    if let Some(old) = self.data.take() {
                        self.retire(old);
                    }
                    self.playing = false;
                    self.position = 0.0;
                    self.primed = false;
                }
                DeckCommand::Seek(seconds) => {
                    if let Some(data) = self.data.as_ref() {
                        let rate = f64::from(data.sample_rate.max(1));
                        self.position = (seconds.max(0.0) * rate).min(data.frames() as f64);
                        self.played = 0;
                        self.primed = false;
                    }
                }
                DeckCommand::Shift(by) => {
                    if let Some(data) = self.data.as_ref()
                        && by.is_finite()
                    {
                        let rate = f64::from(data.sample_rate.max(1));
                        self.position = (self.position + by * rate).clamp(0.0, data.frames() as f64);
                        self.played = 0;
                        self.primed = false;
                    }
                }
                DeckCommand::Rate(rate) => {
                    if rate.is_finite() {
                        self.rate = rate.clamp(MIN_RATE, MAX_RATE);
                    }
                }
                DeckCommand::Bend(bend) => {
                    if bend.is_finite() {
                        self.bend = bend.clamp(-MAX_BEND, MAX_BEND);
                    }
                }
                DeckCommand::Loop(region) => {
                    self.looping = region.filter(|(a, b)| a.is_finite() && b.is_finite() && *a >= 0.0 && b > a);
                    // What the stretcher already holds may lie past the new loop's end.
                    if self.primed {
                        self.primed = false;
                        self.played = 0;
                    }
                }
                DeckCommand::KeyLock(on) => self.key_lock = on,
                DeckCommand::Key(key) => {
                    if key.is_finite() {
                        self.key = key.clamp(-MAX_KEY, MAX_KEY);
                    }
                }
            }
            let seconds = self.position / self.track_rate();
            self.monitor.seconds.store(seconds.to_bits(), Ordering::Relaxed);
        }
    }

    /// Adds `frames` frames of the deck into `out` (interleaved stereo).
    pub(crate) fn finish(&mut self, out: &mut [f32], frames: usize) {
        if !self.playing {
            self.monitor.playing.store(false, Ordering::Relaxed);
            return;
        }
        // Held aside while it plays, and put back after: no copy, nothing freed.
        let Some(data) = self.data.take() else {
            self.monitor.playing.store(false, Ordering::Relaxed);
            return;
        };
        let track_rate = f64::from(data.sample_rate.max(1));
        let end = data.frames() as f64;
        // Track frames per engine frame at the track's own speed, and at the deck's.
        let natural = track_rate / f64::from(self.sample_rate);
        let tempo = (self.rate * (1.0 + self.bend)).clamp(MIN_RATE, MAX_RATE);
        let step = natural * tempo;
        let guard = (CLICK_GUARD_SECONDS * f64::from(self.sample_rate)).max(1.0);
        // The loop in the track's frames, when it is long enough to cross-fade across its end.
        let looping = self
            .looping
            .map(|(a, b)| ((a * track_rate).min(end), (b * track_rate).min(end)))
            .filter(|(a, b)| b - a > 2.0 * guard * step.max(natural));
        let stretching = self.key.abs() > UNITY || (self.key_lock && (tempo - 1.0).abs() > UNITY);
        let frames = frames.min(out.len() / 2);
        let out = out.get_mut(..frames * 2).unwrap_or_default();
        if stretching {
            self.stretched(&data, out, Track { end, natural, step, guard, looping }, tempo);
        } else {
            self.primed = false;
            for [l, r] in out.as_chunks_mut::<2>().0 {
                self.position = wrap(looping, self.position);
                if self.position >= end {
                    self.playing = false;
                    break;
                }
                let (left, right) = loop_frame(&data, looping, self.position, step, guard);
                let gain = self.gain(end, step, guard);
                *l += left * gain;
                *r += right * gain;
                self.position += step;
                self.played += 1;
            }
        }
        let seconds = self.position.min(end) / track_rate;
        self.monitor.seconds.store(seconds.to_bits(), Ordering::Relaxed);
        self.monitor.playing.store(self.playing, Ordering::Relaxed);
        self.data = Some(data);
    }

    /// The fade in after a start or jump, times the fade out before the track's end.
    fn gain(&self, end: f64, step: f64, guard: f64) -> f32 {
        let fade_in = ((self.played as f64 + 1.0) / guard).min(1.0);
        let fade_out = ((end - self.position) / step / guard).min(1.0);
        (fade_in * fade_out) as f32
    }

    /// Plays through the stretcher: the track is read at its own pitch, `tempo` times as fast,
    /// and the stretcher keeps the pitch, or moves it by the key (and by the speed too when the
    /// key is not locked).
    fn stretched(&mut self, data: &AudioData, out: &mut [f32], track: Track, tempo: f64) {
        let Track { end, natural, step, guard, looping } = track;
        let pitch = if self.key_lock { 1.0 } else { tempo };
        self.stretch.set_transpose_factor((pitch * 2f64.powf(self.key / 12.0)) as f32, 0.0);
        if !self.primed {
            // Start afresh so that the next sound is the place heard.
            let length = self.stretch.output_seek_length(tempo as f32).min(self.input.len() / 2);
            let mut feed = self.position;
            for frame in self.input.as_chunks_mut::<2>().0.iter_mut().take(length) {
                feed = wrap(looping, feed);
                let (l, r) = loop_frame(data, looping, feed, natural, guard);
                *frame = [l, r];
                feed += natural;
            }
            self.stretch.output_seek(self.input.get(..length * 2).unwrap_or_default());
            self.feed = feed;
            self.carry = 0.0;
            self.primed = true;
        }
        for block in out.chunks_mut(CHUNK * 2) {
            let frames = block.len() / 2;
            self.carry += frames as f64 * tempo;
            let wanted = (self.carry.floor().max(0.0) as usize).min(self.input.len() / 2);
            self.carry -= wanted as f64;
            for frame in self.input.as_chunks_mut::<2>().0.iter_mut().take(wanted) {
                self.feed = wrap(looping, self.feed);
                let (l, r) = loop_frame(data, looping, self.feed, natural, guard);
                *frame = [l, r];
                self.feed += natural;
            }
            let Some(made) = self.output.get_mut(..frames * 2) else { return };
            self.stretch.process(self.input.get(..wanted * 2).unwrap_or_default(), made);
            for (i, [l, r]) in block.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                self.position = wrap(looping, self.position);
                if self.position >= end {
                    self.playing = false;
                    return;
                }
                let gain = self.gain(end, step, guard);
                let sample = |c: usize| self.output.get(i * 2 + c).copied().unwrap_or(0.0);
                let (left, right) = (sample(0), sample(1));
                *l += left * gain;
                *r += right * gain;
                self.position += step;
                self.played += 1;
            }
        }
    }
}

/// The track as a deck plays it for one block, in the track's frames.
#[derive(Clone, Copy)]
struct Track {
    end: f64,
    /// Track frames per engine frame at the track's own speed, and at the deck's.
    natural: f64,
    step: f64,
    /// The click guard, in engine frames.
    guard: f64,
    looping: Option<(f64, f64)>,
}

/// Rounds the loop: a place just past its end goes back by its length; a place far beyond it (a
/// seek) is left alone.
fn wrap(looping: Option<(f64, f64)>, position: f64) -> f64 {
    match looping {
        Some((start, stop)) if position >= stop && position < stop + (stop - start) => position - (stop - start),
        _ => position,
    }
}

/// The track's frame at `position`; close to the loop's end, faded into what follows its start,
/// so the loop has no seam. `step` is how far one engine frame moves.
fn loop_frame(data: &AudioData, looping: Option<(f64, f64)>, position: f64, step: f64, guard: f64) -> (f32, f32) {
    let (left, right) = data.frame_at(position);
    if let Some((start, stop)) = looping
        && position >= start
        && position < stop
    {
        let left_in_loop = (stop - position) / step;
        if left_in_loop < guard {
            let t = (left_in_loop / guard) as f32;
            let (al, ar) = data.frame_at(position - (stop - start));
            return (left * t + al * (1.0 - t), right * t + ar * (1.0 - t));
        }
    }
    (left, right)
}
