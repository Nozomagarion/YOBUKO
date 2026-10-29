// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared multiresolution waveform summaries; compute on a file worker, never the audio callback.

use std::{collections::HashMap, sync::Arc};
use suhina_engine::{AudioData, AudioPool};

/// Frames per bucket at each level, finest first; each level is eight of the one before.
pub const LEVELS: [usize; 3] = [256, 2048, 16384];
/// At most this many channels get lanes of their own (left and right); others are not drawn.
pub const MAX_LANES: usize = 2;

/// The lowest and highest sample of a stretch, and its RMS level.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bucket {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

/// Sums samples into a [`Bucket`].
#[derive(Clone, Copy)]
struct Sum {
    min: f32,
    max: f32,
    squares: f64,
    count: u32,
}

impl Sum {
    const EMPTY: Self = Self { min: f32::INFINITY, max: f32::NEG_INFINITY, squares: 0.0, count: 0 };

    /// Takes in a bucket, as `weight` samples (its frames).
    fn take(&mut self, bucket: Bucket, weight: u32) {
        self.min = self.min.min(bucket.min);
        self.max = self.max.max(bucket.max);
        self.squares += f64::from(bucket.rms) * f64::from(bucket.rms) * f64::from(weight);
        self.count = self.count.saturating_add(weight);
    }

    fn add(&mut self, sample: f32) {
        let sample = if sample.is_finite() { sample } else { 0.0 };
        self.min = self.min.min(sample);
        self.max = self.max.max(sample);
        self.squares += f64::from(sample) * f64::from(sample);
        self.count += 1;
    }

    fn bucket(self) -> Bucket {
        if self.count == 0 {
            return Bucket::default();
        }
        let rms = (self.squares / f64::from(self.count)).sqrt() as f32;
        Bucket { min: self.min, max: self.max, rms }
    }
}

/// One level: `frames` per bucket, buckets interleaved by lane.
#[derive(Debug, Clone, PartialEq, Default)]
struct Level {
    frames: usize,
    buckets: Vec<Bucket>,
}

/// One file's waveform.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Peaks {
    pub sample_rate: u32,
    /// Frames in the file.
    pub frames: usize,
    /// Lanes it can be drawn in: 1 for a mono file, 2 (left, right) for more channels.
    pub lanes: usize,
    levels: Vec<Level>,
    /// The samples, for drawing closer than the finest level; shared with the audio pool.
    data: Option<Arc<AudioData>>,
}

/// Waveforms by the `file` string of the project's audio regions, as the audio pool keys them.
pub type Waveforms = HashMap<String, Arc<Peaks>>;

/// The waveforms of every file in `pool`.
pub fn from_pool(pool: &AudioPool) -> Waveforms {
    pool.files.iter().map(|(file, data)| (file.clone(), Arc::new(peaks(data)))).collect()
}

/// Summarises one decoded file at every level; keeps the samples for drawing close up.
pub fn peaks(data: &Arc<AudioData>) -> Peaks {
    let mut peaks = summarise(data);
    peaks.data = Some(Arc::clone(data));
    peaks
}

fn summarise(data: &AudioData) -> Peaks {
    let channels = usize::from(data.channels.max(1));
    let lanes = channels.min(MAX_LANES);
    let frames = data.samples.len() / channels;
    let mut finest = Vec::with_capacity((frames / LEVELS[0] + 1) * lanes);
    for chunk in data.samples.chunks(LEVELS[0] * channels) {
        let mut sums = [Sum::EMPTY; MAX_LANES];
        for frame in chunk.chunks_exact(channels) {
            for (sum, sample) in sums.iter_mut().zip(frame) {
                sum.add(*sample);
            }
        }
        finest.extend(sums.iter().take(lanes).map(|s| s.bucket()));
    }
    let mut levels = vec![Level { frames: LEVELS[0], buckets: finest }];
    for pair in LEVELS.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let ratio = to / from;
        let below = levels.last().map(|l| l.buckets.as_slice()).unwrap_or_default();
        let count = below.len() / lanes;
        let mut buckets = Vec::with_capacity((count / ratio + 1) * lanes);
        for group in (0..count).step_by(ratio) {
            for lane in 0..lanes {
                let mut sum = Sum::EMPTY;
                for i in group..(group + ratio).min(count) {
                    if let Some(b) = below.get(i * lanes + lane) {
                        sum.take(*b, from as u32);
                    }
                }
                buckets.push(sum.bucket());
            }
        }
        levels.push(Level { frames: to, buckets });
    }
    Peaks { sample_rate: data.sample_rate, frames, lanes, levels, data: None }
}

