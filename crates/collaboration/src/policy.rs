// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact command and parameter admission, without namespace-wide permissions.

use serde_json::{Map, Value};
use suhina_session::{CommandKind, Session, commands};

use crate::Error;

/// A validated, immutable edit in the experiment's audited subset.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub(crate) command: &'static str,
    pub(crate) params: Value,
}

impl Edit {
    pub fn new(command: &str, params: Value) -> Result<Self, Error> {
        let command = match command {
            "track.rename" => "track.rename",
            "track.createShared" => "track.createShared",
            "track.setVolume" => "track.setVolume",
            "track.setPan" => "track.setPan",
            "device.addShared" => "device.addShared",
            "device.removeShared" => "device.removeShared",
            "device.setSharedParam" => "device.setSharedParam",
            "note.add" => "note.add",
            "note.update" => "note.update",
            "note.delete" => "note.delete",
            "clip.create" => "clip.create",
            "clip.insertAudio" => "clip.insertAudio",
            "clip.move" => "clip.move",
            "clip.trim" => "clip.trim",
            "clip.duplicate" => "clip.duplicate",
            "clip.delete" => "clip.delete",
            "clip.setEnabled" => "clip.setEnabled",
            _ => return Err(Error::UnsupportedCommand),
        };
        if !commands().iter().any(|spec| spec.id == command && spec.kind == CommandKind::Edit && !spec.policy.critical)
        {
            return Err(Error::UnsupportedCommand);
        }
        let object = params.as_object().ok_or(Error::InvalidParameters)?;
        let valid = match command {
            "track.createShared" => {
                keys(object, &["kind", "name"])
                    && matches!(object.get("kind").and_then(Value::as_str), Some("audio" | "midi"))
                    && object
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
            }
            "track.rename" => {
                keys(object, &["track", "name"])
                    && object.get("track").is_some_and(valid_id)
                    && object
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 256)
            }
            "track.setVolume" => {
                keys(object, &["tracks", "db"])
                    && object.get("tracks").is_some_and(valid_tracks)
                    && number_in(object.get("db"), -150.0, 6.0)
            }
            "track.setPan" => {
                keys(object, &["tracks", "pan"])
                    && object.get("tracks").is_some_and(valid_tracks)
                    && number_in(object.get("pan"), -1.0, 1.0)
            }
            "device.addShared" => {
                keys(object, &["track", "kind"])
                    && object.get("track").is_some_and(valid_id)
                    && matches!(object.get("kind").and_then(Value::as_str), Some("fm-synth" | "utility"))
            }
            "device.removeShared" => {
                keys(object, &["track", "device"])
                    && object.get("track").is_some_and(valid_id)
                    && object.get("device").and_then(Value::as_u64).is_some_and(|n| n < 16)
            }
            "device.setSharedParam" => {
                keys(object, &["track", "device", "param", "value"])
                    && object.get("track").is_some_and(valid_id)
                    && object.get("device").and_then(Value::as_u64).is_some_and(|n| n < 16)
                    && object.get("param").and_then(Value::as_str).is_some_and(|s| {
                        !s.is_empty()
                            && s.len() <= 64
                            && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_'))
                    })
                    && number_in(object.get("value"), -1_000_000.0, 1_000_000.0)
            }
            "note.add" => {
                keys(object, &["clip", "notes"])
                    && object.get("clip").is_some_and(valid_id)
                    && object.get("notes").and_then(Value::as_array).is_some_and(|notes| {
                        notes.len() == 1
                            && notes.first().and_then(Value::as_object).is_some_and(|note| {
                                keys(note, &["time", "length", "key", "velocity"]) && valid_note(note)
                            })
                    })
            }
            "note.update" => {
                keys(object, &["clip", "note", "time", "length", "key", "velocity", "muted"])
                    && object.get("clip").is_some_and(valid_id)
                    && valid_index(object.get("note"))
                    && valid_note(object)
                    && object.get("muted").is_some_and(Value::is_boolean)
            }
            "note.delete" => {
                keys(object, &["clip", "notes"])
                    && object.get("clip").is_some_and(valid_id)
                    && object
                        .get("notes")
                        .and_then(Value::as_array)
                        .is_some_and(|notes| notes.len() == 1 && valid_index(notes.first()))
            }
            "clip.insertAudio" => {
                keys(object, &["track", "start", "file", "rate", "channels", "frames"])
                    && object.get("track").is_some_and(valid_id)
                    && number_in(object.get("start"), 0.0, 120_000.0)
                    && object.get("file").and_then(Value::as_str).is_some_and(|s| {
                        s.strip_prefix("Audio/").and_then(|s| s.strip_suffix(".wav")).is_some_and(|s| {
                            s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        })
                    })
                    && object.get("rate").and_then(Value::as_u64).is_some_and(|n| (8000..=192000).contains(&n))
                    && object.get("channels").and_then(Value::as_u64).is_some_and(|n| (1..=2).contains(&n))
                    && object.get("frames").and_then(Value::as_u64).is_some_and(|n| n > 0 && n <= 16_777_184)
            }
            "clip.create" => {
                keys(object, &["track", "start", "length", "stack"])
                    && object.get("track").is_some_and(valid_id)
                    && stacked(object)
                    && number_in(object.get("start"), 0.0, 120_000.0)
                    && number_in(object.get("length"), f64::MIN_POSITIVE, 120_000.0)
            }
            "clip.move" => {
                keys(object, &["clip", "start", "toTrack", "stack"])
                    && object.get("clip").is_some_and(valid_id)
                    && stacked(object)
                    && object.get("toTrack").is_some_and(valid_id)
                    && number_in(object.get("start"), 0.0, 120_000.0)
            }
            "clip.trim" => {
                keys(object, &["clip", "start", "end", "stack"])
                    && object.get("clip").is_some_and(valid_id)
                    && stacked(object)
                    && number_in(object.get("start"), 0.0, 120_000.0)
                    && number_in(object.get("end"), 0.0, 120_000.0)
                    && object.get("end").and_then(Value::as_f64) > object.get("start").and_then(Value::as_f64)
            }
            "clip.duplicate" => keys(object, &["clips", "stack"]) && stacked(object) && one_clip(object),
            "clip.delete" => keys(object, &["clip"]) && object.get("clip").is_some_and(valid_id),
            "clip.setEnabled" => {
                keys(object, &["clips", "enabled"])
                    && one_clip(object)
                    && object.get("enabled").is_some_and(Value::is_boolean)
            }
            _ => false,
        };
        if !valid {
            return Err(Error::InvalidParameters);
        }
        Ok(Self { command, params })
    }
}

