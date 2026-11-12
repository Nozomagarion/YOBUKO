// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ decks' effects, for [`crate::decks`]: two effect units of three effects each, put on
//! decks A–D or the master. Each effect has an on button and one knob (its "meta": how
//! much); the unit has a mix knob (how much of its effects is heard) and a length in beats that
//! the echoes and the sweeping effects follow at the deck's tempo.
//!
//! An effect is built on the controller's thread, for each place it can go, and handed to the
//! audio thread over a queue; the one it replaces goes back to be freed on the controller's
//! thread. An echo or reverb switched off stops taking sound in but lets what it holds ring out.
//! YOBUKO's own: nothing here touches Suhina's code.

use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};
use suhina_device_api::{Device, ProcessContext};
use suhina_device_sdk::Devices;
use suhina_dsp::lfo::{self, Shape};
use suhina_dsp::svf::{Svf, SvfCoefficients};
use suhina_dsp::{DelayLine, FirstOrder, FirstOrderCoefficients, OnePole, Smoother};

use crate::chain::MAX_BLOCK;

/// Effect units, effects in a unit, and the places a unit goes: decks A–D, then the master.
pub const UNITS: usize = 2;
pub const SLOTS: usize = 3;
pub const TARGETS: usize = crate::decks::DECKS + 1;
/// The shortest and longest length in beats.
pub const MIN_BEATS: f32 = 1.0 / 16.0;
pub const MAX_BEATS: f32 = 32.0;
/// The longest echo, in seconds.
const MAX_ECHO_SECONDS: f64 = 4.0;
/// How long an effect takes to come in or go out, so it never clicks.
const FADE_SECONDS: f64 = 0.02;
/// Below this, a ringing echo or reverb counts as silent.
const SILENT: f32 = 1e-5;
/// How long an echo or reverb switched off is still run after it last made a sound, in seconds:
/// longer than its longest repeat, so a quiet stretch between two repeats does not cut it.
const RING_SECONDS: f64 = 10.0;

/// An effect a slot can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    Echo,
    PingPong,
    Reverb,
    Filter,
    Flanger,
    Phaser,
    Trans,
    Autopan,
    Bitcrusher,
    Distortion,
    Noise,
}

impl FxKind {
    pub const ALL: [FxKind; 11] = [
        FxKind::Echo,
        FxKind::PingPong,
        FxKind::Reverb,
        FxKind::Filter,
        FxKind::Flanger,
        FxKind::Phaser,
        FxKind::Trans,
        FxKind::Autopan,
        FxKind::Bitcrusher,
        FxKind::Distortion,
        FxKind::Noise,
    ];

    /// Its id in commands.
    pub fn id(self) -> &'static str {
        match self {
            FxKind::Echo => "echo",
            FxKind::PingPong => "pingPong",
            FxKind::Reverb => "reverb",
            FxKind::Filter => "filter",
            FxKind::Flanger => "flanger",
            FxKind::Phaser => "phaser",
            FxKind::Trans => "trans",
            FxKind::Autopan => "autopan",
            FxKind::Bitcrusher => "bitcrusher",
            FxKind::Distortion => "distortion",
            FxKind::Noise => "noise",
        }
    }

    /// Its name on screen.
    pub fn name(self) -> &'static str {
        match self {
            FxKind::Echo => "Echo",
            FxKind::PingPong => "Ping Pong",
            FxKind::Reverb => "Reverb",
            FxKind::Filter => "Filter",
            FxKind::Flanger => "Flanger",
            FxKind::Phaser => "Phaser",
            FxKind::Trans => "Trans",
            FxKind::Autopan => "Autopan",
            FxKind::Bitcrusher => "Bitcrusher",
            FxKind::Distortion => "Distortion",
            FxKind::Noise => "Noise",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// Whether it adds to the sound (and rings out when switched off) rather than changing it.
    fn adds(self) -> bool {
        matches!(self, FxKind::Echo | FxKind::PingPong | FxKind::Reverb)
    }
}

/// One effect of a unit: which, whether it is on, and its knob, 0 … 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxSlot {
    pub kind: Option<FxKind>,
    pub on: bool,
    pub meta: f32,
}

