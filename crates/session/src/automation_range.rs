// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A time range of one automation lane (`docs/spec/automation.md` › Ranges): copied and pasted
//! (onto any lane, its values placed where they sat on their own), duplicated right after itself,
//! cleared, or filled with a shape. An edit replaces the points inside the range and keeps the
//! envelope outside it: points at the range's edges hold the values it had there.

use serde_json::{Value, json};
use suhina_engine::MAX_START_BEAT;
use suhina_model::AutomationPoint;
use suhina_model::automation::{MAX_POINTS, Taper, curve_through, value_at};

use crate::automation::{Target, target};
use crate::commands::number;
use crate::{CommandError, Session};

/// Copied points: their offsets from the range's start, their places on the lane (0 … 1) and
/// their curves, and how long the range was.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AutomationClip {
    pub(crate) length: f64,
    pub(crate) points: Vec<(f64, f64, f64)>,
    /// Copied as picked points, not a range: pasted, they go in among the lane's points without
    /// edge points, replacing only those between the first and the last.
    pub(crate) picked: bool,
}

/// A lane's taper as the engine plays it, from the parameter `target` names: segments move
/// evenly in its position.
struct Lane<'a> {
    target: &'a Target,
    session: &'a Session,
}

impl Taper for Lane<'_> {
    fn position(&self, value: f64) -> f64 {
        self.target.position(self.session, value)
    }
    fn value(&self, position: f64) -> f64 {
        self.target.value_at_position(self.session, position)
    }
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
fn slice(points: &[AutomationPoint], start: f64, end: f64, stepped: bool, taper: &impl Taper) -> Vec<AutomationPoint> {
    let Some(first) = value_at(points, start, stepped, taper) else { return Vec::new() };
    let last = value_at(points, end, stepped, taper).unwrap_or(first);
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
    taper: &impl Taper,
) -> Vec<AutomationPoint> {
    let edges = value_at(points, start, stepped, taper).zip(value_at(points, end, stepped, taper));
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
    // Picked points (`points`): copied as they are, from the first of them.
    if let Some(list) = params.get("points").filter(|v| !v.is_null()) {
        let lane = points_of(session, &target)?;
        let mut picked: Vec<AutomationPoint> = Vec::new();
        for value in list.as_array().ok_or("points must be a list of point indices")? {
            let index = value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("each point is a point index, from 0, as in automation.list")?;
            picked
                .push(*lane.get(index).ok_or_else(|| format!("the lane has no point {index}; it has {}", lane.len()))?);
        }
        picked.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        let (Some(first), Some(last)) = (picked.first().map(|p| p.beat), picked.last().map(|p| p.beat)) else {
            return Err("points must name at least one point".into());
        };
        let points: Vec<(f64, f64, f64)> =
            picked.iter().map(|p| (p.beat - first, target.position(session, p.value), p.curve)).collect();
        let count = points.len();
        session.clipboards.automation = Some(AutomationClip { length: last - first, points, picked: true });
        return Ok(json!({ "param": target.param, "points": count, "length": last - first }));
    }
    let (start, end) = range(params)?;
    let points = points_of(session, &target)?;
    let stepped = target.stepped();
    let cut = slice(&points, start, end, stepped, &Lane { target: &target, session });
    if cut.is_empty() {
        return Err(format!("{} has no automation to copy", target.param).into());
    }
    let points: Vec<(f64, f64, f64)> =
        cut.iter().map(|p| (p.beat - start, target.position(session, p.value), p.curve)).collect();
    let count = points.len();
    session.clipboards.automation = Some(AutomationClip { length: end - start, points, picked: false });
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
    let merged = if clip.picked {
        // Picked points go in among the lane's, replacing those from the first to the last.
        let mut merged: Vec<AutomationPoint> =
            points.into_iter().filter(|p| p.beat < start || p.beat > end).chain(inner).collect();
        merged.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        merged
    } else {
        replace(&points, start, end, inner, target.stepped(), &Lane { target: &target, session })
    };
    let count = put(session, &target, merged)?;
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
    let cut = slice(&points, start, end, stepped, &Lane { target: &target, session });
    if cut.is_empty() {
        return Err(format!("{} has no automation to duplicate", target.param).into());
    }
    let inner: Vec<AutomationPoint> = cut.into_iter().map(|p| AutomationPoint { beat: p.beat + length, ..p }).collect();
    let merged = replace(&points, end, end + length, inner, stepped, &Lane { target: &target, session });
    let count = put(session, &target, merged)?;
    Ok(json!({ "param": target.param, "start": end, "end": end + length, "points": count }))
}

