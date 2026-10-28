// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::Catalog;
use serde::{Deserialize, Serialize};

pub const MAX_CUES_PER_TRACK: usize = 16;
pub const MAX_CUES: usize = 8000;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    pub id: u32,
    pub name: String,
    pub seconds: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CueSet {
    pub track: u32,
    pub next_id: u64,
    pub points: Vec<Cue>,
}
#[derive(Debug)]
pub enum CueEdit {
    Add { name: String, seconds: f64 },
    Rename { cue: u32, name: String },
    Move { cue: u32, seconds: f64 },
    Delete(u32),
}
fn name_valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.trim() == name && !name.chars().any(char::is_control)
}
impl Catalog {
    pub(super) fn validate_cues(&self) -> Result<(), String> {
        if self.cues.len() > self.tracks.len() {
            return Err("invalid DJ cue sets".into());
        }
        let mut tracks = std::collections::HashSet::new();
        let mut count = 0usize;
        for set in &self.cues {
            let track = self.tracks.iter().find(|t| t.id == set.track).ok_or("unknown DJ cue track")?;
            count = count.checked_add(set.points.len()).ok_or("DJ cue limit")?;
            if !tracks.insert(set.track)
                || set.next_id == 0
                || set.next_id > u64::from(u32::MAX) + 1
                || set.points.len() > MAX_CUES_PER_TRACK
                || count > MAX_CUES
            {
                return Err("invalid DJ cue counter or limit".into());
            }
            let mut ids = std::collections::HashSet::new();
            let mut previous = None;
            for cue in &set.points {
                if cue.id == 0
                    || u64::from(cue.id) >= set.next_id
                    || !ids.insert(cue.id)
                    || !name_valid(&cue.name)
                    || !cue.seconds.is_finite()
                    || cue.seconds < 0.0
                    || cue.seconds >= track.seconds
                    || previous.is_some_and(|(s, id)| cue.seconds < s || (cue.seconds == s && cue.id <= id))
                {
                    return Err("invalid DJ cue name, position or id (reimport cannot shorten audio past a cue)".into());
                }
                previous = Some((cue.seconds, cue.id));
            }
        }
        Ok(())
    }

    pub fn edit_cue(&mut self, revision: u64, track: u32, edit: CueEdit) -> Result<(), String> {
        if revision != self.revision {
            return Err("the DJ library changed; refresh before editing cues".into());
        }
        if !self.tracks.iter().any(|t| t.id == track) {
            return Err("unknown DJ track".into());
        }
        let name = match &edit {
            CueEdit::Add { name, .. } | CueEdit::Rename { name, .. } => Some(name),
            _ => None,
        };
        if name.is_some_and(|n| n.len() > 64 || n.chars().any(char::is_control)) {
            return Err("invalid DJ cue name".into());
        }
        let mut next = self.clone();
        if !next.cues.iter().any(|s| s.track == track) {
            if !matches!(edit, CueEdit::Add { .. }) {
                return Err("unknown DJ cue".into());
            }
            next.cues.push(CueSet { track, next_id: 1, points: Vec::new() });
        }
        let set = next.cues.iter_mut().find(|s| s.track == track).ok_or("unknown DJ cue track")?;
        match edit {
            CueEdit::Add { name, seconds } => {
                if set.points.len() >= MAX_CUES_PER_TRACK {
                    return Err("this track already has 16 cue points".into());
                }
                let id = u32::try_from(set.next_id).map_err(|_| "DJ cue ids exhausted")?;
                set.next_id = set.next_id.checked_add(1).ok_or("DJ cue ids exhausted")?;
                set.points.push(Cue { id, name: name.trim().into(), seconds });
            }
            CueEdit::Delete(id) => {
                let index = set.points.iter().position(|c| c.id == id).ok_or("unknown DJ cue")?;
                set.points.remove(index);
            }
            CueEdit::Rename { cue, name } => {
                set.points.iter_mut().find(|c| c.id == cue).ok_or("unknown DJ cue")?.name = name.trim().into()
            }
            CueEdit::Move { cue, seconds } => {
                set.points.iter_mut().find(|c| c.id == cue).ok_or("unknown DJ cue")?.seconds = seconds
            }
        }
        for cue in &mut set.points {
            if cue.seconds == 0.0 {
                cue.seconds = 0.0;
            }
        }
        set.points.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then(a.id.cmp(&b.id)));
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(())
    }
}
