// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ history: each track a deck starts playing, by day, with the time it started, kept in
//! `dj-history.json` beside the DJ library (`dj.history.*`). It is its own small file, written
//! at once on Play, so it never waits for the library's worker. Titles are kept with the ids,
//! so a day still reads after a track leaves the library.

use crate::{CommandError, Projects};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const FORMAT: u64 = 1;
/// Days kept: the oldest goes when a new day would pass this.
const MAX_DAYS: usize = 366;
/// Plays kept in one day.
const MAX_PLAYS: usize = 2000;
/// Days the page is sent, newest first.
const SHOWN_DAYS: usize = 60;
/// The most a history file may weigh when read.
const MAX_BYTES: u64 = 16 << 20;

#[derive(Default)]
pub(crate) struct History {
    /// `[{day, plays: [{track, title, at}]}]`, oldest day first; `None` until read.
    days: Option<Vec<Value>>,
    path: Option<PathBuf>,
    error: Option<String>,
}

fn file(p: &Projects) -> Option<PathBuf> {
    p.browser.files.data_folder().and_then(|f| f.parent().map(|f| f.join("dj-history.json")))
}

/// A history read from `path`; nothing there is an empty history.
fn read(path: &Path) -> Result<Vec<Value>, String> {
    let bytes = match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
        Ok(m) if m.len() > MAX_BYTES => return Err("the DJ history file is too large".into()),
        Ok(_) => std::fs::read(path).map_err(|e| e.to_string())?,
    };
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let days = v["days"].as_array().filter(|_| v["format"] == FORMAT).ok_or("not a DJ history file")?;
    let valid = days.len() <= MAX_DAYS
        && days.iter().all(|d| {
            d["day"].as_str().is_some_and(|s| s.len() == 10)
                && d["plays"].as_array().is_some_and(|p| {
                    p.len() <= MAX_PLAYS && p.iter().all(|p| p["track"].is_u64() && p["title"].is_string())
                })
        });
    if valid { Ok(days.clone()) } else { Err("the DJ history file is damaged".into()) }
}

/// Writes `days` to `path` through a file beside it, so a crash never leaves half a history.
fn write(path: &Path, days: &[Value]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&json!({"format": FORMAT, "days": days})).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let next = path.with_extension("json.next");
    std::fs::write(&next, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&next, path).map_err(|e| e.to_string())
}

impl History {
    /// The days, read the first time; `None` (with the error kept) when the file cannot be read,
    /// so a damaged file is never written over.
    fn days(&mut self, path: Option<PathBuf>) -> Option<&mut Vec<Value>> {
        if self.path != path {
            self.days = None;
            self.path = path;
        }
        if self.days.is_none() {
            match self.path.as_deref().map_or(Ok(Vec::new()), read) {
                Ok(days) => {
                    self.days = Some(days);
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            }
        }
        self.days.as_mut()
    }

    fn save(&mut self) {
        if let (Some(path), Some(days)) = (&self.path, &self.days) {
            self.error = write(path, days).err();
        }
    }
}

/// Track `track` started playing on a deck now.
pub(crate) fn played(p: &mut Projects, track: &suhina_dj::Track) {
    let path = file(p);
    let now = chrono::Local::now();
    let (day, at) = (now.format("%Y-%m-%d").to_string(), now.format("%H:%M").to_string());
    let history = &mut p.dj.decks.history;
    let Some(days) = history.days(path) else { return };
    if days.last().is_none_or(|d| d["day"] != day.as_str()) {
        days.push(json!({"day": day, "plays": []}));
        if days.len() > MAX_DAYS {
            days.remove(0);
        }
    }
    if let Some(plays) = days.last_mut().and_then(|d| d["plays"].as_array_mut())
        && plays.len() < MAX_PLAYS
    {
        plays.push(json!({"track": track.id, "title": track.title, "at": at}));
    }
    history.save();
}

/// `dj.history.delete {day}`: forgets one day.
pub(crate) fn delete(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let day = v["day"].as_str().ok_or("choose a day")?.to_owned();
    let path = file(p);
    let history = &mut p.dj.decks.history;
    if history.days(path).is_none() {
        return Err(CommandError(history.error.clone().unwrap_or_default()));
    }
    let days = history.days.as_mut().ok_or("the DJ history cannot be read")?;
    let before = days.len();
    days.retain(|d| d["day"] != day.as_str());
    if days.len() == before {
        return Err("no history on that day".into());
    }
    history.save();
    match history.error.clone() {
        Some(e) => Err(CommandError(e)),
        None => Ok(json!({"deleted": day})),
    }
}

/// The newest days, newest first, as the page shows them.
pub(crate) fn snapshot(p: &mut Projects) -> Value {
    let path = file(p);
    let history = &mut p.dj.decks.history;
    let days: Vec<Value> =
        history.days(path).map(|d| d.iter().rev().take(SHOWN_DAYS).cloned().collect()).unwrap_or_default();
    json!({"days": days, "error": history.error})
}