/// Which lane of a file a column is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// One channel (0 left, 1 right).
    Channel(usize),
    /// Every lane together.
    Mixed,
}

impl Peaks {
    /// The bucket covering frames `from` … `to` of `lane`, from the coarsest level with at
    /// least one bucket in that stretch, or the samples when it is shorter than the finest;
    /// `None` outside the file.
    pub fn column(&self, lane: Lane, from: f64, to: f64) -> Option<Bucket> {
        if !(from.is_finite() && to.is_finite()) || self.frames == 0 || self.lanes == 0 {
            return None;
        }
        let (from, to) = (from.min(to).max(0.0), from.max(to).min(self.frames as f64));
        if to <= from && from >= self.frames as f64 || to < 0.0 {
            return None;
        }
        let width = to - from;
        let lanes: &[usize] = match lane {
            Lane::Channel(c) if c < self.lanes => &[c][..],
            Lane::Channel(_) => return None,
            Lane::Mixed if self.lanes == 2 => &[0, 1][..],
            Lane::Mixed => &[0][..],
        };
        let raw = self.data.as_deref().filter(|_| width < LEVELS[0] as f64);
        if let Some(data) = raw {
            return Some(self.read_samples(data, lanes, from, to));
        }
        let level = self.levels.iter().rev().find(|l| l.frames as f64 <= width).or(self.levels.first())?;
        let first = (from / level.frames as f64).floor() as usize;
        let last = ((to / level.frames as f64).ceil() as usize).max(first + 1);
        let mut sum = Sum::EMPTY;
        for &l in lanes {
            for i in first..last {
                if let Some(b) = level.buckets.get(i * self.lanes + l) {
                    sum.take(*b, level.frames as u32);
                }
            }
        }
        (sum.count > 0).then(|| sum.bucket())
    }

    fn read_samples(&self, data: &AudioData, lanes: &[usize], from: f64, to: f64) -> Bucket {
        let channels = usize::from(data.channels.max(1));
        let first = from.floor() as usize;
        let last = (to.ceil() as usize).max(first + 1).min(self.frames);
        let Some(frames) = data.samples.get(first * channels..last * channels) else { return Bucket::default() };
        // At most the finest level's frames: sums in f32 are exact enough.
        let (mut min, mut max, mut squares, mut count) = (f32::INFINITY, f32::NEG_INFINITY, 0.0f32, 0u32);
        for frame in frames.chunks_exact(channels) {
            for &lane in lanes {
                let s = frame.get(lane).copied().unwrap_or(0.0);
                let s = if s.is_finite() { s } else { 0.0 };
                min = min.min(s);
                max = max.max(s);
                squares += s * s;
                count += 1;
            }
        }
        if count == 0 {
            return Bucket::default();
        }
        Bucket { min, max, rms: (squares / count as f32).sqrt() }
    }

    /// One frame's sample of `lane` (the lanes averaged for [`Lane::Mixed`]); `None` without the
    /// samples or outside the file.
    pub fn sample(&self, lane: Lane, frame: usize) -> Option<f32> {
        let data = self.data.as_deref()?;
        let channels = usize::from(data.channels.max(1));
        let at = |c: usize| data.samples.get(frame * channels + c).copied().filter(|s| s.is_finite());
        match lane {
            Lane::Channel(c) if c < self.lanes => at(c),
            Lane::Channel(_) => None,
            Lane::Mixed if self.lanes == 2 => Some((at(0)? + at(1)?) * 0.5),
            Lane::Mixed => at(0),
        }
    }

    /// The range of the samples between two positions in seconds, every lane together;
    /// (0, 0) outside the file.
    pub fn range(&self, from: f64, to: f64) -> (f32, f32) {
        let rate = f64::from(self.sample_rate);
        self.column(Lane::Mixed, from * rate, to * rate).map_or((0.0, 0.0), |b| (b.min, b.max))
    }

    /// The same peaks without the samples, for keeping many files' waveforms without their
    /// audio (the browser's cache): close up, they read the finest level.
    pub fn without_samples(&self) -> Self {
        Self { data: None, ..self.clone() }
    }

    /// Whether it can be drawn sample by sample.
    pub fn has_samples(&self) -> bool {
        self.data.is_some()
    }
}

#[cfg(test)]
#[path = "waveform_tests.rs"]
mod tests;
