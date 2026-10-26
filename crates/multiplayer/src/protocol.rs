// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use suhina_collaboration::Edit;
use suhina_model::Project;

pub const PROTOCOL: u16 = 2;
pub const MAX_FRAME: usize = 65536;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrackControl {
    pub track: u32,
    pub name: String,
    pub db: f64,
    pub pan: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub revision: u64,
    pub tracks: Vec<TrackControl>,
}

impl Controls {
    pub fn validate(&self) -> Result<(), String> {
        if self.tracks.is_empty() || self.tracks.len() > crate::midi::MAX_TRACKS {
            return Err("invalid shared track count".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for t in &self.tracks {
            if t.track == 0
                || !ids.insert(t.track)
                || t.name.trim().is_empty()
                || t.name.len() > 256
                || t.name.chars().any(char::is_control)
                || !t.db.is_finite()
                || !(-150.0..=6.0).contains(&t.db)
                || !t.pan.is_finite()
                || !(-1.0..=1.0).contains(&t.pan)
            {
                return Err("invalid shared track controls".into());
            }
        }
        Ok(())
    }
    pub fn from_project(project: &Project, revision: u64) -> Result<Self, String> {
        let mut tracks = Vec::new();
        for track in &project.tracks {
            let id = track.id.0;
            let channel = track
                .channel
                .and_then(|id| project.channels.iter().find(|c| c.id == id))
                .ok_or("starter channel missing")?;
            let db = if channel.volume > 0.0 { 20.0 * channel.volume.log10() } else { -150.0 };
            tracks.push(TrackControl {
                track: id,
                name: track.name.clone(),
                db: db.clamp(-150.0, 6.0),
                pan: channel.pan,
            });
        }
        let value = Self { revision, tracks };
        value.validate()?;
        Ok(value)
    }
    pub fn changes(&self, project: &Project) -> Result<Vec<(&'static str, Value)>, String> {
        self.validate()?;
        let current = Self::from_project(project, self.revision)?;
        if self.tracks.len() != current.tracks.len()
            || self.tracks.iter().zip(&current.tracks).any(|(a, b)| a.track != b.track)
        {
            return Err("shared track identities do not match the MIDI copy".into());
        }
        let mut changes = Vec::new();
        for (t, old) in self.tracks.iter().zip(current.tracks.iter()) {
            if t.name != old.name {
                changes.push(("track.rename", json!({"track": t.track,"name":t.name})));
            }
            if (t.db - old.db).abs() > 0.00001 {
                changes.push(("track.setVolume", json!({"tracks":[t.track],"db":t.db})));
            }
            if (t.pan - old.pan).abs() > 0.00001 {
                changes.push(("track.setPan", json!({"tracks":[t.track],"pan":t.pan})));
            }
        }
        Ok(changes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", deny_unknown_fields)]
pub enum WireEdit {
    #[serde(rename = "track.rename")]
    Rename { track: u32, name: String },
    #[serde(rename = "track.setVolume")]
    Volume { track: u32, db: f64 },
    #[serde(rename = "track.setPan")]
    Pan { track: u32, pan: f64 },
}

impl WireEdit {
    pub fn edit(&self) -> Result<Edit, String> {
        if matches!(self,Self::Rename{name,..} if name.chars().any(char::is_control)) {
            return Err("shared track names cannot contain control characters".into());
        }
        let (id, params, track) = match self {
            Self::Rename { track, name } => ("track.rename", json!({"track":track,"name":name}), *track),
            Self::Volume { track, db } => ("track.setVolume", json!({"tracks":[track],"db":db}), *track),
            Self::Pan { track, pan } => ("track.setPan", json!({"tracks":[track],"pan":pan}), *track),
        };
        if track == 0 {
            return Err("a shared track needs a nonzero ID".into());
        }
        Edit::new(id, params).map_err(|e| e.to_string())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ClientMessage {
    Hello { protocol: u16, format: u16, secret: String },
    Download { protocol: u16, offset: usize },
    Poll { protocol: u16 },
    Edit { protocol: u16, sequence: u64, base_revision: u64, edit: WireEdit },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ServerMessage {
    Denied,
    Project { protocol: u16, format: u16, bytes: usize, sha256: String },
    Chunk { protocol: u16, offset: usize, data: String },
    State { protocol: u16, controls: Box<Controls>, accepted: Option<bool>, error: Option<String> },
}

pub fn read<T: for<'de> Deserialize<'de>>(io: &mut impl Read) -> Result<T, String> {
    let mut prefix = [0; 4];
    io.read_exact(&mut prefix).map_err(|e| format!("connection closed or timed out: {e}"))?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err("invalid network frame length".into());
    }
    let mut body = vec![0; size];
    io.read_exact(&mut body).map_err(|e| format!("incomplete network frame: {e}"))?;
    serde_json::from_slice(&body).map_err(|_| "invalid network message".into())
}
pub fn write<T: Serialize>(io: &mut impl Write, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "could not encode network message")?;
    if bytes.len() > MAX_FRAME {
        return Err("network message is too large".into());
    }
    io.write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| io.write_all(&bytes))
        .and_then(|_| io.flush())
        .map_err(|e| format!("connection write failed: {e}"))
}