impl Default for FxSlot {
    fn default() -> Self {
        Self { kind: None, on: false, meta: 0.5 }
    }
}

/// An effect unit: its three effects, how much of them is heard (0 … 1), the length the echoes
/// and sweeps follow in beats, the tempo they count beats at, and where it goes (decks A–D,
/// the master).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxUnit {
    pub slots: [FxSlot; SLOTS],
    pub mix: f32,
    pub beats: f32,
    pub bpm: f32,
    pub targets: [bool; TARGETS],
}

impl Default for FxUnit {
    fn default() -> Self {
        Self { slots: [FxSlot::default(); SLOTS], mix: 0.5, beats: 1.0, bpm: 120.0, targets: [false; TARGETS] }
    }
}

impl FxUnit {
    pub(crate) fn is_valid(&self) -> bool {
        let unit = |n: f32| n.is_finite() && (0.0..=1.0).contains(&n);
        unit(self.mix)
            && self.slots.iter().all(|s| unit(s.meta))
            && self.beats.is_finite()
            && (MIN_BEATS..=MAX_BEATS).contains(&self.beats)
            && self.bpm.is_finite()
            && (20.0..=400.0).contains(&self.bpm)
    }

    /// One beat's length the effects follow, in seconds.
    fn beat_seconds(&self) -> f64 {
        60.0 / f64::from(self.bpm.clamp(20.0, 400.0))
    }
}

/// A built effect, its state for one place.
pub(crate) struct Effect {
    kind: FxKind,
    body: Body,
}

enum Body {
    Echo(Echo),
    Reverb(Box<dyn Device>),
    Filter(Filter),
    Flanger(Flanger),
    Phaser(Phaser),
    Lfo(f64),
    Crusher(Crusher),
    Plain,
    Noise(Noise),
}

impl Effect {
    /// Builds `kind` at `rate`. Allocates: call it off the audio thread.
    fn new(kind: FxKind, rate: f64, devices: &Devices) -> Self {
        let body = match kind {
            FxKind::Echo | FxKind::PingPong => Body::Echo(Echo::new(rate)),
            FxKind::Reverb => match devices.create("reverb", None) {
                Some(mut device) => {
                    let _ = device.prepare(rate);
                    if let Some(mix) = device.info().param_index("mix") {
                        device.set_param(mix, 1.0);
                    }
                    device.reset();
                    Body::Reverb(device)
                }
                None => Body::Plain,
            },
            FxKind::Filter => Body::Filter(Filter::new()),
            FxKind::Flanger => Body::Flanger(Flanger::new(rate)),
            FxKind::Phaser => Body::Phaser(Phaser::default()),
            FxKind::Trans | FxKind::Autopan => Body::Lfo(0.0),
            FxKind::Bitcrusher => Body::Crusher(Crusher::default()),
            FxKind::Distortion => Body::Plain,
            FxKind::Noise => Body::Noise(Noise::new()),
        };
        Self { kind, body }
    }

    /// Processes `buffer` (interleaved stereo) in place: for an effect that adds, into what it
    /// adds; for the others, into the changed sound.
    fn process(&mut self, buffer: &mut [f32], meta: f32, beat: f64, rate: f64) {
        let kind = self.kind;
        match &mut self.body {
            Body::Echo(echo) => echo.process(buffer, meta, beat, rate, kind == FxKind::PingPong),
            Body::Reverb(device) => {
                if let Some(decay) = device.info().param_index("decay") {
                    let wanted = 0.4 + f64::from(meta) * 7.6;
                    if device.param(decay).is_none_or(|d| (d - wanted).abs() > 1e-3) {
                        device.set_param(decay, wanted);
                    }
                }
                device.process(&ProcessContext::default(), buffer);
            }
            Body::Filter(filter) => filter.process(buffer, meta, rate),
            Body::Flanger(flanger) => flanger.process(buffer, meta, beat, rate),
            Body::Phaser(phaser) => phaser.process(buffer, meta, beat, rate),
            Body::Lfo(phase) => {
                let step = 1.0 / (beat * rate).max(1.0);
                for frame in buffer.as_chunks_mut::<2>().0 {
                    *phase = (*phase + step).fract();
                    if kind == FxKind::Trans {
                        // Cut on the second half of each length, with soft edges.
                        let gate = (lfo::wave(Shape::Sine, *phase) * 8.0).clamp(-1.0, 1.0) * 0.5 + 0.5;
                        let depth = 0.5 + 0.5 * meta;
                        let gain = 1.0 - depth * (1.0 - gate as f32);
                        frame[0] *= gain;
                        frame[1] *= gain;
                    } else {
                        let pan = lfo::wave(Shape::Sine, *phase) as f32 * meta;
                        frame[0] *= 1.0 - pan.max(0.0);
                        frame[1] *= 1.0 + pan.min(0.0);
                    }
                }
            }
            Body::Crusher(crusher) => crusher.process(buffer, meta),
            Body::Plain => {
                // Distortion: a soft clip driven harder as the knob turns.
                let drive = 1.0 + meta * 24.0;
                let level = 1.0 / drive.sqrt();
                for s in buffer.iter_mut() {
                    *s = (*s * drive).tanh() * level;
                }
            }
            Body::Noise(noise) => noise.process(buffer, meta, rate),
        }
    }
}