/// The points of `points` (time as a share of the span, place on the lane) that keep the line
/// within `tolerance` of the lane's height, the first and the last always: Ramer-Douglas-Peucker.
fn keep(points: &[(f64, f64)], tolerance: f64) -> Vec<usize> {
    fn walk(points: &[(f64, f64)], from: usize, to: usize, tolerance: f64, out: &mut Vec<usize>) {
        let (Some(&(x0, y0)), Some(&(x1, y1))) = (points.get(from), points.get(to)) else { return };
        let mut worst = (0.0, from);
        for (i, &(x, y)) in points.iter().enumerate().take(to).skip(from + 1) {
            let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 0.0 };
            let off = (y - (y0 + (y1 - y0) * t)).abs();
            if off > worst.0 {
                worst = (off, i);
            }
        }
        if worst.0 > tolerance {
            walk(points, from, worst.1, tolerance, out);
            out.push(worst.1);
            walk(points, worst.1, to, tolerance, out);
        }
    }
    let Some(last) = points.len().checked_sub(1) else { return Vec::new() };
    let mut out = vec![0];
    walk(points, 0, last, tolerance, &mut out);
    if last > 0 {
        out.push(last);
    }
    out
}

/// `automation.simplify {track, device?, param, start, end, tolerance?}`: the points inside the
/// range thinned to those the line needs, within `tolerance` of the lane's height (default 2 %),
/// each segment left given the curve that passes through the old line half-way along, so the
/// shape stays.
pub(crate) fn simplify(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let (start, end) = range(params)?;
    let tolerance = share(params, "tolerance")?.unwrap_or(0.02);
    let points = points_of(session, &target)?;
    let inside: Vec<usize> =
        (0..points.len()).filter(|&i| points.get(i).is_some_and(|p| p.beat >= start && p.beat <= end)).collect();
    if inside.len() < 3 || target.stepped() {
        return Ok(json!({ "param": target.param, "points": points.len(), "removed": 0 }));
    }
    let lane = Lane { target: &target, session };
    let span = end - start;
    let placed: Vec<(f64, f64)> = inside
        .iter()
        .filter_map(|&i| points.get(i))
        .map(|p| ((p.beat - start) / span, lane.position(p.value)))
        .collect();
    let kept: Vec<AutomationPoint> = keep(&placed, tolerance)
        .windows(2)
        .filter_map(|pair| {
            let a = points.get(*inside.get(*pair.first()?)?)?;
            let b = points.get(*inside.get(*pair.get(1)?)?)?;
            let (pa, pb) = (lane.position(a.value), lane.position(b.value));
            let mid = value_at(&points, (a.beat + b.beat) / 2.0, false, &lane).map(|v| lane.position(v))?;
            let curve =
                if (pb - pa).abs() > SAME { curve_through(0.5, (mid - pa) / (pb - pa)).unwrap_or(0.0) } else { 0.0 };
            Some(AutomationPoint { curve: (curve * 1000.0).round() / 1000.0, ..*a })
        })
        .collect();
    let (Some(first), Some(last)) =
        (inside.first().and_then(|&i| points.get(i)).copied(), inside.last().and_then(|&i| points.get(i)).copied())
    else {
        return Err("the range has no points".into());
    };
    let mut merged: Vec<AutomationPoint> = points.iter().filter(|p| p.beat < first.beat).copied().collect();
    merged.extend(kept);
    merged.push(last);
    merged.extend(points.iter().filter(|p| p.beat > last.beat).copied());
    let removed = points.len().saturating_sub(merged.len());
    let count = put(session, &target, merged)?;
    Ok(json!({ "param": target.param, "points": count, "removed": removed }))
}

/// `automation.scaleRange {track, device?, param, start, end, factor}`: the points inside the
/// range stretched up and down by `factor` (0 to 4) around the middle of where they sit on the
/// lane, kept inside the travel.
pub(crate) fn scale_range(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let (start, end) = range(params)?;
    let factor = number(params, "factor")?;
    if !(0.0..=4.0).contains(&factor) {
        return Err(format!("factor must be from 0 to 4, got {factor}").into());
    }
    let mut points = points_of(session, &target)?;
    let lane = Lane { target: &target, session };
    let places: Vec<f64> =
        points.iter().filter(|p| p.beat >= start && p.beat <= end).map(|p| lane.position(p.value)).collect();
    let (Some(low), Some(high)) = (places.iter().copied().reduce(f64::min), places.iter().copied().reduce(f64::max))
    else {
        return Err("the range has no points".into());
    };
    let middle = (low + high) / 2.0;
    for point in points.iter_mut().filter(|p| p.beat >= start && p.beat <= end) {
        let place = (middle + (lane.position(point.value) - middle) * factor).clamp(0.0, 1.0);
        point.value = lane.value(place);
    }
    let count = put(session, &target, points)?;
    Ok(json!({ "param": target.param, "points": count }))
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
    let merged = replace(&points, start, end, Vec::new(), target.stepped(), &Lane { target: &target, session });
    let count = put(session, &target, merged)?;
    Ok(json!({ "param": target.param, "points": count }))
}

