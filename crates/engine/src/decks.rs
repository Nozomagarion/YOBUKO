// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Two sample-only Preview voices, mixed after the graph through the same output. Each deck
//! plays at its own tempo (the pitch follows) through its own three-band EQ and filter
//! ([`Tone`]) before the channel level and the crossfader, then the master level. The decks
//! themselves are [`crate::deck_voice`]; meters read how loud each deck and the master are.
use crate::AudioData;
use crate::chain::MAX_BLOCK;
use crate::deck_fx::{DeckFx, FxSender, FxUnit};
use crate::deck_record::{DeckRecording, DeckTap};
use crate::deck_voice::{
    DeckCommand, DeckMeter, DeckMonitor, DeckVoice, DeckVoiceSender, MAX_BEND, MAX_KEY, MAX_RATE, MIN_RATE,
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::Arc;
use suhina_dsp::Smoother;
use suhina_dsp::svf::{Svf, SvfCoefficients};

/// A deck's EQ and filter, set from the mixer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    /// Low, mid and high bands in dB: 0 leaves the band as it is, [`KILL_DB`] all but removes it.
    pub eq: [f32; 3],
    /// The filter knob, −1 … 1: below 0 a low-pass that closes towards −1, above 0 a high-pass
    /// that closes towards 1, and the middle leaves the sound alone.
    pub filter: f32,
    /// The trim (gain) knob in dB, ± [`TRIM_DB`]: brings a quiet or loud track to the others.
    pub trim: f32,
}
impl Default for Tone {
    fn default() -> Self {
        Self { eq: [0.0; 3], filter: 0.0, trim: 0.0 }
    }
}
/// The most the trim knob lifts or lowers a deck.
pub const TRIM_DB: f32 = 12.0;
/// The most samples a deck holds decoded: 384 MiB, a 16-minute stereo track at 48 kHz.
pub const MAX_SAMPLES: usize = 384 * 1024 * 1024 / 4;
/// The deepest cut of an EQ band (a "kill"), and the most it lifts.
pub const KILL_DB: f32 = -40.0;
pub const BOOST_DB: f32 = 6.0;
/// Where the bands meet: the low shelf below, the mid bell, the high shelf above.
const LOW_HZ: f64 = 250.0;
const MID_HZ: f64 = 1_000.0;
const HIGH_HZ: f64 = 3_000.0;
/// The filter knob's dead middle, where nothing is filtered.
const FILTER_REST: f32 = 0.02;

impl Tone {
    /// Whether the EQ and filter leave the sound alone (the trim is a plain gain, applied apart).
    fn is_flat(&self) -> bool {
        self.eq.iter().all(|g| g.abs() < 0.01) && self.filter.abs() < FILTER_REST
    }
    /// The four filters' settings at `rate`: low shelf, mid bell, high shelf, then the filter.
    fn coefficients(&self, rate: f64) -> [SvfCoefficients; 4] {
        let band = |g: f32| f64::from(g.clamp(KILL_DB, BOOST_DB));
        let nyquist = rate * 0.45;
        let filter = if self.filter <= -FILTER_REST {
            // From 20 kHz at rest down to 150 Hz fully turned, evenly in octaves.
            let x = f64::from((-self.filter - FILTER_REST) / (1.0 - FILTER_REST)).clamp(0.0, 1.0);
            SvfCoefficients::lowpass(rate, (20_000.0 * (150.0f64 / 20_000.0).powf(x)).min(nyquist), 0.9)
        } else if self.filter >= FILTER_REST {
            // From 20 Hz at rest up to 8 kHz fully turned.
            let x = f64::from((self.filter - FILTER_REST) / (1.0 - FILTER_REST)).clamp(0.0, 1.0);
            SvfCoefficients::highpass(rate, (20.0 * (8_000.0f64 / 20.0).powf(x)).min(nyquist), 0.9)
        } else {
            SvfCoefficients::default()
        };
        [
            SvfCoefficients::low_shelf(rate, LOW_HZ, 0.707, band(self.eq[0])),
            SvfCoefficients::bell(rate, MID_HZ.min(nyquist), 0.7, band(self.eq[1])),
            SvfCoefficients::high_shelf(rate, HIGH_HZ.min(nyquist), 0.707, band(self.eq[2])),
            filter,
        ]
    }
}

