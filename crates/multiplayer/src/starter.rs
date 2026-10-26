// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use suhina_model::{
    Clip, ClipContent, ContentId, ContentKinds, ContentLane, Extensions, LaneData, LoopRange, Note, Project,
};

/// Self-authored, deterministic music, generated on each machine without sharing any file.
pub fn starter_project() -> Project {
    let mut project = suhina_session::blank_project();
    project.transport.tempo_bpm = 120.0;
    project.transport.loop_range = Some(LoopRange { start: 0.0, end: 64.0 });
    project.transport.loop_enabled = true;
    for (index, (name, root)) in [("Pulse", 48u8), ("Bass", 36), ("Chords", 60), ("Lead", 72)].into_iter().enumerate() {
        if let Some(track) = project.tracks.get_mut(index) {
            track.name = name.into();
            track.content = ContentKinds { notes: true, audio: false };
        }
        let content = ContentId(1000 + index as u32);
        let notes = (0..4)
            .map(|beat| Note {
                time: beat as f64,
                duration: 0.35,
                key: root + [0, 7, 3, 5][beat as usize],
                velocity: 0.5,
                release_velocity: 0.5,
                channel: 0,
                muted: false,
            })
            .collect();
        project.contents.push(ClipContent {
            id: content,
            name: name.into(),
            length: 4.0,
            lanes: vec![ContentLane { target: None, data: LaneData::Notes(notes) }],
            extensions: Extensions::default(),
            scale: None,
        });
        if let Some(lane) = project.arrangement.lanes.get_mut(index) {
            lane.name = name.into();
            lane.clips.push(Clip {
                content,
                start: 0.0,
                length: 64.0,
                loop_range: Some(LoopRange { start: 0.0, end: 4.0 }),
                ..Clip::new(content, 0.0, 64.0)
            });
        }
        if let Some(channel) = project.channels.get_mut(index) {
            channel.volume = 0.2;
            channel.name = name.into();
        }
    }
    let _ = project.assign_ids();
    project
}