/// A point of a shape over one cycle: its time (0 … 1), its place on the lane (0 … 1) and the
/// curve of the segment after it.
pub type ShapePoint = (f64, f64, f64);

/// A shape `automation.insertShape` draws: its points over one cycle, and whether it repeats.
pub fn shape_points(shape: &str) -> Option<(Vec<ShapePoint>, bool)> {
    // A sine from four bent segments a cycle: each quarter leaves the middle fast and reaches a
    // peak or a trough slowly, the curves that pass through a sine's value half-way along.
    let away = curve_through(0.5, std::f64::consts::FRAC_1_SQRT_2).unwrap_or(0.0);
    let back = curve_through(0.5, 1.0 - std::f64::consts::FRAC_1_SQRT_2).unwrap_or(0.0);
    Some(match shape {
        "sine" => {
            (vec![(0.0, 0.5, away), (0.25, 1.0, back), (0.5, 0.5, away), (0.75, 0.0, back), (1.0, 0.5, 0.0)], true)
        }
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

/// The cycles `params["cycles"]` asks for, from 1 to [`MAX_CYCLES`]; `None` when left out.
fn cycles(params: &Value) -> Result<Option<usize>, CommandError> {
    match params.get("cycles") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=MAX_CYCLES).contains(n))
            .map(Some)
            .ok_or_else(|| format!("cycles must be a whole number from 1 to {MAX_CYCLES}").into()),
    }
}

/// A share from 0 to 1 that `params[name]` gives, `None` when left out.
fn share(params: &Value, name: &str) -> Result<Option<f64>, CommandError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|n| (0.0..=1.0).contains(n))
            .map(Some)
            .ok_or_else(|| format!("{name} must be a number from 0 to 1").into()),
    }
}

/// A repeating shape's cycles over `start` to `end` when none are asked for: one a beat.
fn cycles_per_beat(start: f64, end: f64) -> usize {
    let beats = (end - start).round();
    if beats.is_finite() && beats >= 1.0 { (beats as usize).min(MAX_CYCLES) } else { 1 }
}

/// The segment a shape on point `index` of `points` replaces: from the point before it to it (for
/// the first point, from it to the next), and the two points' places on the lane.
fn segment(
    session: &Session,
    target: &Target,
    points: &[AutomationPoint],
    index: usize,
) -> Result<(f64, f64, f64, f64), CommandError> {
    let (a, b) = if index == 0 { (0, 1) } else { (index - 1, index) };
    let (Some(a), Some(b)) = (points.get(a), points.get(b)) else {
        return Err("the point needs a neighbour: a lane of at least two points".into());
    };
    if b.beat - a.beat <= SAME {
        return Err("the two points are at one beat: there is no segment between them".into());
    }
    Ok((a.beat, b.beat, target.position(session, a.value), target.position(session, b.value)))
}

