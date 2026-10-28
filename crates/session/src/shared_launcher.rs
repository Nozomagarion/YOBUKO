// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded launcher placement without clipboard, file access or implicit replacement.
use crate::{CommandError, Session};
use serde_json::{Value, json};
use suhina_model::{ClipId, LaneData, TrackId};

fn id(params: &Value, key: &str) -> Result<u32, CommandError> {
    params[key]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("{key} must be a nonzero numeric id").into())
}
fn target(session: &Session, params: &Value) -> Result<(usize, usize), CommandError> {
    let track = TrackId(id(params, "track")?);
    let row = params["scene"]
        .as_u64()
        .filter(|v| (1..=128).contains(v))
        .and_then(|v| usize::try_from(v - 1).ok())
        .ok_or("scene must be 1..128")?;
    let p = session.project();
    if p.launcher.scenes.get(row).is_none() {
        return Err("scene is missing".into());
    }
    let column = p.launcher.columns.iter().position(|c| c.track == track).ok_or("track has no launcher column")?;
    let slot = p.launcher.columns.get(column).and_then(|c| c.slots.get(row)).ok_or("slot is missing")?;
    if slot.clip.is_some() {
        return Err("destination slot must be empty".into());
    }
    Ok((column, row))
}
pub(crate) fn create(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    target(session, params)?;
    let length = crate::commands::number(params, "length")?;
    if !(0.0..=120_000.0).contains(&length) || length == 0.0 {
        return Err("invalid slot clip length".into());
    }
    crate::files::create_clip(session, params)
}
pub(crate) fn copy(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let (column, row) = target(session, params)?;
    let p = session.project();
    let source = ClipId(id(params, "source")?);
    let mut clip = p.clip(source).cloned().ok_or("source clip is missing")?;
    let mut content = p.content(clip.content).cloned().ok_or("source content is missing")?;
    let track = p.track(TrackId(id(params, "track")?)).ok_or("destination track is missing")?;
    if content.lanes.iter().any(|lane| lane.target.is_some()) {
        return Err("copying multi-track patterns into a slot is not supported; use a single-track clip".into());
    }

    if content.lanes.is_empty()
        || content.lanes.iter().any(|l| match l.data {
            LaneData::Notes(_) => !track.content.notes,
            LaneData::Audio(_) => !track.content.audio,
        })
    {
        return Err("source clip is incompatible with destination track".into());
    }
    // Do every fallible check before consuming IDs or changing the project.
    content.id = crate::commands::new_content_id(p)?;
    let destination = session.project.launcher.columns.get_mut(column).and_then(|c| c.slots.get_mut(row));
    if destination.is_none() {
        return Err("destination slot is missing".into());
    }
    clip.id = session.project.new_clip_id()?;
    clip.content = content.id;
    clip.start = 0.0;
    let result = json!({"clip":clip.id.0,"scene":row + 1,"source":source.0});
    session.project.contents.push(content);
    if let Some(slot) = session.project.launcher.columns.get_mut(column).and_then(|c| c.slots.get_mut(row)) {
        slot.clip = Some(clip);
    }
    Ok(result)
}
