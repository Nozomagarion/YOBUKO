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
            "track.setVolume" => "track.setVolume",
            "track.setPan" => "track.setPan",
            "note.add" => "note.add",
            "note.update" => "note.update",
            "note.delete" => "note.delete",
            _ => return Err(Error::UnsupportedCommand),
        };
        if !commands().iter().any(|spec| spec.id == command && spec.kind == CommandKind::Edit && !spec.policy.critical)
        {
            return Err(Error::UnsupportedCommand);
        }
        let object = params.as_object().ok_or(Error::InvalidParameters)?;
        let valid = match command {
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
            _ => false,
        };
        if !valid {
            return Err(Error::InvalidParameters);
        }
        Ok(Self { command, params })
    }
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
