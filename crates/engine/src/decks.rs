// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Two sample-only Preview voices, mixed after the graph through the same output. Each deck
//! plays at its own tempo (the pitch follows) through its own three-band EQ and filter
//! ([`Tone`]) before the channel level and the crossfader.
use crate::chain::MAX_BLOCK;
use crate::preview::{Preview, PreviewCommand, PreviewSender};
use crate::{AudioData, PreviewMonitor};
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
}
impl Default for Tone {
    fn default() -> Self {
        Self { eq: [0.0; 3], filter: 0.0 }
    }
}
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

#[derive(Clone, Copy)]
pub(crate) struct Mix {
    pub crossfade: f32,
    pub levels: [f32; 2],
    pub cue: [bool; 2],
}
impl Default for Mix {
    fn default() -> Self {
        Self { crossfade: 0.5, levels: [1.0; 2], cue: [false; 2] }
    }
}
pub(crate) struct DeckSender {
    pub voices: [PreviewSender; 2],
    pub monitors: [Arc<PreviewMonitor>; 2],
    pub mix: Producer<Mix>,
    pub tone: Producer<(usize, Tone)>,
}
pub(crate) struct Decks {
    voices: [Preview; 2],
    mix: Option<Consumer<Mix>>,
    tone: Option<Consumer<(usize, Tone)>>,
    shapers: [Shaper; 2],
    rate: f64,
    buffer: Vec<f32>,
    main: [Smoother; 2],
    cue: [Smoother; 2],
    settings: Mix,
}
impl Decks {
    pub fn new(rate: u32) -> Self {
        let ramp = Smoother::ramp_frames(0.01, f64::from(rate));
        Self {
            voices: std::array::from_fn(|_| Preview::new(rate)),
            mix: None,
            tone: None,
            shapers: [Shaper::new(), Shaper::new()],
            rate: f64::from(rate.max(1)),
            buffer: vec![0.0; MAX_BLOCK * 2],
            main: [Smoother::new(0.0, ramp); 2],
            cue: [Smoother::new(0.0, ramp); 2],
            settings: Mix::default(),
        }
    }
    pub fn sender(&mut self) -> DeckSender {
        let (tx, rx) = RingBuffer::new(16);
        self.mix = Some(rx);
        let (tone, tones) = RingBuffer::new(32);
        self.tone = Some(tones);
        DeckSender {
            voices: [self.voices[0].sender(), self.voices[1].sender()],
            monitors: std::array::from_fn(|i| Arc::clone(&self.voices[i].monitor)),
            mix: tx,
            tone,
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
        for (i, voice) in self.voices.iter_mut().enumerate() {
            voice.receive(&mut []);
            let Some(buffer) = self.buffer.get_mut(..frames.min(MAX_BLOCK) * 2) else { return };
            let sounding = voice.playing();
            buffer.fill(0.0);
            voice.finish(buffer, frames, &mut []);
            if sounding && let Some(shaper) = self.shapers.get_mut(i) {
                shaper.process(buffer);
            }
            let weight = if i == 0 { 1.0 - self.settings.crossfade } else { self.settings.crossfade };
            let main_gain = 0.501_187_2 * self.settings.levels[i] * weight;
            let cue_gain = if separate && self.settings.cue[i] { 0.501_187_2 * self.settings.levels[i] } else { 0.0 };
            if !sounding {
                // Changes made while paused must already be settled when resumed.
                self.main[i].reset(main_gain);
                self.cue[i].reset(cue_gain);
                continue;
            }
            self.main[i].set_target(main_gain);
            self.cue[i].set_target(cue_gain);
            for ((dst, head), src) in out
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(cue.as_chunks_mut::<2>().0.iter_mut())
                .zip(buffer.as_chunks::<2>().0)
            {
                let m = self.main[i].next_value();
                let c = self.cue[i].next_value();
                for channel in 0..2 {
                    let s = if src[channel].is_finite() {
                        src[channel].clamp(-f32::MAX / 16.0, f32::MAX / 16.0)
                    } else {
                        0.0
                    };
                    dst[channel] += s * m;
                    // Never expose a hidden deck on main when the headphone pair disappears.
                    if separate {
                        head[channel] += s * c;
                    }
                }
            }
        }
    }
}
impl crate::Controller {
    pub fn deck_play(&mut self, deck: usize, data: Arc<AudioData>, from: f64) -> Result<(), String> {
        if !(1..=2).contains(&data.channels)
            || !(1000..=96000).contains(&data.sample_rate)
            || data.samples.is_empty()
            || data.samples.len() > 128 * 1024 * 1024 / 4
            || !from.is_finite()
            || from < 0.0
            || from >= data.frames() as f64 / f64::from(data.sample_rate.max(1))
        {
            return Err("invalid or oversized deck audio/position".into());
        }
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Sample {
            data,
            from,
            gain: 1.0,
            resume: true,
        })?;
        Ok(())
    }
    pub fn deck_pause(&mut self, deck: usize) -> Result<(), String> {
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Pause(true))?;
        Ok(())
    }
    pub fn deck_stop(&mut self, deck: usize) -> Result<(), String> {
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Stop)?;
        Ok(())
    }
    pub fn deck_seek(&mut self, deck: usize, from: f64) -> Result<(), String> {
        if !from.is_finite() || from < 0.0 {
            return Err("invalid DJ seek position".into());
        }
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Seek(from))?;
        Ok(())
    }
    /// How fast a deck plays, 1 at the track's own speed (0.5 … 2; the pitch follows).
    pub fn deck_rate(&mut self, deck: usize, rate: f64) -> Result<(), String> {
        if !rate.is_finite() || !(crate::preview::MIN_RATE..=crate::preview::MAX_RATE).contains(&rate) {
            return Err("a DJ deck's speed must be 0.5–2".into());
        }
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Rate(rate))?;
        Ok(())
    }
    /// A deck's EQ (dB, [`KILL_DB`] … [`BOOST_DB`]) and filter (−1 … 1).
    pub fn deck_tone(&mut self, deck: usize, tone: Tone) -> Result<(), String> {
        if deck > 1 {
            return Err("unknown DJ deck".into());
        }
        if tone.eq.iter().any(|g| !g.is_finite() || !(KILL_DB..=BOOST_DB).contains(g))
            || !tone.filter.is_finite()
            || !(-1.0..=1.0).contains(&tone.filter)
        {
            return Err("a DJ deck's EQ must be −40 to +6 dB and its filter −1 to 1".into());
        }
        self.decks
            .tone
            .push((deck, tone))
            .map_err(|_| "the audio engine has not taken the previous DJ EQ changes".into())
    }
    pub fn deck_monitor(&self, deck: usize) -> Option<Arc<PreviewMonitor>> {
        self.decks.monitors.get(deck).cloned()
    }
    pub fn deck_mix(&mut self, crossfade: f32, levels: [f32; 2], cue: [bool; 2]) -> Result<(), String> {
        if !crossfade.is_finite()
            || !(0.0..=1.0).contains(&crossfade)
            || levels.iter().any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
        {
            return Err("DJ mixer values must be 0–1".into());
        }
        self.decks
            .mix
            .push(Mix { crossfade, levels, cue })
            .map_err(|_| "the audio engine has not taken the previous DJ mixer changes".into())
    }
}