/// A deck's four filters, one set per channel.
struct Shaper {
    filters: [[Svf; 4]; 2],
    tone: Tone,
}
impl Shaper {
    fn new() -> Self {
        Self {
            filters: std::array::from_fn(|_| std::array::from_fn(|_| Svf::new(SvfCoefficients::default()))),
            tone: Tone::default(),
        }
    }
    fn set(&mut self, tone: Tone, rate: f64) {
        if tone == self.tone {
            return;
        }
        self.tone = tone;
        let coefficients = tone.coefficients(rate);
        for channel in &mut self.filters {
            for (filter, c) in channel.iter_mut().zip(coefficients) {
                filter.set(c);
            }
        }
    }
    fn process(&mut self, buffer: &mut [f32]) {
        if self.tone.is_flat() {
            for channel in &mut self.filters {
                for filter in channel {
                    filter.reset();
                }
            }
            return;
        }
        for frame in buffer.as_chunks_mut::<2>().0 {
            for (sample, filters) in frame.iter_mut().zip(&mut self.filters) {
                let mut x = f64::from(*sample);
                for filter in filters.iter_mut() {
                    x = filter.process(x);
                }
                *sample = x as f32;
            }
        }
    }
}

/// The mixer: the crossfader, each channel's fader and headphone button, the master level, and
/// what the headphones hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    /// 0 all deck A, 1 all deck B.
    pub crossfade: f32,
    /// Each channel's fader, 0 … 1.
    pub levels: [f32; 2],
    /// Which decks the headphones hear.
    pub cue: [bool; 2],
    /// The master level, 0 … 1 (1 leaves the mix as it is).
    pub master: f32,
    /// The headphones' level, 0 … 1.
    pub phones: f32,
    /// What the headphones hear: 0 only the decks cued, 1 only the master, between a blend.
    pub phones_mix: f32,
}
impl Default for Mix {
    fn default() -> Self {
        Self { crossfade: 0.5, levels: [1.0; 2], cue: [false; 2], master: 1.0, phones: 1.0, phones_mix: 0.0 }
    }
}
impl Mix {
    fn is_valid(&self) -> bool {
        let unit = |n: f32| n.is_finite() && (0.0..=1.0).contains(&n);
        unit(self.crossfade)
            && self.levels.iter().all(|n| unit(*n))
            && unit(self.master)
            && unit(self.phones)
            && unit(self.phones_mix)
    }
}
/// The headroom the decks leave, so two full tracks summed do not clip: −6 dB.
const HEADROOM: f32 = 0.501_187_2;
/// The sampler's slots: short sounds a pad plays from their start, straight to the master.
pub const SAMPLERS: usize = 8;

