// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of a DJ deck's waveform: how loud the lows, mids and highs of a track are, a
//! hundred times a second, split by two 24 dB/octave crossovers and worked out by the DJ worker
//! when a deck loads, so the page can draw kicks red, voices and synths green and hats blue.

use suhina_dsp::crossover::LinkwitzRiley;
use suhina_engine::AudioData;

/// Columns of band levels per second of the track.
const PER_SECOND: f64 = 100.0;
/// Where the lows end and the highs start, in hertz.
const LOW_HZ: f64 = 250.0;
const HIGH_HZ: f64 = 2_500.0;
/// How much each band is raised before they share one scale.
const WEIGHTS: [f32; 3] = [1.0, 2.0, 4.0];
/// The most columns kept, so a hostile length cannot exhaust memory (about 33 minutes).
const MAX_COLUMNS: usize = 200_000;

/// A track's low, mid and high levels over time, 0 … 1 of the loudest band anywhere in it.
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

/// Splits the track into lows, mids and highs and measures each, column by column.
pub(crate) fn analyse(data: &AudioData) -> WaveColors {
    let rate = f64::from(data.sample_rate.max(1));
    let channels = usize::from(data.channels.max(1));
    let frames = data.frames();
    let per_column = ((rate / PER_SECOND) as usize).max(1);
    // Two 24 dB/octave crossovers: the lows off first, then the rest split into mids and highs.
    let (mut lows, mut highs) = (LinkwitzRiley::new(rate, LOW_HZ), LinkwitzRiley::new(rate, HIGH_HZ));
    let mut bands = Vec::with_capacity((frames / per_column + 1).min(MAX_COLUMNS));
    let mut sums = [0.0f32; 3];
    let mut count = 0;
    for frame in data.samples.chunks(channels) {
        let x = frame.iter().copied().filter(|s| s.is_finite()).sum::<f32>() / channels as f32;
        let (low, rest) = lows.split(f64::from(x));
        let (mid, high) = highs.split(rest);
        let parts = [low as f32, mid as f32, high as f32];
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
    // All bands on one scale, so a kick reads red and a hat blue; the mids and highs, which
    // carry less energy in music, are raised so a quiet hi-hat still shows.
    let mut top = 0.0f32;
    for c in &mut bands {
        for (v, w) in c.iter_mut().zip(WEIGHTS) {
            *v *= w;
            top = top.max(*v);
        }
    }
    for c in &mut bands {
        for v in c.iter_mut() {
            *v = if top > 1e-9 { (*v / top).clamp(0.0, 1.0) } else { 0.0 };
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
        // The lows lead where the kick is, the highs where the hat is: red, then blue.
        assert!(first[0] > 0.1 && first[0] > 10.0 * first[1].max(first[2]), "{first:?}");
        assert!(second[2] > 0.9 && second[2] > 10.0 * second[0].max(second[1]), "{second:?}");
        assert!(colors.column(5.0, 6.0).is_none());
        assert!(colors.column(f64::NAN, 1.0).is_none());
    }
}
