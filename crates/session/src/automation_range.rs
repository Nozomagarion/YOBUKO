// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A time range of one automation lane (`docs/spec/automation.md` › Ranges): copied and pasted
//! (onto any lane, its values placed where they sat on their own), duplicated right after itself,
//! cleared, or filled with a shape. An edit replaces the points inside the range and keeps the
//! envelope outside it: points at the range's edges hold the values it had there.

use serde_json::{Value, json};
use suhina_engine::MAX_START_BEAT;
use suhina_model::AutomationPoint;
use suhina_model::automation::{MAX_POINTS, value_at};

use crate::automation::{Target, target};
use crate::commands::number;
use crate::{CommandError, Session};

/// Copied points: their offsets from the range's start, their places on the lane (0 … 1) and
/// their curves, and how long the range was.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AutomationClip {
    pub(crate) length: f64,
    pub(crate) points: Vec<(f64, f64, f64)>,
}

/// Values closer than this are one, for an edge point that would repeat its neighbour.
const SAME: f64 = 1e-9;

/// Most cycles a shape repeats in its range.
pub(crate) const MAX_CYCLES: usize = 512;

/// `start` and `end` from `params`: from beat 0, the end after the start.
fn range(params: &Value) -> Result<(f64, f64), CommandError> {
    let start = number(params, "start")?;
    let end = number(params, "end")?;
    if !(0.0..=MAX_START_BEAT).contains(&start) || !(0.0..=MAX_START_BEAT).contains(&end) {
        return Err(format!("start and end must be from 0 to {MAX_START_BEAT}").into());
    }
    if end <= start {
        return Err(format!("end must be after start, got {start} to {end}").into());
    }
    Ok((start, end))
}

/// The envelope of `points` from `start` to `end`: its value at each edge and the points
/// between. Empty when the lane has no points.
fn slice(points: &[AutomationPoint], start: f64, end: f64, stepped: bool) -> Vec<AutomationPoint> {
    let Some(first) = value_at(points, start, stepped) else { return Vec::new() };
    let last = value_at(points, end, stepped).unwrap_or(first);
    let curve_at = |beat: f64| {
        let index = points.partition_point(|p| p.beat <= beat);
        index.checked_sub(1).and_then(|i| points.get(i)).map_or(0.0, |p| p.curve)
    };
    let mut out = vec![AutomationPoint { beat: start, value: first, curve: curve_at(start) }];
    out.extend(points.iter().filter(|p| p.beat > start && p.beat < end).copied());
    out.push(AutomationPoint { beat: end, value: last, curve: 0.0 });
    out
}

/// `points` with what was from `start` to `end` replaced by `inner` (beats in that range), the
/// values at the edges kept so the envelope outside does not change.
fn replace(
    points: &[AutomationPoint],
    start: f64,
    end: f64,
    inner: Vec<AutomationPoint>,
    stepped: bool,
) -> Vec<AutomationPoint> {
    let edges = value_at(points, start, stepped).zip(value_at(points, end, stepped));
    let after_curve = {
        let index = points.partition_point(|p| p.beat <= end);
        index.checked_sub(1).and_then(|i| points.get(i)).map_or(0.0, |p| p.curve)
    };
    let mut out: Vec<AutomationPoint> = points.iter().filter(|p| p.beat < start).copied().collect();
    if let Some((before, _)) = edges
        && inner.first().is_none_or(|p| p.beat > start || (p.value - before).abs() > SAME)
    {
        out.push(AutomationPoint { beat: start, value: before, curve: 0.0 });
    }
    let last_inner = inner.last().copied();
    out.extend(inner);
    if let Some((_, after)) = edges
        && last_inner.is_none_or(|p| p.beat < end || (p.value - after).abs() > SAME)
    {
        out.push(AutomationPoint { beat: end, value: after, curve: after_curve });
    }
    out.extend(points.iter().filter(|p| p.beat > end).copied());
    out
}

/// Puts `points` on the lane `target` names, making the lane when it has none.
fn put(session: &mut Session, target: &Target, points: Vec<AutomationPoint>) -> Result<usize, CommandError> {
    if points.len() > MAX_POINTS {
        return Err(format!("a lane holds at most {MAX_POINTS} points").into());
    }
    let lanes = target.lanes(session)?;
    let count = points.len();
    match lanes.iter_mut().find(|l| target.matches(l)) {
        Some(lane) => lane.points = points,
        None => {
            let mut lane = suhina_model::AutomationLane::new(target.param.clone());
            lane.points = points;
            lanes.push(lane);
        }
    }
    Ok(count)
}

