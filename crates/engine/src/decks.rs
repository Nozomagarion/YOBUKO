// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Two sample-only Preview voices, mixed after the graph through the same output.
use crate::chain::MAX_BLOCK;
use crate::preview::{Preview, PreviewCommand, PreviewSender};
use crate::{AudioData, PreviewMonitor};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::Arc;
use suhina_dsp::Smoother;

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
}
pub(crate) struct Decks {
    voices: [Preview; 2],
    mix: Option<Consumer<Mix>>,
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
            buffer: vec![0.0; MAX_BLOCK * 2],
            main: [Smoother::new(0.0, ramp); 2],
            cue: [Smoother::new(0.0, ramp); 2],
            settings: Mix::default(),
        }
    }
    pub fn sender(&mut self) -> DeckSender {
        let (tx, rx) = RingBuffer::new(16);
        self.mix = Some(rx);
        DeckSender {
            voices: [self.voices[0].sender(), self.voices[1].sender()],
            monitors: std::array::from_fn(|i| Arc::clone(&self.voices[i].monitor)),
            mix: tx,
        }
    }
    pub fn finish(&mut self, out: &mut [f32], cue: &mut [f32], frames: usize, separate: bool) {
        while let Some(mix) = self.mix.as_mut().and_then(|r| r.pop().ok()) {
            self.settings = mix;
        }
        for (i, voice) in self.voices.iter_mut().enumerate() {
            voice.receive(&mut []);
            let Some(buffer) = self.buffer.get_mut(..frames.min(MAX_BLOCK) * 2) else { return };
            let sounding = voice.playing();
            buffer.fill(0.0);
            voice.finish(buffer, frames, &mut []);
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
        })
    }
    pub fn deck_pause(&mut self, deck: usize) -> Result<(), String> {
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Pause(true))
    }
    pub fn deck_stop(&mut self, deck: usize) -> Result<(), String> {
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Stop)
    }
    pub fn deck_seek(&mut self, deck: usize, from: f64) -> Result<(), String> {
        if !from.is_finite() || from < 0.0 {
            return Err("invalid DJ seek position".into());
        }
        self.decks.voices.get_mut(deck).ok_or("unknown DJ deck")?.send(PreviewCommand::Seek(from))
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