fn stacked(object: &Map<String, Value>) -> bool {
    object.get("stack").and_then(Value::as_bool) == Some(true)
}
fn one_clip(object: &Map<String, Value>) -> bool {
    object.get("clips").and_then(Value::as_array).is_some_and(|clips| {
        clips.len() == 1
            && clips
                .first()
                .and_then(Value::as_object)
                .is_some_and(|clip| keys(clip, &["clip"]) && clip.get("clip").is_some_and(valid_id))
    })
}

fn valid_index(value: Option<&Value>) -> bool {
    value.and_then(Value::as_u64).is_some_and(|i| i < 16_384)
}
fn valid_note(object: &Map<String, Value>) -> bool {
    number_in(object.get("time"), 0.0, 120_000.0)
        && number_in(object.get("length"), f64::MIN_POSITIVE, 120_000.0)
        && object.get("key").and_then(Value::as_u64).is_some_and(|key| key <= 127)
        && number_in(object.get("velocity"), 1.0, 127.0)
}

fn keys(object: &Map<String, Value>, permitted: &[&str]) -> bool {
    object.len() == permitted.len() && permitted.iter().all(|key| object.contains_key(*key))
}

fn valid_id(value: &Value) -> bool {
    value.as_u64().is_some_and(|id| id > 0 && u32::try_from(id).is_ok())
}

fn valid_tracks(value: &Value) -> bool {
    value.as_array().is_some_and(|ids| !ids.is_empty() && ids.len() <= 64 && ids.iter().all(valid_id))
}

fn number_in(value: Option<&Value>, min: f64, max: f64) -> bool {
    value.and_then(Value::as_f64).is_some_and(|n| n.is_finite() && (min..=max).contains(&n))
}

pub(crate) fn audition(session: &mut Session, command: &str, params: &Value) -> Result<Value, Error> {
    let object = params.as_object().ok_or(Error::InvalidParameters)?;
    let valid = match command {
        "transport.play" | "transport.stop" => object.is_empty(),
        "transport.setPosition" => keys(object, &["beats"]) && number_in(object.get("beats"), 0.0, 120_000.0),
        _ => return Err(Error::UnsupportedCommand),
    };
    if !valid {
        return Err(Error::InvalidParameters);
    }
    if !commands().iter().any(|spec| spec.id == command && spec.kind == CommandKind::Transport) {
        return Err(Error::UnsupportedCommand);
    }
    session.execute(command, params).map_err(Error::from)
}
