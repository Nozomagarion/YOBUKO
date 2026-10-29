// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Waveforms of audio files, as `docs/spec/waveforms.md` describes: peaks (lowest, highest and
//! RMS per channel) at a few resolutions, computed once when a file is decoded, and one way of
//! drawing them for clips, the browser's preview and the sample map. Drawing reads the level
//! closest to the zoom, the samples themselves when zoomed in further, so a column costs a few
//! numbers at any zoom; the shape is an anti-aliased fill with the RMS as a brighter band inside,
//! one lane per channel when there is room, and a line through the samples once a sample is
//! wider than a pixel.

#[cfg(test)]
use std::sync::Arc;

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};
#[cfg(test)]
use suhina_engine::AudioData;

pub use suhina_media::waveform::{Bucket, LEVELS, Lane, MAX_LANES, Peaks, Waveforms, from_pool, peaks};
/// A clip at least this tall draws a stereo file as two lanes, one per channel.
pub const STEREO_MIN_HEIGHT: f32 = 40.0;
/// The quietest level the dB scale shows, in dB below full scale.
pub const DB_RANGE: f32 = 48.0;
const DOT_SPACING: f32 = 6.0;

/// How sample values map onto the height of a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scale {
    /// Full scale at the edges, silence in the middle.
    #[default]
    Linear,
    /// Decibels: full scale at the edges, [`DB_RANGE`] below it in the middle, so quiet parts
    /// show.
    Db,
}

impl Scale {
    /// `value` (−1 … 1, more when it clips) as a share of half the lane, −1 … 1.
    pub fn apply(self, value: f32) -> f32 {
        let value = if value.is_finite() { value.clamp(-1.0, 1.0) } else { 0.0 };
        match self {
            Self::Linear => value,
            Self::Db => {
                let magnitude = value.abs();
                if magnitude <= 0.0 {
                    return 0.0;
                }
                let db = 20.0 * magnitude.log10();
                ((db + DB_RANGE) / DB_RANGE).clamp(0.0, 1.0).copysign(value)
            }
        }
    }
}

/// One pixel column to draw: the stretch of the file it shows, in seconds, and the gain on it
/// (a clip's gain and fades).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub x: f32,
    pub from: f64,
    pub to: f64,
    pub gain: f32,
}

/// A column of the envelope, in points: the peak span and the RMS span inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub x: f32,
    pub top: f32,
    pub bottom: f32,
    pub rms_top: f32,
    pub rms_bottom: f32,
}

/// What is drawn in one lane.
#[derive(Debug, Clone, PartialEq)]
pub enum LaneShape {
    /// Peaks and RMS per column; runs of columns inside the file, broken where it is not.
    Envelope(Vec<Vec<Span>>),
    /// The samples joined by a line, with a dot on each when they are far enough apart.
    Samples { points: Vec<Pos2>, dots: bool },
}

/// A waveform ready to paint: one shape per lane, and the zero line of each lane.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub lanes: Vec<(f32, LaneShape)>,
}

/// Lays `peaks` out in `rect` over `columns` (left to right, one point apart): lanes, the
/// envelope from the level closest to the zoom, or the samples when a sample is wider than a
/// column.
pub fn geometry(peaks: &Peaks, rect: Rect, columns: &[Column], scale: Scale) -> Geometry {
    let _work_trace = crate::kit::frame_trace::WorkSpan::new("waveform geometry");
    let lanes: Vec<Lane> = if peaks.lanes >= 2 && rect.height() >= STEREO_MIN_HEIGHT {
        (0..peaks.lanes).map(Lane::Channel).collect()
    } else {
        vec![Lane::Mixed]
    };
    let rate = f64::from(peaks.sample_rate);
    let gap = if lanes.len() > 1 { 2.0 } else { 0.0 };
    let height = ((rect.height() - gap * (lanes.len() - 1) as f32) / lanes.len() as f32).max(1.0);
    // Frames a column covers, on average over the columns in the file.
    let widths: Vec<f64> = columns.iter().map(|c| (c.to - c.from).abs() * rate).filter(|w| w.is_finite()).collect();
    let per_column = if widths.is_empty() { 0.0 } else { widths.iter().sum::<f64>() / widths.len() as f64 };
    let close = peaks.has_samples() && per_column > 0.0 && per_column < 1.0;
    let shapes = lanes
        .iter()
        .enumerate()
        .map(|(i, lane)| {
            let top = rect.top() + i as f32 * (height + gap);
            let middle = top + height / 2.0;
            let half = height / 2.0;
            let y = |v: f32, gain: f32| middle - scale.apply(v * gain) * half;
            let shape = if close {
                samples(peaks, *lane, columns, rate, &y)
            } else {
                envelope(peaks, *lane, columns, rate, &y)
            };
            (middle, shape)
        })
        .collect();
    Geometry { lanes: shapes }
}

