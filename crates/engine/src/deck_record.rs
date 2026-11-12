// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ master, recorded: while a recording is on, the audio thread copies what the decks,
//! the sampler and the master effects send to the master into a ring that a writer on another
//! thread empties into a file. The ring is made with the decks, so the audio thread never
//! allocates; what does not fit is counted, not waited for.

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Seconds of stereo the ring holds before the writer must have emptied it.
const RING_SECONDS: usize = 4;

/// The audio thread's end: copies the master while the recording is on.
pub(crate) struct DeckTap {
    on: Arc<AtomicBool>,
    producer: Producer<f32>,
    dropped: Arc<AtomicU64>,
}

/// The other end, for the writer: whether the recording is on, and the master's samples.
pub struct DeckRecording {
    on: Arc<AtomicBool>,
    consumer: Consumer<f32>,
    dropped: Arc<AtomicU64>,
    rate: u32,
}

/// A tap and its recording, joined by a ring of [`RING_SECONDS`] at `rate`.
pub(crate) fn tap(rate: u32) -> (DeckTap, DeckRecording) {
    let (producer, consumer) = RingBuffer::new(rate.max(1) as usize * 2 * RING_SECONDS);
    let on = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicU64::new(0));
    (
        DeckTap { on: Arc::clone(&on), producer, dropped: Arc::clone(&dropped) },
        DeckRecording { on, consumer, dropped, rate },
    )
}

impl DeckTap {
    pub(crate) fn on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// Copies one block of interleaved stereo; frames that do not fit are counted.
    pub(crate) fn write(&mut self, stereo: &[f32]) {
        let mut lost = 0u64;
        for frame in stereo.as_chunks::<2>().0 {
            if self.producer.slots() < 2 {
                lost += 1;
                continue;
            }
            for s in frame {
                let _ = self.producer.push(if s.is_finite() { *s } else { 0.0 });
            }
        }
        if lost > 0 {
            self.dropped.fetch_add(lost, Ordering::Relaxed);
        }
    }

    /// Copies `frames` of silence: the decks are quiet but the recording runs on.
    pub(crate) fn silence(&mut self, frames: usize) {
        let mut lost = 0u64;
        for _ in 0..frames {
            if self.producer.slots() < 2 {
                lost += 1;
                continue;
            }
            let _ = self.producer.push(0.0);
            let _ = self.producer.push(0.0);
        }
        if lost > 0 {
            self.dropped.fetch_add(lost, Ordering::Relaxed);
        }
    }
}

impl DeckRecording {
    /// The sample rate the master is recorded at.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Starts copying the master, from an empty ring.
    pub fn start(&mut self) {
        self.on.store(false, Ordering::Relaxed);
        let stale = self.consumer.slots();
        if let Ok(chunk) = self.consumer.read_chunk(stale) {
            chunk.commit_all();
        }
        self.dropped.store(0, Ordering::Relaxed);
        self.on.store(true, Ordering::Relaxed);
    }

    /// Stops copying; what is already in the ring can still be drained.
    pub fn stop(&mut self) {
        self.on.store(false, Ordering::Relaxed);
    }

    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// Moves what the audio thread has copied, interleaved stereo, into `out`.
    pub fn drain(&mut self, out: &mut Vec<f32>) {
        let ready = self.consumer.slots() & !1;
        if let Ok(chunk) = self.consumer.read_chunk(ready) {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
        }
    }

    /// Frames the ring had no room for since the recording started.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}
