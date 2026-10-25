// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which take a clip on a track was comped from (#989). A part comped onto the track plays its
//! take's content from the same place, so it is found by its content and by where in it each
//! beat lands: the passes of an audio take are clips of one file at different offsets, and each
//! MIDI pass has a content of its own. No id is stored; a clip that is trimmed keeps its take, as
//! its offset moves with its start.

use crate::{Clip, Project, TrackId};

/// How far apart, in beats, two clips' alignments with their content may be and still be one.
const ALIGNED: f64 = 1e-6;

impl Project {
    /// The index in `arrangement.takes` of the take of `track` that `clip` was comped from.
    pub fn comp_source(&self, track: TrackId, clip: &Clip) -> Option<usize> {
        let end = clip.start + clip.length;
        let aligned = clip.offset - clip.start;
        self.arrangement.takes.iter().position(|take| {
            take.track == track
                && take.clips.iter().any(|t| {
                    t.content == clip.content
                        && (t.offset - t.start - aligned).abs() < ALIGNED
                        && t.start < end
                        && clip.start < t.start + t.length
                })
        })
    }

    /// The spans (start, end in beats) of take `index` that are on its track's lane: the parts of
    /// it the comp plays.
    pub fn comp_used(&self, index: usize) -> Vec<(f64, f64)> {
        let Some(take) = self.arrangement.takes.get(index) else { return Vec::new() };
        let Some(lane) = self.arrangement.lanes.iter().find(|l| l.track == Some(take.track)) else {
            return Vec::new();
        };
        lane.clips
            .iter()
            .filter(|c| self.comp_source(take.track, c) == Some(index))
            .map(|c| (c.start, c.start + c.length))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::{ArrangementLane, Clip, ContentId, LaneId, Project, TakeLane, TrackId};

    #[test]
    fn a_comped_part_is_found_by_its_content_and_alignment() {
        let track = TrackId(1);
        let mut p = Project::default();
        let pass = |offset: f64| Clip { offset, ..Clip::new(ContentId(5), 0.0, 8.0) };
        p.arrangement.takes = vec![
            TakeLane { track, name: "Take 1".into(), clips: vec![pass(0.0)], color: None },
            TakeLane { track, name: "Take 2".into(), clips: vec![pass(8.0)], color: None },
        ];
        // Beats 2 to 4 of the second pass: the file from 10.
        let part = Clip { offset: 10.0, ..Clip::new(ContentId(5), 2.0, 2.0) };
        let other = Clip::new(ContentId(9), 4.0, 2.0);
        p.arrangement.lanes = vec![ArrangementLane {
            id: LaneId(1),
            name: String::new(),
            track: Some(track),
            clips: vec![part.clone(), other.clone()],
        }];
        assert_eq!(p.comp_source(track, &part), Some(1));
        assert_eq!(p.comp_source(track, &other), None);
        assert_eq!(p.comp_used(1), [(2.0, 4.0)]);
        assert!(p.comp_used(0).is_empty());
    }
}