/// An echo: a delay a number of beats long, fed back through a band-pass so the repeats darken.
struct Echo {
    lines: [DelayLine; 2],
    tone: [[FirstOrder; 2]; 2],
    /// The delay in frames, moved smoothly when the length or tempo changes; `None` before
    /// the first block, which starts it at its length.
    time: Option<OnePole>,
}

impl Echo {
    fn new(rate: f64) -> Self {
        let tone = |_| {
            [
                FirstOrder::new(FirstOrderCoefficients::lowpass(rate, 6_000.0)),
                FirstOrder::new(FirstOrderCoefficients::highpass(rate, 120.0)),
            ]
        };
        Self {
            lines: std::array::from_fn(|_| DelayLine::for_seconds(MAX_ECHO_SECONDS, rate)),
            tone: std::array::from_fn(tone),
            time: None,
        }
    }

    fn process(&mut self, buffer: &mut [f32], meta: f32, beat: f64, rate: f64, ping_pong: bool) {
        let feedback = f64::from(meta) * 0.9;
        let wanted = (beat * rate).clamp(1.0, MAX_ECHO_SECONDS * rate - 4.0);
        let time = self.time.get_or_insert_with(|| {
            let mut glide = OnePole::new(0.05, rate);
            glide.reset(wanted);
            glide
        });
        for frame in buffer.as_chunks_mut::<2>().0 {
            let delay = time.process(wanted);
            let out = [self.lines[0].read_linear(delay), self.lines[1].read_linear(delay)];
            let back: [f64; 2] = std::array::from_fn(|c| {
                let [low, high] = &mut self.tone[c];
                high.process(low.process(f64::from(out[c]))) * feedback
            });
            if ping_pong {
                // One side's repeats go to the other, so they bounce left and right.
                let mono = 0.5 * (frame[0] + frame[1]);
                self.lines[0].push(mono + back[1] as f32);
                self.lines[1].push(back[0] as f32);
            } else {
                self.lines[0].push(frame[0] + back[0] as f32);
                self.lines[1].push(frame[1] + back[1] as f32);
            }
            *frame = out;
        }
    }
}

/// A filter one knob sweeps: below the middle a low-pass closing, above it a high-pass opening.
struct Filter {
    filters: [Svf; 2],
    meta: f32,
}

impl Filter {
    fn new() -> Self {
        Self { filters: std::array::from_fn(|_| Svf::new(SvfCoefficients::default())), meta: -1.0 }
    }

    fn process(&mut self, buffer: &mut [f32], meta: f32, rate: f64) {
        if (meta - self.meta).abs() > 1e-4 {
            self.meta = meta;
            let nyquist = rate * 0.45;
            let x = f64::from(meta);
            let c = if x < 0.48 {
                let t = (0.48 - x) / 0.48;
                SvfCoefficients::lowpass(rate, (20_000.0 * (100.0f64 / 20_000.0).powf(t)).min(nyquist), 1.2)
            } else if x > 0.52 {
                let t = (x - 0.52) / 0.48;
                SvfCoefficients::highpass(rate, (20.0 * (8_000.0f64 / 20.0).powf(t)).min(nyquist), 1.2)
            } else {
                SvfCoefficients::default()
            };
            for f in &mut self.filters {
                f.set(c);
            }
        }
        for frame in buffer.as_chunks_mut::<2>().0 {
            for (s, f) in frame.iter_mut().zip(&mut self.filters) {
                *s = f.process(f64::from(*s)) as f32;
            }
        }
    }
}