/// The points of the lane `target` names (none when it has no lane).
fn points_of(session: &mut Session, target: &Target) -> Result<Vec<AutomationPoint>, CommandError> {
    Ok(target.lanes(session)?.iter().find(|l| target.matches(l)).map(|l| l.points.clone()).unwrap_or_default())
}

/// `automation.copy {track, device?, param, start, end}`: the envelope of the range, for
/// `automation.paste`.
pub(crate) fn copy(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let (start, end) = range(params)?;
    let points = points_of(session, &target)?;
    let stepped = target.stepped();
    let cut = slice(&points, start, end, stepped);
    if cut.is_empty() {
        return Err(format!("{} has no automation to copy", target.param).into());
    }
    let points: Vec<(f64, f64, f64)> =
        cut.iter().map(|p| (p.beat - start, target.position(session, p.value), p.curve)).collect();
    let count = points.len();
    session.clipboards.automation = Some(AutomationClip { length: end - start, points });
    Ok(json!({ "param": target.param, "points": count, "length": end - start }))
}

/// `automation.paste {track, device?, param, start}`: the copied range from `start`, over what
/// is there; on another parameter its values sit where they sat on their own lane.
pub(crate) fn paste(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let start = number(params, "start")?;
    if !(0.0..=MAX_START_BEAT).contains(&start) {
        return Err(format!("start must be from 0 to {MAX_START_BEAT}").into());
    }
    let clip =
        session.clipboards.automation.clone().ok_or("no automation has been copied; automation.copy copies a range")?;
    let end = start + clip.length;
    let inner: Vec<AutomationPoint> = clip
        .points
        .iter()
        .map(|&(offset, position, curve)| AutomationPoint {
            beat: start + offset,
            value: target.value_at_position(session, position),
            curve,
        })
        .collect();
    let points = points_of(session, &target)?;
    let count = put(session, &target, replace(&points, start, end, inner, target.stepped()))?;
    Ok(json!({ "param": target.param, "start": start, "end": end, "points": count }))
}

/// `automation.duplicate {track, device?, param, start, end}`: a copy of the range right after
/// it; the copy's range is the answer, so the next duplicate goes on from there. The clipboard
/// stays as it was.
pub(crate) fn duplicate(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let (start, end) = range(params)?;
    let length = end - start;
    if end + length > MAX_START_BEAT {
        return Err("the copy would end past the arrangement's last beat".into());
    }
    let points = points_of(session, &target)?;
    let stepped = target.stepped();
    let cut = slice(&points, start, end, stepped);
    if cut.is_empty() {
        return Err(format!("{} has no automation to duplicate", target.param).into());
    }
    let inner: Vec<AutomationPoint> = cut.into_iter().map(|p| AutomationPoint { beat: p.beat + length, ..p }).collect();
    let count = put(session, &target, replace(&points, end, end + length, inner, stepped))?;
    Ok(json!({ "param": target.param, "start": end, "end": end + length, "points": count }))
}

/// `automation.deleteRange {track, device?, param, start, end}`: the points inside the range go;
/// the values at its edges stay, so the envelope outside is as it was.
pub(crate) fn delete_range(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let (start, end) = range(params)?;
    let points = points_of(session, &target)?;
    if points.is_empty() {
        return Err(format!("{} has no automation", target.param).into());
    }
    let count = put(session, &target, replace(&points, start, end, Vec::new(), target.stepped()))?;
    Ok(json!({ "param": target.param, "points": count }))
}

/// A point of a shape over one cycle: its time (0 … 1), its place on the lane (0 … 1) and the
/// curve of the segment after it.
pub type ShapePoint = (f64, f64, f64);

/// A shape `automation.insertShape` draws: its points over one cycle, and whether it repeats.
pub fn shape_points(shape: &str) -> Option<(Vec<ShapePoint>, bool)> {
    let sine = |n: usize| -> Vec<ShapePoint> {
        (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                (t, 0.5 + 0.5 * (std::f64::consts::TAU * t).sin(), 0.0)
            })
            .collect()
    };
    Some(match shape {
        "sine" => (sine(16), true),
        "triangle" => (vec![(0.0, 0.0, 0.0), (0.5, 1.0, 0.0), (1.0, 0.0, 0.0)], true),
        "saw_up" => (vec![(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)], true),
        "saw_down" => (vec![(0.0, 1.0, 0.0), (1.0, 0.0, 0.0)], true),
        "square" => (vec![(0.0, 1.0, 0.0), (0.5, 1.0, 0.0), (0.5, 0.0, 0.0), (1.0, 0.0, 0.0)], true),
        "ramp_up" => (vec![(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)], false),
        "ramp_down" => (vec![(0.0, 1.0, 0.0), (1.0, 0.0, 0.0)], false),
        "curve_up" => (vec![(0.0, 0.0, -0.7), (1.0, 1.0, 0.0)], false),
        "curve_down" => (vec![(0.0, 1.0, 0.7), (1.0, 0.0, 0.0)], false),
        "swell" => (vec![(0.0, 0.0, 0.6), (0.5, 1.0, -0.6), (1.0, 0.0, 0.0)], false),
        _ => return None,
    })
}