/// Fills `start` to `end` of the lane with shape `name` between lane places `low` and `high`, a
/// repeating shape `cycles` times; answers the lane's point count.
#[allow(clippy::too_many_arguments)]
fn fill(
    session: &mut Session,
    target: &Target,
    start: f64,
    end: f64,
    (low, high): (f64, f64),
    name: &str,
    cycles: usize,
) -> Result<usize, CommandError> {
    let (cycle, repeats) =
        shape_points(name).ok_or_else(|| format!("no shape {name:?}; the shapes are {}", SHAPES.join(", ")))?;
    let cycles = if repeats { cycles.clamp(1, MAX_CYCLES) } else { 1 };
    let span = (end - start) / cycles as f64;
    let mut inner: Vec<AutomationPoint> = Vec::new();
    for n in 0..cycles {
        for (i, &(t, position, curve)) in cycle.iter().enumerate() {
            // A cycle starts where the one before ended: its first point is that end, unless the
            // shape jumps there (a saw).
            let beat = start + (n as f64 + t) * span;
            let value = target.value_at_position(session, low + position * (high - low));
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
    let points = points_of(session, target)?;
    let merged = replace(&points, start, end, inner, target.stepped(), &Lane { target, session });
    put(session, target, merged)
}

/// The segment shape `automation.addPoint {shape}` gives the segment that ends at the point it
/// added (from the point before it), between the two points' heights, a repeating shape once a
/// beat. Nothing for the lane's first point.
pub(crate) fn shape_new_segment(
    session: &mut Session,
    target: &Target,
    index: usize,
    name: &str,
) -> Result<(), CommandError> {
    if index == 0 {
        return Ok(());
    }
    let points = points_of(session, target)?;
    let Ok((start, end, a, b)) = segment(session, target, &points, index) else { return Ok(()) };
    fill(session, target, start, end, (a.min(b), a.max(b)), name, cycles_per_beat(start, end)).map(|_| ())
}

/// `automation.insertShape {track, device?, param, start, end, shape, cycles?}`: the range filled
/// with a shape over the parameter's whole range, a repeating one `cycles` times (default 1). With
/// `point` in place of the range: the segment from the point before it to it, between the two
/// points' heights (the whole range when they are level), a repeating shape once a beat unless
/// `cycles` says.
pub(crate) fn insert_shape(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let name = params.get("shape").and_then(Value::as_str).ok_or("shape must be a shape's name")?;
    if shape_points(name).is_none() {
        return Err(format!("no shape {name:?}; the shapes are {}", SHAPES.join(", ")).into());
    }
    let asked = cycles(params)?;
    let depth = share(params, "depth")?.unwrap_or(1.0);
    let (start, end, heights, cycles) = match params.get("point") {
        None | Some(Value::Null) => {
            let (start, end) = range(params)?;
            // A shape less than the whole travel swings around the lane's value where the range
            // starts (or `center`), kept inside the travel.
            let center = match share(params, "center")? {
                Some(center) => center,
                None => {
                    let points = points_of(session, &target)?;
                    let lane = Lane { target: &target, session };
                    value_at(&points, start, target.stepped(), &lane).map_or(0.5, |v| target.position(session, v))
                }
            };
            let low = (center - depth / 2.0).clamp(0.0, 1.0 - depth);
            (start, end, (low, low + depth), asked.unwrap_or(1))
        }
        Some(v) => {
            let index =
                v.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("point must be a point index, from 0")?;
            let points = points_of(session, &target)?;
            if index >= points.len() {
                return Err(format!("the lane has no point {index}; it has {}", points.len()).into());
            }
            let (start, end, a, b) = segment(session, &target, &points, index)?;
            let (low, high) = if (a - b).abs() < SAME { (0.0, 1.0) } else { (a.min(b), a.max(b)) };
            // Less than the whole depth: as much of the way between the two heights, around
            // their middle.
            let middle = (low + high) / 2.0;
            let half = (high - low) / 2.0 * depth;
            (start, end, (middle - half, middle + half), asked.unwrap_or_else(|| cycles_per_beat(start, end)))
        }
    };
    let count = fill(session, &target, start, end, heights, name, cycles)?;
    Ok(json!({ "param": target.param, "shape": name, "start": start, "end": end, "points": count }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use suhina_model::automation::Plain;

    fn p(beat: f64, value: f64) -> AutomationPoint {
        AutomationPoint { beat, value, curve: 0.0 }
    }

    #[test]
    fn a_replaced_range_keeps_the_envelope_outside_it() {
        let points = vec![p(0.0, 0.0), p(8.0, 1.0)];
        let out = replace(&points, 2.0, 4.0, vec![p(3.0, 0.5)], false, &Plain);
        let beats: Vec<(f64, f64)> = out.iter().map(|p| (p.beat, p.value)).collect();
        assert_eq!(beats, [(0.0, 0.0), (2.0, 0.25), (3.0, 0.5), (4.0, 0.5), (8.0, 1.0)]);
        for beat in [1.0, 5.0, 7.0] {
            assert!(
                (value_at(&out, beat, false, &Plain).unwrap() - value_at(&points, beat, false, &Plain).unwrap()).abs()
                    < 1e-9
            );
        }
    }

    #[test]
    fn a_slice_has_the_values_at_its_edges() {
        let points = vec![p(0.0, 0.0), p(4.0, 1.0), p(8.0, 0.0)];
        let cut: Vec<(f64, f64)> = slice(&points, 2.0, 6.0, false, &Plain).iter().map(|p| (p.beat, p.value)).collect();
        assert_eq!(cut, [(2.0, 0.5), (4.0, 1.0), (6.0, 0.5)]);
        assert!(slice(&[], 0.0, 1.0, false, &Plain).is_empty());
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