/// A flanger: the sound mixed with itself a few milliseconds late, the delay swept over the
/// length.
struct Flanger {
    lines: [DelayLine; 2],
    phase: f64,
}

impl Flanger {
    fn new(rate: f64) -> Self {
        Self { lines: std::array::from_fn(|_| DelayLine::for_seconds(0.02, rate)), phase: 0.0 }
    }

    fn process(&mut self, buffer: &mut [f32], meta: f32, beat: f64, rate: f64) {
        let step = 1.0 / (beat * rate).max(1.0);
        let feedback = 0.2 + 0.6 * meta;
        for frame in buffer.as_chunks_mut::<2>().0 {
            self.phase = (self.phase + step).fract();
            let sweep = 0.5 + 0.5 * lfo::wave(Shape::Triangle, self.phase);
            let delay = (0.0005 + 0.0045 * sweep) * rate;
            for (s, line) in frame.iter_mut().zip(&mut self.lines) {
                let late = line.read_linear(delay);
                line.push(*s + late * feedback);
                *s = 0.6 * (*s + late);
            }
        }
    }
}

/// A phaser: four all-pass stages whose corner sweeps over the length, mixed with the sound.
#[derive(Default)]
struct Phaser {
    stages: [[(f32, f32); 4]; 2],
    back: [f32; 2],
    phase: f64,
    coefficient: f32,
}

impl Phaser {
    fn process(&mut self, buffer: &mut [f32], meta: f32, beat: f64, rate: f64) {
        let step = 1.0 / (beat * rate).max(1.0);
        let feedback = 0.7 * meta;
        for (i, frame) in buffer.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            self.phase = (self.phase + step).fract();
            if i % 16 == 0 {
                let sweep = 0.5 + 0.5 * lfo::wave(Shape::Sine, self.phase);
                let hz = 300.0 * 10f64.powf(sweep);
                let t = (std::f64::consts::PI * hz.min(rate * 0.45) / rate).tan();
                self.coefficient = ((t - 1.0) / (t + 1.0)) as f32;
            }
            let a = self.coefficient;
            for (c, s) in frame.iter_mut().enumerate() {
                let mut x = *s + self.back[c] * feedback;
                for (x1, y1) in &mut self.stages[c] {
                    let y = a * x + *x1 - a * *y1;
                    *x1 = x;
                    *y1 = if y.is_finite() && y.abs() > 1e-30 { y } else { 0.0 };
                    x = *y1;
                }
                self.back[c] = x;
                *s = 0.5 * (*s + x);
            }
        }
    }
}

/// A bitcrusher: fewer bits, and samples held for longer, as the knob turns.
#[derive(Default)]
struct Crusher {
    held: [f32; 2],
    count: f32,
}

impl Crusher {
    fn process(&mut self, buffer: &mut [f32], meta: f32) {
        let levels = 2f32.powf(16.0 - 13.0 * meta);
        let hold = 1.0 + meta * meta * 30.0;
        for frame in buffer.as_chunks_mut::<2>().0 {
            self.count += 1.0;
            if self.count >= hold {
                self.count -= hold;
                self.held = frame.map(|s| (s * levels).round() / levels);
            }
            *frame = self.held;
        }
    }
}

/// White noise through a band-pass the knob sweeps, on top of the sound.
struct Noise {
    filters: [Svf; 2],
    seed: u32,
    meta: f32,
}

impl Noise {
    fn new() -> Self {
        Self { filters: std::array::from_fn(|_| Svf::new(SvfCoefficients::default())), seed: 0x1234_5678, meta: -1.0 }
    }

