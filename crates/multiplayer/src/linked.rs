// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Linked playback: one host-ordered transport every participant starts at the same moment.
//!
//! No audio travels. The host keeps the shared transport on its own monotonic clock (milliseconds
//! since the session started); guests estimate that clock from their poll round trips and turn
//! the scheduled start into a local delay (docs/spec/linked-playback.md).

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;

/// How far ahead the host schedules a start, so every peer hears it before it is due.
pub const LEAD_MS: u64 = 300;
/// The last beat a shared position may name, as `transport.setPosition` allows.
pub const MAX_BEAT: f64 = 120_000.0;
/// Round trips kept for the clock estimate: about six seconds of polls.
const SAMPLES: usize = 64;
/// A round trip slower than this says nothing useful about the host clock.
const MAX_RTT_MS: u64 = 2_000;

/// The shared transport the host orders. `start_ms` is on the host clock.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SharedTransport {
    pub seq: u64,
    pub playing: bool,
    pub beat: f64,
    pub start_ms: u64,
}
impl SharedTransport {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_beat(self.beat) {
            return Err("invalid shared transport position".into());
        }
        Ok(())
    }
}

/// One participant's request to change the shared transport.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "lowercase", deny_unknown_fields)]
pub enum TransportRequest {
    Link { linked: bool },
    Play { beat: f64 },
    Stop {},
    Seek { beat: f64 },
}
impl TransportRequest {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Play { beat } | Self::Seek { beat } if !valid_beat(*beat) => {
                Err(format!("beats must be between 0 and {MAX_BEAT}"))
            }
            _ => Ok(()),
        }
    }
}

fn valid_beat(beat: f64) -> bool {
    beat.is_finite() && (0.0..=MAX_BEAT).contains(&beat)
}

/// Milliseconds on a monotonic clock that starts with the session.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Clock(Instant);
impl Clock {
    pub(crate) fn new() -> Self {
        Self(Instant::now())
    }
    pub(crate) fn epoch(&self) -> Instant {
        self.0
    }
    pub(crate) fn now_ms(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// The host's side: whether playback is linked and the transport everyone follows.
#[derive(Debug, Clone)]
pub(crate) struct Linked {
    pub(crate) clock: Clock,
    linked: bool,
    shared: SharedTransport,
}
impl Linked {
    pub(crate) fn new() -> Self {
        Self {
            clock: Clock::new(),
            linked: false,
            shared: SharedTransport { seq: 0, playing: false, beat: 0.0, start_ms: 0 },
        }
    }
    /// What participants follow: nothing while unlinked.
    pub(crate) fn view(&self) -> Option<SharedTransport> {
        self.linked.then_some(self.shared)
    }
    pub(crate) fn seq(&self) -> u64 {
        self.shared.seq
    }
    pub(crate) fn apply(&mut self, request: TransportRequest) -> Result<(), String> {
        let now = self.clock.now_ms();
        self.apply_at(request, now)
    }
    /// Orders one request at host time `now`. A start is scheduled `LEAD_MS` ahead; stop is immediate.
    pub(crate) fn apply_at(&mut self, request: TransportRequest, now: u64) -> Result<(), String> {
        request.validate()?;
        let next = self.shared.seq.checked_add(1).ok_or("shared transport counter exhausted")?;
        match request {
            TransportRequest::Link { linked } => {
                if linked == self.linked {
                    return Ok(());
                }
                self.linked = linked;
                // Linking stops everyone at the shared position: the same starting point for all.
                self.shared.playing = false;
                self.shared.start_ms = now;
            }
            _ if !self.linked => return Err("turn on Link playback first".into()),
            TransportRequest::Play { beat } => {
                self.shared.playing = true;
                self.shared.beat = beat;
                self.shared.start_ms = now.saturating_add(LEAD_MS);
            }
            TransportRequest::Stop {} => {
                self.shared.playing = false;
                self.shared.start_ms = now;
            }
            TransportRequest::Seek { beat } => {
                self.shared.beat = beat;
                self.shared.start_ms = if self.shared.playing { now.saturating_add(LEAD_MS) } else { now };
            }
        }
        self.shared.seq = next;
        Ok(())
    }
}

/// A guest's estimate of the host clock, from the poll round trips with the least delay.
#[derive(Debug, Default, Clone)]
pub(crate) struct ClockEstimate {
    samples: VecDeque<(u64, i64)>,
}
impl ClockEstimate {
    /// One exchange: sent and received on the local clock, `host_ms` stamped by the host between.
    pub(crate) fn add(&mut self, sent_ms: u64, received_ms: u64, host_ms: u64) {
        let Some(rtt) = received_ms.checked_sub(sent_ms).filter(|rtt| *rtt <= MAX_RTT_MS) else {
            return;
        };
        let middle = i128::from(sent_ms) + i128::from(rtt / 2);
        let Ok(offset) = i64::try_from(i128::from(host_ms) - middle) else {
            return;
        };
        if self.samples.len() == SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back((rtt, offset));
    }
    /// Host clock minus local clock, in milliseconds, from the fastest recent round trip.
    pub(crate) fn offset(&self) -> Option<i64> {
        self.samples.iter().min_by_key(|(rtt, _)| *rtt).map(|(_, offset)| *offset)
    }
}

/// Milliseconds until a start at `start_ms` on the host clock, negative once it has passed.
pub fn starts_in_ms(start_ms: u64, host_now_ms: i64) -> i64 {
    i64::try_from(i128::from(start_ms) - i128::from(host_now_ms)).unwrap_or(i64::MAX)
}

/// The beat a transport started at `beat` has reached `elapsed_ms` later at `tempo_bpm`, within 0..=MAX_BEAT.
pub fn beat_after(beat: f64, elapsed_ms: i64, tempo_bpm: f64) -> f64 {
    let elapsed = elapsed_ms.max(0) as f64 / 1000.0;
    let tempo = if tempo_bpm.is_finite() && tempo_bpm > 0.0 { tempo_bpm } else { 120.0 };
    let reached = beat + elapsed * tempo / 60.0;
    if reached.is_finite() { reached.clamp(0.0, MAX_BEAT) } else { beat.clamp(0.0, MAX_BEAT) }
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;