fn envelope(peaks: &Peaks, lane: Lane, columns: &[Column], rate: f64, y: &dyn Fn(f32, f32) -> f32) -> LaneShape {
    let mut runs: Vec<Vec<Span>> = Vec::new();
    let mut run: Vec<Span> = Vec::new();
    for column in columns {
        match peaks.column(lane, column.from * rate, column.to * rate) {
            Some(b) => {
                let top = y(b.max, column.gain);
                // At least a point tall, so silence shows as a line.
                let bottom = y(b.min, column.gain).max(top + 1.0);
                let rms = b.rms.min(b.max.abs().max(b.min.abs()));
                let rms_top = y(rms, column.gain).clamp(top, bottom);
                let rms_bottom = y(-rms, column.gain).clamp(rms_top, bottom);
                run.push(Span { x: column.x, top, bottom, rms_top, rms_bottom });
            }
            None if !run.is_empty() => runs.push(std::mem::take(&mut run)),
            None => {}
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    LaneShape::Envelope(runs)
}

fn samples(peaks: &Peaks, lane: Lane, columns: &[Column], rate: f64, y: &dyn Fn(f32, f32) -> f32) -> LaneShape {
    let mut points = Vec::new();
    let mut spacing = f32::MAX;
    let mut last_frame = None;
    for column in columns {
        let (from, to) = (column.from * rate, column.to * rate);
        if !(from.is_finite() && to.is_finite()) || to <= from {
            continue;
        }
        // Every sample in this column, placed by its time within the column (one point wide).
        let first = from.ceil().max(0.0) as usize;
        let last = to.ceil().max(0.0) as usize;
        let per_point = (to - from) as f32;
        spacing = spacing.min(1.0 / per_point.max(f32::EPSILON));
        for frame in first..last {
            if last_frame.is_some_and(|l| frame <= l) {
                continue;
            }
            let Some(value) = peaks.sample(lane, frame) else { continue };
            let x = column.x + ((frame as f64 - from) / (to - from)) as f32;
            points.push(Pos2::new(x, y(value, column.gain)));
            last_frame = Some(frame);
        }
    }
    LaneShape::Samples { points, dots: spacing >= DOT_SPACING }
}

/// The colours of a waveform: the peaks, and the RMS band drawn over them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub peak: Color32,
    pub rms: Color32,
}

impl Colors {
    /// From one colour: the peaks lighter (more see-through), the RMS band in the colour itself.
    pub fn from(color: Color32) -> Self {
        Self { peak: color.gamma_multiply(0.55), rms: color }
    }
}

/// Paints `geometry`: each run of the envelope as a filled shape with anti-aliased edges and the
/// RMS band inside it, or the samples as a line with dots.
pub fn paint(painter: &Painter, geometry: &Geometry, colors: Colors) {
    let _work_trace = crate::kit::frame_trace::WorkSpan::new("waveform paint");
    for (_, shape) in &geometry.lanes {
        match shape {
            LaneShape::Envelope(runs) => {
                for run in runs {
                    painter.add(Shape::mesh(band(run, |s| (s.top, s.bottom), colors.peak)));
                    painter.add(Shape::mesh(band(run, |s| (s.rms_top, s.rms_bottom), colors.rms)));
                    if run.len() > 1 {
                        // The edges, drawn as lines so they are smoothed.
                        let edge = PathStroke::new(1.0, colors.peak);
                        let top: Vec<Pos2> = run.iter().map(|s| Pos2::new(s.x, s.top)).collect();
                        let bottom: Vec<Pos2> = run.iter().map(|s| Pos2::new(s.x, s.bottom)).collect();
                        painter.add(Shape::Path(PathShape::line(top, edge.clone())));
                        painter.add(Shape::Path(PathShape::line(bottom, edge)));
                    }
                }
            }
            LaneShape::Samples { points, dots } => {
                if points.len() > 1 {
                    painter.add(Shape::line(points.clone(), Stroke::new(1.0, colors.rms)));
                }
                if *dots {
                    for point in points {
                        painter.circle_filled(*point, 1.5, colors.rms);
                    }
                }
            }
        }
    }
}

/// A strip between two edges of `run`, one quad per pair of columns; a single column is a
/// point-wide bar.
fn band(run: &[Span], edges: impl Fn(&Span) -> (f32, f32), color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    if let [only] = run {
        let (top, bottom) = edges(only);
        mesh.add_colored_rect(Rect::from_x_y_ranges(only.x - 0.5..=only.x + 0.5, top..=bottom), color);
        return mesh;
    }
    mesh.reserve_vertices(run.len() * 2);
    mesh.reserve_triangles(run.len().saturating_sub(1) * 2);
    for (i, span) in run.iter().enumerate() {
        let (top, bottom) = edges(span);
        mesh.colored_vertex(Pos2::new(span.x, top), color);
        mesh.colored_vertex(Pos2::new(span.x, bottom), color);
        if i > 0 {
            let (a, b, c, d) = ((i as u32 - 1) * 2, (i as u32 - 1) * 2 + 1, i as u32 * 2, i as u32 * 2 + 1);
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(c, b, d);
        }
    }
    mesh
}

/// Columns for a stretch of a file shown at a steady zoom: `count` columns from `x`, starting
/// at `from` seconds, `seconds_per_point` each, at `gain`.
pub fn steady_columns(x: f32, count: usize, from: f64, seconds_per_point: f64, gain: f32) -> Vec<Column> {
    (0..count)
        .map(|i| {
            let start = from + i as f64 * seconds_per_point;
            Column { x: x + i as f32 + 0.5, from: start, to: start + seconds_per_point, gain }
        })
        .collect()
}

/// The scale waveforms are drawn with, as the window's settings choose it for this frame.
pub fn scale(ctx: &egui::Context) -> Scale {
    ctx.data(|d| d.get_temp::<Scale>(egui::Id::new("waveform-scale"))).unwrap_or_default()
}

/// Draws waveforms with `scale` from now on.
pub fn set_scale(ctx: &egui::Context, scale: Scale) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("waveform-scale"), scale));
}

#[cfg(test)]
mod tests;