    fn process(&mut self, buffer: &mut [f32], meta: f32, rate: f64) {
        if (meta - self.meta).abs() > 1e-4 {
            self.meta = meta;
            let hz = (200.0 * 60f64.powf(f64::from(meta))).min(rate * 0.45);
            for f in &mut self.filters {
                f.set(SvfCoefficients::bandpass(rate, hz, 0.8));
            }
        }
        for frame in buffer.as_chunks_mut::<2>().0 {
            for (s, f) in frame.iter_mut().zip(&mut self.filters) {
                self.seed ^= self.seed << 13;
                self.seed ^= self.seed >> 17;
                self.seed ^= self.seed << 5;
                let white = (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                *s += 0.3 * f.process(f64::from(white)) as f32;
            }
        }
    }
}

/// An effect for each place one slot's effect can go.
type Built = [Option<Effect>; TARGETS];

/// One slot's effect in one place, on the audio thread.
struct Slot {
    effect: Option<Effect>,
    /// How far it is on, 0 … 1, moved smoothly.
    on: Smoother,
    /// Frames since what it adds was last heard, while it still may ring.
    quiet: Option<usize>,
}

/// The controller's end: builds effects and sends them, and the units' settings.
pub(crate) struct FxSender {
    units: Producer<(usize, FxUnit)>,
    built: Producer<(usize, usize, Built)>,
    retired: Consumer<Effect>,
    kinds: [[Option<FxKind>; SLOTS]; UNITS],
    devices: Devices,
    rate: f64,
}

impl FxSender {
    /// Sends a unit's settings, building the effects whose kind changed first.
    pub(crate) fn set(&mut self, unit: usize, settings: FxUnit) -> Result<(), String> {
        while let Ok(old) = self.retired.pop() {
            drop(old);
        }
        let Some(kinds) = self.kinds.get_mut(unit) else { return Err("unknown DJ effect unit".into()) };
        if !settings.is_valid() {
            return Err("DJ effect values must be 0–1, its length 1/16 to 32 beats and its tempo 20–400 BPM".into());
        }
        if self.units.slots() == 0 {
            return Err("the audio engine has not taken the previous DJ effect changes".into());
        }
        for (slot, kind) in kinds.iter_mut().enumerate() {
            let wanted = settings.slots.get(slot).and_then(|s| s.kind);
            if *kind == wanted {
                continue;
            }
            let built: Built = std::array::from_fn(|_| wanted.map(|k| Effect::new(k, self.rate, &self.devices)));
            if self.built.push((unit, slot, built)).is_err() {
                return Err("the audio engine has not taken the previous DJ effects".into());
            }
            *kind = wanted;
        }
        self.units
            .push((unit, settings))
            .map_err(|_| "the audio engine has not taken the previous DJ effect changes".into())
    }
}

/// The renderer's end.
pub(crate) struct DeckFx {
    units: Option<Consumer<(usize, FxUnit)>>,
    built: Option<Consumer<(usize, usize, Built)>>,
    retired: Option<Producer<Effect>>,
    settings: [FxUnit; UNITS],
    slots: [[[Slot; SLOTS]; TARGETS]; UNITS],
    scratch: Vec<f32>,
    envelope: Vec<f32>,
    rate: f64,
}

impl DeckFx {
    pub(crate) fn new(rate: u32) -> Self {
        let rate = f64::from(rate.max(1));
        let fade = Smoother::ramp_frames(FADE_SECONDS, rate);
        Self {
            units: None,
            built: None,
            retired: None,
            settings: [FxUnit::default(); UNITS],
            slots: std::array::from_fn(|_| {
                std::array::from_fn(|_| {
                    std::array::from_fn(|_| Slot { effect: None, on: Smoother::new(0.0, fade), quiet: None })
                })
            }),
            scratch: vec![0.0; MAX_BLOCK * 2],
            envelope: vec![0.0; MAX_BLOCK],
            rate,
        }
    }

    pub(crate) fn sender(&mut self, devices: Devices) -> FxSender {
        let (units, units_in) = RingBuffer::new(16);
        let (built, built_in) = RingBuffer::new(UNITS * SLOTS * 2);
        let (retired_out, retired) = RingBuffer::new(UNITS * SLOTS * TARGETS * 4);
        self.units = Some(units_in);
        self.built = Some(built_in);
        self.retired = Some(retired_out);
        FxSender {
            units,
            built,
            retired,
            kinds: [[None; SLOTS]; UNITS],
            devices: Arc::clone(&devices),
            rate: self.rate,
        }
    }

