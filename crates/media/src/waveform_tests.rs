// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::*;

const RATE: u32 = 8000;

/// Four seconds of stereo: a decaying 110 Hz sine on the left, a square wave at a quarter of full
/// scale on the right for the first half and silence after.
fn stereo() -> Arc<AudioData> {
    let mut samples = Vec::new();
    for frame in 0..RATE * 4 {
        let t = f64::from(frame) / f64::from(RATE);
        let left = ((t * 110.0 * std::f64::consts::TAU).sin() * (-t * 0.8).exp()) as f32;
        let right = if t < 2.0 { if (frame / 40) % 2 == 0 { 0.25 } else { -0.25 } } else { 0.0 };
        samples.push(left);
        samples.push(right);
    }
    Arc::new(AudioData { sample_rate: RATE, channels: 2, samples })
}

#[test]
fn levels_summarise_each_channel() {
    let data = stereo();
    let p = peaks(&data);
    assert_eq!((p.frames, p.lanes, p.sample_rate), (32_000, 2, RATE));
    assert_eq!(p.levels.len(), LEVELS.len());
    for level in &p.levels {
        assert_eq!(level.buckets.len(), 32_000_usize.div_ceil(level.frames) * 2, "{}", level.frames);
    }
    // A stretch of only the positive half: its lowest sample is not 0.
    let up = p.column(Lane::Channel(1), 0.0, 10.0).unwrap();
    assert_eq!((up.min, up.max), (0.25, 0.25));
    // The square wave on the right, read at every resolution.
    for width in [100.0, 300.0, 3000.0, 16_000.0] {
        let b = p.column(Lane::Channel(1), 0.0, width).unwrap();
        assert_eq!((b.min, b.max), (-0.25, 0.25), "{width}");
        // Coarse levels include the silence after the square wave in their last bucket.
        assert!((b.rms - 0.25).abs() < 5e-3, "{width}: {}", b.rms);
    }
    // Silence after two seconds; the left is loud at the start.
    let quiet = p.column(Lane::Channel(1), 20_000.0, 30_000.0).unwrap();
    assert_eq!((quiet.min, quiet.max, quiet.rms), (0.0, 0.0, 0.0));
    let loud = p.column(Lane::Channel(0), 0.0, 400.0).unwrap();
    assert!(loud.max > 0.95 && loud.min < -0.95 && loud.rms > 0.6);
    // Mixed: both lanes together.
    let both = p.column(Lane::Mixed, 0.0, 400.0).unwrap();
    assert_eq!((both.min, both.max), (loud.min, loud.max));
    // Outside the file, or nonsense.
    assert_eq!(p.column(Lane::Mixed, 40_000.0, 50_000.0), None);
    assert_eq!(p.column(Lane::Mixed, -100.0, -1.0), None);
    assert_eq!(p.column(Lane::Mixed, f64::NAN, 5.0), None);
    assert_eq!(p.column(Lane::Channel(2), 0.0, 5.0), None);
    // A stretch running off either end reads what is inside.
    assert!(p.column(Lane::Channel(1), -500.0, 100.0).is_some_and(|b| b.max == 0.25));
    // The old reading of the range in seconds.
    assert_eq!(p.range(2.5, 3.0).0, p.column(Lane::Mixed, 20_000.0, 24_000.0).unwrap().min);
    assert_eq!(p.range(10.0, 11.0), (0.0, 0.0));
}

#[test]
fn samples_and_odd_files() {
    let p = peaks(&stereo());
    assert_eq!(p.sample(Lane::Channel(1), 0), Some(0.25));
    assert_eq!(p.sample(Lane::Mixed, 0), Some(0.125));
    assert_eq!(p.sample(Lane::Mixed, 40_000), None);
    // Without the samples (summarised only), close-up reads use the finest level.
    let summary = summarise(&stereo());
    assert_eq!(summary.sample(Lane::Mixed, 0), None);
    assert!(summary.column(Lane::Channel(1), 0.0, 10.0).is_some_and(|b| b.max == 0.25));
    // Empty, mono, many channels, not-a-number samples: no panic, sensible lanes.
    let empty = peaks(&Arc::new(AudioData { sample_rate: 44_100, channels: 2, samples: Vec::new() }));
    assert_eq!(empty.column(Lane::Mixed, 0.0, 10.0), None);
    let mono = peaks(&Arc::new(AudioData { sample_rate: 44_100, channels: 1, samples: vec![0.5, -0.5, f32::NAN] }));
    assert_eq!(mono.lanes, 1);
    assert_eq!(mono.column(Lane::Mixed, 0.0, 3.0).map(|b| (b.min, b.max)), Some((-0.5, 0.5)));
    let six = peaks(&Arc::new(AudioData { sample_rate: 48_000, channels: 6, samples: vec![0.1; 600] }));
    assert_eq!((six.lanes, six.frames), (2, 100));
    let none = peaks(&Arc::new(AudioData { sample_rate: 0, channels: 0, samples: vec![1.0; 10] }));
    assert_eq!(none.lanes, 1);
}