/// The shapes by name, in the order the menu shows them.
pub const SHAPES: [&str; 10] =
    ["sine", "triangle", "saw_up", "saw_down", "square", "ramp_up", "ramp_down", "curve_up", "curve_down", "swell"];

/// `automation.insertShape {track, device?, param, start, end, shape, cycles?}`: the range
/// filled with a shape over the parameter's whole range, a repeating one `cycles` times.
pub(crate) fn insert_shape(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let (start, end) = range(params)?;
    let name = params.get("shape").and_then(Value::as_str).ok_or("shape must be a shape's name")?;
    let (cycle, repeats) =
        shape_points(name).ok_or_else(|| format!("no shape {name:?}; the shapes are {}", SHAPES.join(", ")))?;
    let cycles = match params.get("cycles") {
        None | Some(Value::Null) => 1,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=MAX_CYCLES).contains(n))
            .ok_or_else(|| format!("cycles must be a whole number from 1 to {MAX_CYCLES}"))?,
    };
    let cycles = if repeats { cycles } else { 1 };
    let span = (end - start) / cycles as f64;
    let mut inner: Vec<AutomationPoint> = Vec::new();
    for n in 0..cycles {
        for (i, &(t, position, curve)) in cycle.iter().enumerate() {
            // A cycle starts where the one before ended: its first point is that end, unless the
            // shape jumps there (a saw).
            let beat = start + (n as f64 + t) * span;
            let value = target.value_at_position(session, position);
            if i == 0
                && let Some(last) = inner.last()
                && (last.beat - beat).abs() < SAME
                && (last.value - value).abs() < SAME
            {
                if let Some(last) = inner.last_mut() {
                    last.curve = curve;
                }
                continue;
            }
            inner.push(AutomationPoint { beat, value, curve });
        }
    }
    let points = points_of(session, &target)?;
    let count = put(session, &target, replace(&points, start, end, inner, target.stepped()))?;
    Ok(json!({ "param": target.param, "shape": name, "points": count }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(beat: f64, value: f64) -> AutomationPoint {
        AutomationPoint { beat, value, curve: 0.0 }
    }

    #[test]
    fn a_replaced_range_keeps_the_envelope_outside_it() {
        let points = vec![p(0.0, 0.0), p(8.0, 1.0)];
        let out = replace(&points, 2.0, 4.0, vec![p(3.0, 0.5)], false);
        let beats: Vec<(f64, f64)> = out.iter().map(|p| (p.beat, p.value)).collect();
        assert_eq!(beats, [(0.0, 0.0), (2.0, 0.25), (3.0, 0.5), (4.0, 0.5), (8.0, 1.0)]);
        for beat in [1.0, 5.0, 7.0] {
            assert!((value_at(&out, beat, false).unwrap() - value_at(&points, beat, false).unwrap()).abs() < 1e-9);
        }
    }

    #[test]
    fn a_slice_has_the_values_at_its_edges() {
        let points = vec![p(0.0, 0.0), p(4.0, 1.0), p(8.0, 0.0)];
        let cut: Vec<(f64, f64)> = slice(&points, 2.0, 6.0, false).iter().map(|p| (p.beat, p.value)).collect();
        assert_eq!(cut, [(2.0, 0.5), (4.0, 1.0), (6.0, 0.5)]);
        assert!(slice(&[], 0.0, 1.0, false).is_empty());
    }

    #[test]
    fn every_shape_starts_and_ends_its_cycle_in_range() {
        for name in SHAPES {
            let (points, _) = shape_points(name).unwrap();
            assert!(points.first().is_some_and(|p| p.0 == 0.0), "{name}");
            assert!(points.last().is_some_and(|p| (p.0 - 1.0).abs() < 1e-12), "{name}");
            assert!(points.iter().all(|p| (0.0..=1.0).contains(&p.1)), "{name}");
        }
        assert!(shape_points("spiral").is_none());
    }
}