    /// Takes what the controller sent.
    pub(crate) fn receive(&mut self) {
        while let Some((unit, settings)) = self.units.as_mut().and_then(|u| u.pop().ok()) {
            if let Some(s) = self.settings.get_mut(unit) {
                *s = settings;
            }
        }
        loop {
            // A swap retires up to one effect per place: take it only when they all fit.
            if !self.retired.as_ref().is_some_and(|r| r.slots() >= TARGETS) {
                return;
            }
            let Some((unit, slot, built)) = self.built.as_mut().and_then(|b| b.pop().ok()) else { return };
            let Some(places) = self.slots.get_mut(unit) else { continue };
            for (place, effect) in places.iter_mut().zip(built) {
                let Some(slot) = place.get_mut(slot) else { continue };
                if let Some(old) = std::mem::replace(&mut slot.effect, effect)
                    && let Some(retired) = self.retired.as_mut()
                {
                    let _full = retired.push(old);
                }
                slot.on.reset(0.0);
                slot.quiet = None;
            }
        }
    }

    /// Runs the units that go to `target` (0 deck A … 3 deck D, 4 the master) over `buffer`
    /// (interleaved stereo), in unit and slot order.
    pub(crate) fn process(&mut self, target: usize, buffer: &mut [f32]) {
        let frames = (buffer.len() / 2).min(MAX_BLOCK);
        let buffer = buffer.get_mut(..frames * 2).unwrap_or_default();
        for unit in 0..UNITS {
            let Some(settings) = self.settings.get(unit).copied() else { continue };
            let beat = settings.beat_seconds() * f64::from(settings.beats);
            let here = settings.targets.get(target).copied().unwrap_or(false);
            let Some(slots) = self.slots.get_mut(unit).and_then(|u| u.get_mut(target)) else { continue };
            for (slot, wanted) in slots.iter_mut().zip(settings.slots) {
                let Some(effect) = slot.effect.as_mut() else { continue };
                let on = if here && wanted.on { 1.0 } else { 0.0 };
                slot.on.set_target(on);
                let adds = effect.kind.adds();
                if on == 0.0 && slot.on.is_settled() && !(adds && slot.quiet.is_some()) {
                    continue;
                }
                let (Some(scratch), Some(envelope)) =
                    (self.scratch.get_mut(..frames * 2), self.envelope.get_mut(..frames))
                else {
                    return;
                };
                for e in envelope.iter_mut() {
                    *e = slot.on.next_value();
                }
                scratch.copy_from_slice(buffer);
                if adds {
                    // What goes in fades with the button; what rings goes on.
                    for (frame, e) in scratch.as_chunks_mut::<2>().0.iter_mut().zip(envelope.iter()) {
                        frame[0] *= *e;
                        frame[1] *= *e;
                    }
                }
                effect.process(scratch, wanted.meta, beat, self.rate);
                let mix = settings.mix;
                let mut peak = 0.0f32;
                for ((dst, wet), e) in
                    buffer.as_chunks_mut::<2>().0.iter_mut().zip(scratch.as_chunks::<2>().0).zip(envelope.iter())
                {
                    for (d, w) in dst.iter_mut().zip(wet) {
                        let w = if w.is_finite() { w.clamp(-16.0, 16.0) } else { 0.0 };
                        if adds {
                            peak = peak.max(w.abs());
                            *d += w * mix;
                        } else {
                            *d += (w - *d) * e * mix;
                        }
                    }
                }
                let limit = (RING_SECONDS * self.rate) as usize;
                slot.quiet = match slot.quiet {
                    _ if !adds => None,
                    _ if peak > SILENT || on > 0.0 => Some(0),
                    Some(quiet) if quiet + frames < limit => Some(quiet + frames),
                    _ => None,
                };
            }
        }
    }

    /// Whether any unit goes to `target`, or still rings there.
    pub(crate) fn active(&self, target: usize) -> bool {
        self.settings.iter().zip(&self.slots).any(|(settings, places)| {
            let here = settings.targets.get(target).copied().unwrap_or(false);
            places.get(target).is_some_and(|slots| {
                slots.iter().any(|s| s.effect.is_some() && (here || !s.on.is_settled() || s.quiet.is_some()))
            })
        })
    }
}
