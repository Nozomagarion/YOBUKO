// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Immutable host-prepared PCM and checked regions; no file I/O or instrument state.
use crate::DeviceSdkError;
use std::sync::Arc;

type Pcm = Arc<dyn AsRef<[f32]> + Send + Sync>;

/// Construct, replace and drop off the audio thread. Reads do not allocate.
pub struct SampleAsset {
    pcm: Pcm,
    sample_rate: u32,
    channels: usize,
    frames: usize,
}

/// Checked half-open interval, with positions relative to its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleRegion {
    start: usize,
    end: usize,
}

impl SampleAsset {
    pub fn new(pcm: Pcm, sample_rate: u32, channels: u16) -> Result<Self, DeviceSdkError> {
        let channels = usize::from(channels);
        if !(8000..=384000).contains(&sample_rate) || !(1..=32).contains(&channels) {
            return Err(DeviceSdkError("invalid sample rate or channel count".into()));
        }
        let samples = pcm.as_ref().as_ref().len();
        if !samples.is_multiple_of(channels) {
            return Err(DeviceSdkError("sample contains an incomplete frame".into()));
        }
        Ok(Self { pcm, sample_rate, channels, frames: samples / channels })
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn frames(&self) -> usize {
        self.frames
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn duration_seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate)
    }
    pub fn region(&self, start: usize, end: usize) -> Result<SampleRegion, DeviceSdkError> {
        if start > end || end > self.frames {
            return Err(DeviceSdkError("sample region is outside prepared PCM".into()));
        }
        Ok(SampleRegion { start, end })
    }
    /// Linear read in region-relative frames. Regions from another asset are rechecked.
    pub fn read_stereo(&self, region: SampleRegion, position: f64) -> [f32; 2] {
        let length = region.end.saturating_sub(region.start);
        if region.end > self.frames
            || length == 0
            || !position.is_finite()
            || position < 0.
            || position >= length as f64
        {
            return [0.; 2];
        }
        let first = position.floor() as usize;
        let second = first.saturating_add(1).min(length - 1);
        let fraction = position - first as f64;
        let read = |frame: usize, channel: usize| {
            let index = region
                .start
                .checked_add(frame)
                .and_then(|v| v.checked_mul(self.channels))
                .and_then(|v| v.checked_add(channel));
            let value = index.and_then(|v| self.pcm.as_ref().as_ref().get(v)).copied().unwrap_or(0.);
            if value.is_finite() { f64::from(value) } else { 0. }
        };
        std::array::from_fn(|channel| {
            let channel = channel.min(self.channels - 1);
            let a = read(first, channel);
            let b = read(second, channel);
            (a + (b - a) * fraction) as f32
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pcm(values: Vec<f32>) -> Pcm {
        Arc::new(values)
    }
    #[test]
    fn validates_resources_and_regions() {
        assert!(SampleAsset::new(pcm(vec![]), 0, 1).is_err());
        assert!(SampleAsset::new(pcm(vec![]), 48000, 0).is_err());
        assert!(SampleAsset::new(pcm(vec![1.]), 48000, 2).is_err());
        let sample = SampleAsset::new(pcm(vec![1., 3.]), 48000, 1).unwrap();
        assert!(sample.region(2, 1).is_err());
        assert!(sample.region(0, usize::MAX).is_err());
        assert_eq!(sample.read_stereo(sample.region(0, 2).unwrap(), 0.5), [2.; 2]);
        assert_eq!(sample.read_stereo(sample.region(1, 2).unwrap(), 0.5), [3.; 2]);
        assert_eq!(sample.read_stereo(sample.region(0, 0).unwrap(), 0.), [0.; 2]);
        for position in [-1., f64::NAN, f64::INFINITY, 2.] {
            assert_eq!(sample.read_stereo(sample.region(0, 2).unwrap(), position), [0.; 2]);
        }
    }
    #[test]
    fn stereo_and_foreign_regions_are_checked() {
        let sample = SampleAsset::new(pcm(vec![1., -1., 3., -3.]), 48000, 2).unwrap();
        let region = sample.region(0, 2).unwrap();
        assert_eq!(sample.read_stereo(region, 0.5), [2., -2.]);
        let small = SampleAsset::new(pcm(vec![f32::NAN]), 48000, 1).unwrap();
        assert_eq!(small.read_stereo(region, 0.), [0.; 2]);
        assert_eq!(small.read_stereo(small.region(0, 1).unwrap(), 0.), [0.; 2]);
    }
}
