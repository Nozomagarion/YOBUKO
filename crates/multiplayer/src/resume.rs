// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Resuming a guest after its link drops (docs/spec/session-resume.md): the timings, the token a
//! host keeps for each guest, and a link wrapper that tells a network failure from a refusal.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crate::transport::Link;

/// How long the host keeps a dropped guest's place and accepts its token.
pub(crate) const GRACE: Duration = Duration::from_secs(120);
/// How long a guest keeps trying to come back before it gives up.
pub(crate) const RETRY: Duration = Duration::from_secs(60);
/// The first wait between two attempts; it doubles up to `MAX_BACKOFF`.
const FIRST_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(4);
/// How long a goodbye (leave or end) may take before the link is dropped anyway.
pub(crate) const FAREWELL: Duration = Duration::from_secs(1);

/// The timings a session runs with. Tests shorten them and may degrade every link.
#[derive(Clone)]
pub(crate) struct Tuning {
    pub grace: Duration,
    pub retry: Duration,
    #[cfg(test)]
    pub faults: Option<std::sync::Arc<crate::faults::Faults>>,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            grace: GRACE,
            retry: RETRY,
            #[cfg(test)]
            faults: None,
        }
    }
}
impl Tuning {
    /// The link a session uses; under test, behind the fault injector when one is set.
    pub fn wrap(&self, link: impl Link + Send + 'static) -> Box<dyn Link + Send> {
        #[cfg(test)]
        if let Some(faults) = &self.faults {
            return Box::new(crate::faults::Faulty::new(link, std::sync::Arc::clone(faults)));
        }
        Box::new(link)
    }
}

/// Waits between reconnection attempts: 0.25 s, 0.5 s, 1 s, 2 s, then 4 s.
pub(crate) struct Backoff {
    next: Duration,
    deadline: Instant,
}
impl Backoff {
    pub fn new(window: Duration) -> Self {
        Self { next: FIRST_BACKOFF, deadline: Instant::now() + window }
    }
    /// The next wait, or `None` once the window is spent.
    pub fn wait(&mut self) -> Option<Duration> {
        let left = self.deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())?;
        let wait = self.next.min(left);
        self.next = self.next.saturating_mul(2).min(MAX_BACKOFF);
        Some(wait)
    }
    pub fn spent(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

/// A link that remembers whether the network failed under it (an error, a timeout or the end of
/// the stream), so a dropped connection is retried while a refusal or a bad message is not.
pub(crate) struct Watch<L> {
    inner: L,
    failed: bool,
}
impl<L: Link> Watch<L> {
    pub fn new(inner: L) -> Self {
        Self { inner, failed: false }
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
}
impl<L: Link> Read for Watch<L> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self.inner.read(bytes) {
            Ok(0) if !bytes.is_empty() => {
                self.failed = true;
                Ok(0)
            }
            Ok(n) => Ok(n),
            Err(e) => {
                self.failed = true;
                Err(e)
            }
        }
    }
}
impl<L: Link> Write for Watch<L> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.inner.write(bytes).inspect_err(|_| self.failed = true)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush().inspect_err(|_| self.failed = true)
    }
}
impl<L: Link> Link for Watch<L> {
    fn set_deadline(&mut self, deadline: Instant) {
        self.inner.set_deadline(deadline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_its_cap_and_ends_with_the_window() {
        let mut backoff = Backoff::new(Duration::from_secs(60));
        let waits: Vec<_> = (0..7).map(|_| backoff.wait().unwrap()).collect();
        let ms: Vec<_> = waits.iter().map(Duration::as_millis).collect();
        assert_eq!(ms, [250, 500, 1000, 2000, 4000, 4000, 4000]);
        assert!(!backoff.spent());
        let mut short = Backoff::new(Duration::ZERO);
        assert!(short.wait().is_none());
        assert!(short.spent());
    }
}
