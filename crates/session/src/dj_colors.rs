// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of a DJ deck's waveform: how loud the lows, mids and highs of a track are, a
//! hundred times a second, worked out by the DJ worker when a deck loads, so the page can draw
//! kicks red, voices and synths green and hats blue.

use suhina_engine::AudioData;

/// Columns of band levels per second of the track.
const PER_SECOND: f64 = 100.0;
/// Where the lows end and the highs start, in hertz.
const LOW_HZ: f64 = 250.0;
const HIGH_HZ: f64 = 2_500.0;
/// The most columns kept, so a hostile length cannot exhaust memory (about 33 minutes).
const MAX_COLUMNS: usize = 200_000;

/// A track's low, mid and high levels over time, each 0 … 1 of the loudest of its band.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaveColors {
    pub per_second: f64,
    pub bands: Vec<[f32; 3]>,
}

impl WaveColors {
    /// The bands of the track between `from` and `to` seconds: their loudest columns.
    pub fn column(&self, from: f64, to: f64) -> Option<[f32; 3]> {
        if !(from.is_finite() && to.is_finite()) || to < 0.0 {
            return None;
        }
        let first = (from.max(0.0) * self.per_second) as usize;
        let last = ((to * self.per_second).ceil() as usize).max(first + 1).min(self.bands.len());
        let columns = self.bands.get(first..last)?;
        if columns.is_empty() {
            return None;
        }
        Some(columns.iter().fold([0.0; 3], |m, c| [m[0].max(c[0]), m[1].max(c[1]), m[2].max(c[2])]))
    }
}

/// One-pole filter coefficient for `hz` at `rate`.
fn pole(hz: f64, rate: f64) -> f32 {
    (1.0 - (-std::f64::consts::TAU * hz / rate).exp()) as f32
}

/// Splits the track into lows, mids and highs and measures each, column by column.
pub(crate) fn analyse(data: &AudioData) -> WaveColors {
    let rate = f64::from(data.sample_rate.max(1));
    let channels = usize::from(data.channels.max(1));
    let frames = data.frames();
    let per_column = ((rate / PER_SECOND) as usize).max(1);
    let (low_k, high_k) = (pole(LOW_HZ, rate), pole(HIGH_HZ, rate));
    let (mut low, mut below_high) = (0.0f32, 0.0f32);
    let mut bands = Vec::with_capacity((frames / per_column + 1).min(MAX_COLUMNS));
    let mut sums = [0.0f32; 3];
    let mut count = 0;
    for frame in data.samples.chunks(channels) {
        let x = frame.iter().copied().filter(|s| s.is_finite()).sum::<f32>() / channels as f32;
        low += low_k * (x - low);
        below_high += high_k * (x - below_high);
        let parts = [low, below_high - low, x - below_high];
        for (sum, part) in sums.iter_mut().zip(parts) {
            *sum += part * part;
        }
        count += 1;
        if count == per_column {
            if bands.len() == MAX_COLUMNS {
                break;
            }
            bands.push(sums.map(|s| (s / count as f32).sqrt()));
            sums = [0.0; 3];
            count = 0;
        }
    }
    // Each band against its own loudest, so a quiet hi-hat still shows.
    let mut top = [0.0f32; 3];
    for c in &bands {
        for (t, v) in top.iter_mut().zip(c) {
            *t = t.max(*v);
        }
    }
    for c in &mut bands {
        for (v, t) in c.iter_mut().zip(top) {
            *v = if t > 1e-9 { (*v / t).clamp(0.0, 1.0) } else { 0.0 };
        }
    }
    WaveColors { per_second: rate / per_column as f64, bands }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn tone(hz: f32, seconds: f32) -> Vec<f32> {
        (0..(8000.0 * seconds) as usize).map(|i| (std::f32::consts::TAU * hz * i as f32 / 8000.0).sin()).collect()
    }

    #[test]
    fn a_kick_shows_in_the_lows_and_a_hat_in_the_highs() {
        let mut samples = tone(60.0, 1.0);
        samples.extend(tone(3_500.0, 1.0));
        let colors = analyse(&AudioData { sample_rate: 8000, channels: 1, samples });
        let first = colors.column(0.2, 0.8).unwrap();
        let second = colors.column(1.2, 1.8).unwrap();
        assert!(first[0] > 0.8 && first[2] < 0.2, "{first:?}");
        assert!(second[2] > 0.8 && second[0] < 0.2, "{second:?}");
        assert!(colors.column(5.0, 6.0).is_none());
        assert!(colors.column(f64::NAN, 1.0).is_none());
    }
}
