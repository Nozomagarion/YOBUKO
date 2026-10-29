// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::Catalog;
use serde::{Deserialize, Serialize};

/// A constant, manually confirmed 4/4 grid. Analysis BPM is kept separately on Track.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BeatGrid {
    pub track: u32,
    pub bpm: f64,
    pub first_beat: f64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLine {
    /// Zero-based beat index; bar = index / 4 + 1.
    pub index: u32,
    pub seconds: f64,
}
impl BeatGrid {
    pub fn valid_for(&self, seconds: f64) -> bool {
        self.track > 0
            && seconds.is_finite()
            && seconds > 0.0
            && seconds <= 1200.0
            && self.bpm.is_finite()
            && (20.0..=400.0).contains(&self.bpm)
            && self.first_beat.is_finite()
            && self.first_beat >= 0.0
            && self.first_beat < seconds
    }
    /// Bounded overview ticks; dense views retain bar starts and their original indices.
    pub fn lines(&self, seconds: f64, budget: usize) -> Vec<GridLine> {
        if !self.valid_for(seconds) || budget == 0 {
            return Vec::new();
        }
        let budget = budget.min(2048);
        let period = 60.0 / self.bpm;
        let count = ((seconds - self.first_beat) / period).ceil().min(8001.0) as usize;
        let stride = if count <= budget { 1 } else { count.div_ceil(budget).div_ceil(4) * 4 };
        (0..count)
            .step_by(stride)
            .filter_map(|index| {
                let at = self.first_beat + index as f64 * period;
                (at < seconds).then_some(GridLine { index: index as u32, seconds: at })
            })
            .collect()
    }
}
impl Catalog {
    pub(super) fn validate_grids(&self) -> Result<(), String> {
        if self.grids.len() > self.tracks.len() {
            return Err("too many DJ grids".into());
        }
        let mut ids = std::collections::HashSet::new();
        for grid in &self.grids {
            let track = self.tracks.iter().find(|t| t.id == grid.track).ok_or("unknown DJ grid track")?;
            if !ids.insert(grid.track) || !grid.valid_for(track.seconds) {
                return Err(
                    "invalid DJ grid tempo or anchor (reimport cannot shorten audio past the first beat)".into()
                );
            }
        }
        Ok(())
    }
    pub fn set_grid(&mut self, revision: u64, track: u32, value: Option<(f64, f64)>) -> Result<(), String> {
        if revision != self.revision {
            return Err("the DJ library changed; refresh before editing the grid".into());
        }
        if !self.tracks.iter().any(|t| t.id == track) {
            return Err("unknown DJ track".into());
        }
        let mut next = self.clone();
        let index = next.grids.iter().position(|g| g.track == track);
        match value {
            Some((bpm, first_beat)) => {
                let grid = BeatGrid { track, bpm, first_beat: if first_beat == 0.0 { 0.0 } else { first_beat } };
                if let Some(i) = index {
                    if let Some(g) = next.grids.get_mut(i) {
                        *g = grid;
                    }
                } else {
                    next.grids.push(grid);
                }
            }
            None => {
                next.grids.remove(index.ok_or("this track has no saved beat grid")?);
            }
        }
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(())
    }
}
