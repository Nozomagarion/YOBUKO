// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `take.*` commands: the take lanes a recording into the arrangement keeps under a track,
//! and comping parts of them onto it (`docs/spec/recording.md` › Take lanes, #909).

use super::*;

pub(super) static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "take.comp",
        label: "Copy a recorded take, or the part of it from start to end, onto its track, over what is there (comping)",
        params: &[
            TRACK,
            param("take", ParamType::Integer, "the take's number on the track, from 1, as the take lanes show"),
            optional("start", ParamType::Number, "where the part starts, in beats (default: where the take starts)"),
            optional("end", ParamType::Number, "where the part ends, in beats (default: where the take ends)"),
            optional(
                "color",
                ParamType::Text,
                "the colour the part takes on the track, \"#rrggbb\" (the take lane's), so the track shows which take each part came from",
            ),
        ],
        kind: CommandKind::Edit,
        run: comp_take,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "take.delete",
        label: "Delete a recorded take lane of a track; the track's clips stay",
        params: &[TRACK, param("take", ParamType::Integer, "the take's number on the track, from 1")],
        kind: CommandKind::Edit,
        run: delete_take,
        // Takes do not play: deleting one changes only what is shown.
        policy: Policy::UNHEARD,
    },
];

/// The index in `project.arrangement.takes` of take number `take` (from 1) of `track`.
fn take_index(p: &Project, track: TrackId, params: &Value) -> Result<usize, CommandError> {
    let number =
        params.get("take").and_then(Value::as_u64).filter(|n| *n >= 1).ok_or("take must be a number from 1")?;
    let number = usize::try_from(number).map_err(|_| "take is out of range")?;
    p.arrangement
        .takes
        .iter()
        .enumerate()
        .filter(|(_, t)| t.track == track)
        .nth(number - 1)
        .map(|(i, _)| i)
        .ok_or_else(|| format!("the track has no take {number}").into())
}

/// `take.comp`: the take's clips, cut to `start` … `end`, laid on the track over what they
/// cover, as new clips (docs/spec/recording.md › Take lanes).
fn comp_take(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let p = &mut session.project;
    let track = track_id(p, params)?;
    let index = take_index(p, track, params)?;
    let take = p.arrangement.takes.get(index).ok_or("the take is missing")?.clone();
    let first = take.clips.iter().map(|c| c.start).fold(f64::INFINITY, f64::min);
    let last = take.clips.iter().map(|c| c.start + c.length).fold(f64::NEG_INFINITY, f64::max);
    let bound = |key: &str, default: f64| -> Result<f64, CommandError> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v
                .as_f64()
                .filter(|b| b.is_finite() && (0.0..=MAX_START_BEAT).contains(b))
                .ok_or_else(|| format!("{key} must be a beat from 0 to {MAX_START_BEAT}").into()),
        }
    };
    let (from, to) = (bound("start", first)?, bound("end", last)?);
    if to.partial_cmp(&from) != Some(std::cmp::Ordering::Greater) {
        return Err("end must be after start".into());
    }
    let color = match params.get("color") {
        None | Some(Value::Null) => None,
        Some(_) => crate::clip_color::parse_color(params, "color")?,
    };
    let mut pieces = Vec::new();
    for clip in &take.clips {
        let (start, end) = (clip.start.max(from), (clip.start + clip.length).min(to));
        if end - start < 1e-9 {
            continue;
        }
        let offset = content_at(clip, start - clip.start, 1.0);
        let mut piece = Clip { id: p.new_clip_id()?, start, length: end - start, offset, ..clip.clone() };
        piece.fade_in = 0.0;
        piece.fade_out = 0.0;
        if color.is_some() {
            piece.color = color;
        }
        pieces.push(piece);
    }
    if pieces.is_empty() {
        return Err("the take has nothing between start and end".into());
    }
    let lane = match p.arrangement.lanes.iter().position(|l| l.track == Some(track)) {
        Some(lane) => lane,
        None => {
            let name = p.track(track).map(|t| t.name.clone()).unwrap_or_default();
            let id = p.new_lane_id()?;
            p.arrangement.lanes.push(ArrangementLane { id, name, track: Some(track), clips: Vec::new() });
            p.arrangement.lanes.len() - 1
        }
    };
    let lane = p.arrangement.lanes.get_mut(lane).ok_or("the lane is missing")?;
    let clips: Vec<u32> = pieces.iter().map(|c| c.id.0).collect();
    for piece in pieces {
        make_room(&mut lane.clips, piece.start, piece.start + piece.length);
        lane.clips.push(piece);
    }
    lane.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    // The second parts of clips the pieces split get their ids (ADR 0025).
    p.assign_ids()?;
    Ok(json!({ "track": track.0, "take": take.name, "start": from, "end": to, "clips": clips }))
}

/// `take.delete`.
fn delete_take(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let p = &mut session.project;
    let track = track_id(p, params)?;
    let index = take_index(p, track, params)?;
    let take = p.arrangement.takes.remove(index);
    Ok(json!({ "track": track.0, "deleted": take.name }))
}
