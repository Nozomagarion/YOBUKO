// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test-only fault injection: a `Link` wrapper that adds latency, throttles bandwidth and cuts the
//! connection after a number of bytes or on demand. It wraps the online stream and the LAN TLS
//! stream alike, through `Tuning::faults` on either side of a session.

#![allow(clippy::unwrap_used)] // Test-only helpers.

use std::io::{Read, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::transport::Link;

/// A slice of a blocking read, so a cut on demand interrupts a link that waits for data.
const SLICE: Duration = Duration::from_millis(20);
/// Largest read or write passed through at once, so throttling paces large transfers smoothly.
const CHUNK: usize = 4096;

/// How every link wrapped with these faults behaves. Shared by the test and the links.
#[derive(Default)]
pub(crate) struct Faults {
    /// Added once each time a link turns from writing to reading or back: a one-way delay.
    pub latency: Duration,
    /// Bytes per second in each direction; `None` is unthrottled.
    pub rate: Option<u64>,
    /// Each link fails once it has carried this many bytes (both directions).
    cut_after: Mutex<Option<u64>>,
    /// Every link fails while this is set, new ones included.
    cut: AtomicBool,
    /// Links that have failed because of these faults.
    cuts: AtomicU64,
    /// Bytes carried by every link wrapped with these faults, both directions.
    carried: AtomicU64,
}
impl Faults {
    pub fn new(latency: Duration, rate: Option<u64>) -> Self {
        Self { latency, rate, ..Self::default() }
    }
    /// Cuts every link now and refuses new ones until `heal`.
    pub fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
    }
    pub fn heal(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }
    /// Cuts each link (current and future) once it has carried `bytes`; `None` turns this off.
    pub fn cut_after(&self, bytes: Option<u64>) {
        *self.cut_after.lock().unwrap() = bytes;
    }
    pub fn cuts(&self) -> u64 {
        self.cuts.load(Ordering::SeqCst)
    }
    pub fn carried(&self) -> u64 {
        self.carried.load(Ordering::SeqCst)
    }
}

#[derive(PartialEq)]
enum Turn {
    Idle,
    Reading,
    Writing,
}

/// A link degraded by shared `Faults`. Once cut it drops the inner link, as a lost network would
/// leave it, and fails every later call.
pub(crate) struct Faulty<L> {
    inner: Option<L>,
    faults: std::sync::Arc<Faults>,
    carried: u64,
    deadline: Instant,
    turn: Turn,
}
impl<L: Link> Faulty<L> {
    pub fn new(inner: L, faults: std::sync::Arc<Faults>) -> Self {
        Self {
            inner: Some(inner),
            faults,
            carried: 0,
            deadline: Instant::now() + crate::transport::TIMEOUT,
            turn: Turn::Idle,
        }
    }
    fn check(&mut self) -> std::io::Result<()> {
        let limit = *self.faults.cut_after.lock().unwrap();
        let cut = self.faults.cut.load(Ordering::SeqCst) || limit.is_some_and(|n| self.carried >= n);
        if cut && self.inner.take().is_some() {
            self.faults.cuts.fetch_add(1, Ordering::SeqCst);
        }
        if self.inner.is_none() {
            return Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "fault injection cut the link"));
        }
        Ok(())
    }
    fn turn(&mut self, turn: Turn) {
        if self.turn != turn {
            self.turn = turn;
            std::thread::sleep(self.faults.latency);
        }
    }
    /// The most bytes this call may carry before the cut threshold.
    fn allowance(&self, wanted: usize) -> usize {
        let limit = *self.faults.cut_after.lock().unwrap();
        let left = limit.map_or(u64::MAX, |n| n.saturating_sub(self.carried));
        wanted.min(CHUNK).min(usize::try_from(left).unwrap_or(usize::MAX)).max(1)
    }
    fn pace(&mut self, bytes: usize) {
        self.carried += bytes as u64;
        self.faults.carried.fetch_add(bytes as u64, Ordering::SeqCst);
        if let Some(rate) = self.faults.rate.filter(|r| *r > 0) {
            std::thread::sleep(Duration::from_secs_f64(bytes as f64 / rate as f64));
        }
    }
}
impl<L: Link> Read for Faulty<L> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.check()?;
        self.turn(Turn::Reading);
        let size = self.allowance(bytes.len());
        loop {
            self.check()?;
            // Wait in short slices so a cut on demand interrupts a blocked read.
            let now = Instant::now();
            if now >= self.deadline {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "fault injection deadline"));
            }
            let slice = self.deadline.min(now + SLICE);
            let inner = self.inner.as_mut().unwrap();
            inner.set_deadline(slice);
            match inner.read(&mut bytes[..size]) {
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                Ok(n) => {
                    self.pace(n);
                    return Ok(n);
                }
                Err(e) => return Err(e),
            }
        }
    }
}
impl<L: Link> Write for Faulty<L> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.check()?;
        self.turn(Turn::Writing);
        let size = self.allowance(bytes.len());
        let deadline = self.deadline;
        let inner = self.inner.as_mut().unwrap();
        inner.set_deadline(deadline);
        let n = inner.write(&bytes[..size])?;
        self.pace(n);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.check()?;
        self.inner.as_mut().unwrap().flush()
    }
}
impl<L: Link> Link for Faulty<L> {
    fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
        if let Some(inner) = self.inner.as_mut() {
            inner.set_deadline(deadline);
        }
    }
}

#[cfg(test)]
#[path = "faults_tests.rs"]
mod tests;