pub(crate) struct DeckSender {
    pub voices: [DeckVoiceSender; 2],
    pub monitors: [Arc<DeckMonitor>; 2],
    pub master: Arc<DeckMeter>,
    pub mix: Producer<Mix>,
    pub tone: Producer<(usize, Tone)>,
    pub fx: FxSender,
    pub samplers: [DeckVoiceSender; SAMPLERS],
    pub sampler_monitors: [Arc<DeckMonitor>; SAMPLERS],
    pub sampler_gains: Producer<(usize, f32)>,
    /// The master's recording, while no writer holds it.
    pub recording: Option<DeckRecording>,
}
pub(crate) struct Decks {
    voices: [DeckVoice; 2],
    mix: Option<Consumer<Mix>>,
    tone: Option<Consumer<(usize, Tone)>>,
    shapers: [Shaper; 2],
    rate: f64,
    buffer: Vec<f32>,
    /// Both decks after their faders and the crossfader, before the master level.
    sum: Vec<f32>,
    main: [Smoother; 2],
    cue: [Smoother; 2],
    master_gain: Smoother,
    phones_master: Smoother,
    master: Arc<DeckMeter>,
    settings: Mix,
    fx: DeckFx,
    samplers: [DeckVoice; SAMPLERS],
    sampler_gains: Option<Consumer<(usize, f32)>>,
    sampler_levels: [Smoother; SAMPLERS],
    tap: DeckTap,
    recording: Option<DeckRecording>,
}
impl Decks {
    pub fn new(rate: u32) -> Self {
        let ramp = Smoother::ramp_frames(0.01, f64::from(rate));
        let (tap, recording) = crate::deck_record::tap(rate);
        Self {
            voices: std::array::from_fn(|_| DeckVoice::new(rate)),
            mix: None,
            tone: None,
            shapers: [Shaper::new(), Shaper::new()],
            rate: f64::from(rate.max(1)),
            buffer: vec![0.0; MAX_BLOCK * 2],
            sum: vec![0.0; MAX_BLOCK * 2],
            main: [Smoother::new(0.0, ramp); 2],
            cue: [Smoother::new(0.0, ramp); 2],
            master_gain: Smoother::new(1.0, ramp),
            phones_master: Smoother::new(0.0, ramp),
            master: Arc::new(DeckMeter::default()),
            settings: Mix::default(),
            fx: DeckFx::new(rate),
            samplers: std::array::from_fn(|_| DeckVoice::new(rate)),
            sampler_gains: None,
            sampler_levels: [Smoother::new(1.0, ramp); SAMPLERS],
            tap,
            recording: Some(recording),
        }
    }
    pub fn sender(&mut self, devices: suhina_device_sdk::Devices) -> DeckSender {
        let (tx, rx) = RingBuffer::new(16);
        self.mix = Some(rx);
        let (tone, tones) = RingBuffer::new(32);
        self.tone = Some(tones);
        DeckSender {
            voices: [self.voices[0].sender(), self.voices[1].sender()],
            monitors: std::array::from_fn(|i| Arc::clone(&self.voices[i].monitor)),
            master: Arc::clone(&self.master),
            mix: tx,
            tone,
            fx: self.fx.sender(devices),
            samplers: std::array::from_fn(|i| self.samplers[i].sender()),
            sampler_monitors: std::array::from_fn(|i| Arc::clone(&self.samplers[i].monitor)),
            sampler_gains: {
                let (tx, rx) = RingBuffer::new(64);
                self.sampler_gains = Some(rx);
                tx
            },
            recording: self.recording.take(),
        }
    }
    pub fn finish(&mut self, out: &mut [f32], cue: &mut [f32], frames: usize, separate: bool) {
        while let Some(mix) = self.mix.as_mut().and_then(|r| r.pop().ok()) {
            self.settings = mix;
        }
        while let Some((deck, tone)) = self.tone.as_mut().and_then(|r| r.pop().ok()) {
            if let Some(shaper) = self.shapers.get_mut(deck) {
                shaper.set(tone, self.rate);
            }
        }
        self.fx.receive();
        while let Some((i, gain)) = self.sampler_gains.as_mut().and_then(|r| r.pop().ok()) {
            if let Some(level) = self.sampler_levels.get_mut(i) {
                level.set_target(gain);
            }
        }
        let frames = frames.min(MAX_BLOCK);
        let Some(sum) = self.sum.get_mut(..frames * 2) else { return };
        sum.fill(0.0);
        let mut any = false;
        let mix = self.settings;
        for (i, voice) in self.voices.iter_mut().enumerate() {
            voice.receive();
            let Some(buffer) = self.buffer.get_mut(..frames * 2) else { return };
            let sounding = voice.playing();
            buffer.fill(0.0);
            voice.finish(buffer, frames);
            let Some(shaper) = self.shapers.get_mut(i) else { continue };
            if sounding {
                shaper.process(buffer);
                // The deck's effects: after its EQ, before its fader.
                self.fx.process(i, buffer);
            }
            let trim = db_to_gain(shaper.tone.trim);
            let weight = if i == 0 { 1.0 - mix.crossfade } else { mix.crossfade };
            let fader = mix.levels[i] * trim;
            let main_gain = HEADROOM * fader * weight;
            let cue_gain =
                if separate && mix.cue[i] { HEADROOM * fader * mix.phones * (1.0 - mix.phones_mix) } else { 0.0 };
            if !sounding {
                // Changes made while paused must already be settled when resumed.
                self.main[i].reset(main_gain);
                self.cue[i].reset(cue_gain);
                continue;
            }
            any = true;
            self.main[i].set_target(main_gain);
            self.cue[i].set_target(cue_gain);
            let (mut peak_l, mut peak_r) = (0.0f32, 0.0f32);
            for ((dst, head), src) in sum
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(cue.as_chunks_mut::<2>().0.iter_mut())
                .zip(buffer.as_chunks::<2>().0)
            {
                let m = self.main[i].next_value();
                let c = self.cue[i].next_value();
                let clean = |s: f32| if s.is_finite() { s.clamp(-f32::MAX / 16.0, f32::MAX / 16.0) } else { 0.0 };
                let (l, r) = (clean(src[0]), clean(src[1]));
                peak_l = peak_l.max(l.abs());
                peak_r = peak_r.max(r.abs());
                dst[0] += l * m;
                dst[1] += r * m;
                // Never expose a hidden deck on main when the headphone pair disappears.
                if separate {
                    head[0] += l * c;
                    head[1] += r * c;
                }
            }
            // The channel meter: after the EQ, trim and fader, before the crossfader.
            voice.monitor.meter.add(peak_l * fader, peak_r * fader);
        }
        // The sampler's slots, straight into the mix, before the master's effects and level.
        for (voice, level) in self.samplers.iter_mut().zip(&mut self.sampler_levels) {
            voice.receive();
            if !voice.playing() {
                voice.finish(&mut [], 0);
                continue;
            }
            let Some(buffer) = self.buffer.get_mut(..frames * 2) else { return };
            buffer.fill(0.0);
            voice.finish(buffer, frames);
            any = true;
            let (mut peak_l, mut peak_r) = (0.0f32, 0.0f32);
            for (dst, src) in sum.as_chunks_mut::<2>().0.iter_mut().zip(buffer.as_chunks::<2>().0) {
                let g = HEADROOM * level.next_value();
                let (l, r) = (src[0] * g, src[1] * g);
                peak_l = peak_l.max(l.abs());
                peak_r = peak_r.max(r.abs());
                dst[0] += l;
                dst[1] += r;
            }
            voice.monitor.meter.add(peak_l, peak_r);
        }
        let phones_master = if separate { mix.phones * mix.phones_mix } else { 0.0 };
        if !any {
            self.master_gain.reset(mix.master);
            self.phones_master.reset(phones_master);
            if self.tap.on() {
                self.tap.silence(frames);
            }
            return;
        }
        self.master_gain.set_target(mix.master);
        self.phones_master.set_target(phones_master);
        // The master's effects, before the master level.
        if self.fx.active(2) {
            self.fx.process(2, sum);
        }
        let (mut peak_l, mut peak_r) = (0.0f32, 0.0f32);
        for ((dst, head), src) in out
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(cue.as_chunks_mut::<2>().0.iter_mut())
            .zip(sum.as_chunks_mut::<2>().0.iter_mut())
        {
            let g = self.master_gain.next_value();
            let p = self.phones_master.next_value();
            let (l, r) = (src[0] * g, src[1] * g);
            // What the master sends, kept for the recording.
            *src = [l, r];
            peak_l = peak_l.max(l.abs());
            peak_r = peak_r.max(r.abs());
            dst[0] += l;
            dst[1] += r;
            if separate {
                head[0] += l * p;
                head[1] += r * p;
            }
        }
        self.master.add(peak_l, peak_r);
        if self.tap.on() {
            self.tap.write(sum);
        }
    }
}
fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
impl crate::Controller {
    fn deck(&mut self, deck: usize) -> Result<&mut DeckVoiceSender, String> {
        self.decks.voices.get_mut(deck).ok_or_else(|| "unknown DJ deck".to_string())
    }
    pub fn deck_play(&mut self, deck: usize, data: Arc<AudioData>, from: f64) -> Result<(), String> {
        if !(1..=2).contains(&data.channels)
            || !(1000..=96000).contains(&data.sample_rate)
            || data.samples.is_empty()
            || data.samples.len() > MAX_SAMPLES
            || !from.is_finite()
            || from < 0.0
            || from >= data.frames() as f64 / f64::from(data.sample_rate.max(1))
        {
            return Err("invalid or oversized deck audio/position".into());
        }
        self.deck(deck)?.send(DeckCommand::Play { data, from })
    }
    pub fn deck_pause(&mut self, deck: usize) -> Result<(), String> {
        self.deck(deck)?.send(DeckCommand::Pause)
    }
    pub fn deck_stop(&mut self, deck: usize) -> Result<(), String> {
        self.deck(deck)?.send(DeckCommand::Stop)
    }
    pub fn deck_seek(&mut self, deck: usize, from: f64) -> Result<(), String> {
        if !from.is_finite() || from < 0.0 {
            return Err("invalid DJ seek position".into());
        }
        self.deck(deck)?.send(DeckCommand::Seek(from))
    }
    /// Moves a deck by `by` seconds of its track from wherever it is when this lands, so a beat
    /// alignment measured a moment ago still holds.
    pub fn deck_shift(&mut self, deck: usize, by: f64) -> Result<(), String> {
        if !by.is_finite() {
            return Err("invalid DJ shift".into());
        }
        self.deck(deck)?.send(DeckCommand::Shift(by))
    }
    /// How fast a deck plays, 1 at the track's own speed (0.5 … 2; the pitch follows).
    pub fn deck_rate(&mut self, deck: usize, rate: f64) -> Result<(), String> {
        if !rate.is_finite() || !(MIN_RATE..=MAX_RATE).contains(&rate) {
            return Err("a DJ deck's speed must be 0.5–2".into());
        }
        self.deck(deck)?.send(DeckCommand::Rate(rate))
    }
    /// Nudges a deck faster (above 0) or slower for as long as it is held, on top of its rate:
    /// −0.25 … 0.25, 0 to let go.
    pub fn deck_bend(&mut self, deck: usize, bend: f64) -> Result<(), String> {
        if !bend.is_finite() || !(-MAX_BEND..=MAX_BEND).contains(&bend) {
            return Err("a DJ deck's nudge must be −0.25 to 0.25".into());
        }
        self.deck(deck)?.send(DeckCommand::Bend(bend))
    }
    /// Keeps a deck's key where it is when its speed changes (a time-stretcher plays it).
    pub fn deck_key_lock(&mut self, deck: usize, on: bool) -> Result<(), String> {
        self.deck(deck)?.send(DeckCommand::KeyLock(on))
    }
    /// Moves a deck's key up (above 0) or down, in semitones, −12 … 12.
    pub fn deck_key(&mut self, deck: usize, semitones: f64) -> Result<(), String> {
        if !semitones.is_finite() || !(-MAX_KEY..=MAX_KEY).contains(&semitones) {
            return Err("a DJ deck's key moves at most 12 semitones up or down".into());
        }
        self.deck(deck)?.send(DeckCommand::Key(semitones))
    }
    /// Loops a deck between two points of its track, in seconds; `None` plays straight on.
    pub fn deck_loop(&mut self, deck: usize, region: Option<(f64, f64)>) -> Result<(), String> {
        if region.is_some_and(|(a, b)| !a.is_finite() || !b.is_finite() || a < 0.0 || b <= a) {
            return Err("a DJ loop must start at or after 0 and end after it starts".into());
        }
        self.deck(deck)?.send(DeckCommand::Loop(region))
    }
    /// A deck's EQ (dB, [`KILL_DB`] … [`BOOST_DB`]), filter (−1 … 1) and trim (± [`TRIM_DB`]).
    pub fn deck_tone(&mut self, deck: usize, tone: Tone) -> Result<(), String> {
        if deck > 1 {
            return Err("unknown DJ deck".into());
        }
        if tone.eq.iter().any(|g| !g.is_finite() || !(KILL_DB..=BOOST_DB).contains(g))
            || !tone.filter.is_finite()
            || !(-1.0..=1.0).contains(&tone.filter)
            || !tone.trim.is_finite()
            || !(-TRIM_DB..=TRIM_DB).contains(&tone.trim)
        {
            return Err("a DJ deck's EQ must be −40 to +6 dB, its filter −1 to 1 and its trim −12 to +12 dB".into());
        }
        self.decks
            .tone
            .push((deck, tone))
            .map_err(|_| "the audio engine has not taken the previous DJ EQ changes".into())
    }
    pub fn deck_monitor(&self, deck: usize) -> Option<Arc<DeckMonitor>> {
        self.decks.monitors.get(deck).cloned()
    }
    /// The decks' master meter.
    pub fn deck_master_meter(&self) -> Arc<DeckMeter> {
        Arc::clone(&self.decks.master)
    }
    /// Plays sampler slot `slot` from the start of `data`.
    pub fn sampler_play(&mut self, slot: usize, data: Arc<AudioData>) -> Result<(), String> {
        if !(1..=2).contains(&data.channels)
            || !(1000..=96000).contains(&data.sample_rate)
            || data.samples.is_empty()
            || data.samples.len() > MAX_SAMPLES
        {
            return Err("invalid or oversized sampler audio".into());
        }
        let voice = self.decks.samplers.get_mut(slot).ok_or("unknown sampler slot")?;
        voice.send(DeckCommand::Stop)?;
        voice.send(DeckCommand::Play { data, from: 0.0 })
    }
    /// Stops sampler slot `slot`.
    pub fn sampler_stop(&mut self, slot: usize) -> Result<(), String> {
        self.decks.samplers.get_mut(slot).ok_or("unknown sampler slot")?.send(DeckCommand::Stop)
    }
    /// Sampler slot `slot`'s level, 0 … 1.
    pub fn sampler_gain(&mut self, slot: usize, gain: f32) -> Result<(), String> {
        if slot >= SAMPLERS || !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
            return Err("a sampler slot's level must be 0–1".into());
        }
        self.decks
            .sampler_gains
            .push((slot, gain))
            .map_err(|_| "the audio engine has not taken the previous sampler levels".into())
    }
    pub fn sampler_monitor(&self, slot: usize) -> Option<Arc<DeckMonitor>> {
        self.decks.sampler_monitors.get(slot).cloned()
    }
    /// Sets effect unit `unit` (0 or 1): its effects, their knobs, its mix, length and tempo,
    /// and where it goes.
    pub fn deck_fx(&mut self, unit: usize, settings: FxUnit) -> Result<(), String> {
        self.decks.fx.set(unit, settings)
    }
    /// The master's recording, for a writer to hold while it records; `None` while one does.
    pub fn take_deck_recording(&mut self) -> Option<DeckRecording> {
        self.decks.recording.take()
    }
    /// Gives the master's recording back when its writer is done.
    pub fn return_deck_recording(&mut self, recording: DeckRecording) {
        self.decks.recording = Some(recording);
    }
    pub fn deck_mix(&mut self, mix: Mix) -> Result<(), String> {
        if !mix.is_valid() {
            return Err("DJ mixer values must be 0–1".into());
        }
        self.decks.mix.push(mix).map_err(|_| "the audio engine has not taken the previous DJ mixer changes".into())
    }
}
