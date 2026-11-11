// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One DJ deck's player, for [`crate::decks`]: a decoded track played at a speed (the pitch
//! follows), nudged a little faster or slower for a moment, and looped between two points. It
//! is YOBUKO's own, so Suhina's preview voice stays as Suhina has it.
//!
//! The controller sends what to play over a lock-free queue, and the track the deck lets go of
//! goes back over a second queue to be freed on the controller's thread. Nothing here
//! allocates, locks or frees on the audio thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::AudioData;
use crate::audio::CLICK_GUARD_SECONDS;

/// Commands waiting for the audio thread.
const QUEUE: usize = 16;
/// The slowest and fastest a deck plays, its tempo fader and nudge together.
pub(crate) const MIN_RATE: f64 = 0.5;
pub(crate) const MAX_RATE: f64 = 2.0;
/// The most a nudge (a pitch bend button, a jog wheel's side) changes the speed for a moment.
pub(crate) const MAX_BEND: f64 = 0.25;

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
    /// How fast it plays, 1 at the track's own speed.
    Rate(f64),
    /// A moment's nudge on top of the rate: 0 for none, 0.04 for 4 % faster.
    Bend(f64),
    /// The loop, from and to, in seconds of the track; `None` plays straight on.
    Loop(Option<(f64, f64)>),
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
    pub(crate) monitor: Arc<DeckMonitor>,
}

impl DeckVoice {
    pub(crate) fn new(sample_rate: u32) -> Self {
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
            sample_rate: sample_rate.max(1),
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
                }
                DeckCommand::Seek(seconds) => {
                    if let Some(data) = self.data.as_ref() {
                        let rate = f64::from(data.sample_rate.max(1));
                        self.position = (seconds.max(0.0) * rate).min(data.frames() as f64);
                        self.played = 0;
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
        let step = track_rate / f64::from(self.sample_rate) * (self.rate * (1.0 + self.bend)).clamp(MIN_RATE, MAX_RATE);
        let guard = (CLICK_GUARD_SECONDS * f64::from(self.sample_rate)).max(1.0);
        // The loop in the track's frames, when it is long enough to cross-fade across its end.
        let looping = self
            .looping
            .map(|(a, b)| ((a * track_rate).min(end), (b * track_rate).min(end)))
            .filter(|(a, b)| b - a > 2.0 * guard * step);
        for [l, r] in out.as_chunks_mut::<2>().0.iter_mut().take(frames) {
            if let Some((start, stop)) = looping
                && self.position >= stop
                && self.position < stop + (stop - start)
            {
                // Round the loop and carry on; a jump far beyond it (a seek) plays on instead.
                self.position -= stop - start;
            }
            if self.position >= end {
                self.playing = false;
                break;
            }
            let (mut left, mut right) = data.frame_at(self.position);
            if let Some((start, stop)) = looping
                && self.position >= start
                && self.position < stop
            {
                // Close to the loop's end, fade into what follows its start, so the loop has no
                // seam.
                let left_in_loop = (stop - self.position) / step;
                if left_in_loop < guard {
                    let t = (left_in_loop / guard) as f32;
                    let (al, ar) = data.frame_at(self.position - (stop - start));
                    left = left * t + al * (1.0 - t);
                    right = right * t + ar * (1.0 - t);
                }
            }
            let fade_in = ((self.played as f64 + 1.0) / guard).min(1.0);
            let fade_out = ((end - self.position) / step / guard).min(1.0);
            let gain = (fade_in * fade_out) as f32;
            *l += left * gain;
            *r += right * gain;
            self.position += step;
            self.played += 1;
        }
        let seconds = self.position.min(end) / track_rate;
        self.monitor.seconds.store(seconds.to_bits(), Ordering::Relaxed);
        self.monitor.playing.store(self.playing, Ordering::Relaxed);
        self.data = Some(data);
    }
}
