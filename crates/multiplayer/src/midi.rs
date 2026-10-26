// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A narrow, file-free native project document, never a general remote project import.

use serde_json::Value;
use suhina_model::{AudioInput, DeviceKind, Extensions, LaneData, MidiInput, MidiPort, Monitor, Project};

pub const MAX_DOCUMENT: usize = 2 * 1024 * 1024;
pub const MAX_TRACKS: usize = 64;
// Upstream source integration must explicitly review this adapter when the native format changes.
const NATIVE_VERSION: u64 = 3;

fn input() -> MidiInput {
    MidiInput { port: MidiPort::Nothing, channel: None, monitor: Monitor::Off }
}
fn audio_input() -> AudioInput {
    AudioInput { monitor: Monitor::Off, ..Default::default() }
}

/// Cheap structural admission before cloning a local project or constructing its document.
pub fn validate(project: &Project) -> Result<(), String> {
    validate_inner(project, false, false)
}
pub fn validate_audio_source(project: &Project) -> Result<(), String> {
    validate_inner(project, true, false)
}
pub fn validate_portable(project: &Project) -> Result<(), String> {
    validate_inner(project, true, true)
}
fn validate_inner(project: &Project, audio: bool, portable: bool) -> Result<(), String> {
    if project.tracks.is_empty()
        || project.tracks.len() > MAX_TRACKS
        || project.channels.len() > 128
        || project.contents.len() > 512
        || project.arrangement.lanes.len() > 512
        || project.launcher.scenes.len() > 128
        || project.launcher.columns.len() > MAX_TRACKS
    {
        return Err("MIDI sharing supports 1–64 tracks, 128 channels and 512 contents".into());
    }
    let mut notes = 0usize;
    let mut clips = 0usize;
    for track in &project.tracks {
        if track.name.trim().is_empty() || track.name.len() > 256 || track.name.chars().any(char::is_control) {
            return Err("shared track names must have 1–256 bytes".into());
        }
        if track.content.audio && !audio {
            return Err("this project has audio tracks; audio-file sharing comes later".into());
        }
        if track.channel.is_none() {
            return Err("each shared track needs a mixer channel".into());
        }
    }
    for content in &project.contents {
        if content.lanes.len() > 64 {
            return Err("too many lanes in a MIDI content".into());
        }
        for lane in &content.lanes {
            match &lane.data {
                LaneData::Notes(items) => notes = notes.checked_add(items.len()).ok_or("too many notes")?,
                LaneData::Audio(region) => {
                    if !audio {
                        return Err("this project contains audio files; use Share project + audio".into());
                    }
                    if region.stream.is_some()
                        || portable
                            && (region.absolute_file.is_some()
                                || !region.file.is_empty() && !crate::assets::valid_path(&region.file))
                    {
                        return Err(
                            "shared audio needs portable manifest references and cannot contain streamed stems".into(),
                        );
                    }
                }
            }
        }
    }
    for lane in &project.arrangement.lanes {
        clips = clips.checked_add(lane.clips.len()).ok_or("too many clips")?;
    }
    for column in &project.launcher.columns {
        if column.slots.len() > 128 {
            return Err("too many launcher slots".into());
        }
        clips = clips.checked_add(column.slots.iter().filter(|s| s.clip.is_some()).count()).ok_or("too many clips")?;
    }
    if notes > 16384 || clips > 4096 {
        return Err("MIDI sharing is limited to 16384 notes and 4096 clips".into());
    }
    let mut devices = 0usize;
    for channel in &project.channels {
        if !channel.volume.is_finite()
            || !(0.0..=10.0_f64.powf(6.0 / 20.0)).contains(&channel.volume)
            || !channel.pan.is_finite()
            || !(-1.0..=1.0).contains(&channel.pan)
            || channel.sends.iter().any(|s| !s.level.is_finite() || !(0.0..=1.0).contains(&s.level))
        {
            return Err("MIDI sharing supports mixer levels up to +6 dB, pan -1..1 and send levels 0..1".into());
        }
        devices = devices.checked_add(channel.devices.len()).ok_or("too many devices")?;
        if devices > 128 {
            return Err("MIDI sharing supports at most 128 built-in devices".into());
        }
        if channel.devices.len() > 16 || channel.sends.len() > 128 {
            return Err("too many devices or sends".into());
        }
        for device in &channel.devices {
            if !matches!(&device.kind,DeviceKind::Builtin(id) if matches!(id.as_str(),"fm-synth"|"utility"))
                || device.state.is_some()
            {
                return Err("MIDI sharing currently supports FM Synth and Utility parameters only; audio, plugins and other devices must wait".into());
            }
            if device.params.len() > 512 || device.automation.len() > 512 {
                return Err("too many device parameters or automation lanes".into());
            }
        }
        if channel.automation.len() > 512 {
            return Err("too many automation lanes".into());
        }
    }
    project.validate().map_err(|_| "invalid source MIDI project".to_string())?;
    Ok(())
}

#[cfg(test)]
pub fn encode(project: &Project) -> Result<(Project, Vec<u8>), String> {
    encode_inner(project, false)
}
pub fn encode_portable(project: &Project) -> Result<(Project, Vec<u8>), String> {
    encode_inner(project, true)
}
fn encode_inner(project: &Project, audio: bool) -> Result<(Project, Vec<u8>), String> {
    if audio {
        validate_portable(project)?;
    } else {
        validate(project)?;
    }
    let mut copy = project.clone();
    copy.extensions = Extensions::default();
    copy.launcher.mappings.clear();
    for track in &mut copy.tracks {
        track.extensions = Extensions::default();
        track.midi_input = input();
        track.audio_input = audio_input();
    }
    for channel in &mut copy.channels {
        channel.extensions = Extensions::default();
        channel.cue = false;
        for device in &mut channel.devices {
            device.extensions = Extensions::default();
        }
    }
    for content in &mut copy.contents {
        content.extensions = Extensions::default();
    }
    for lane in &mut copy.arrangement.lanes {
        for clip in &mut lane.clips {
            clip.extensions = Extensions::default();
        }
    }
    for column in &mut copy.launcher.columns {
        for slot in &mut column.slots {
            if let Some(clip) = &mut slot.clip {
                clip.extensions = Extensions::default();
            }
        }
    }
    copy.assign_ids().map_err(|e| e.to_string())?;
    let text = suhina_project_file::to_json(&copy);
    if text.len() > MAX_DOCUMENT {
        return Err("the MIDI copy is larger than 2 MiB".into());
    }
    // The sender obeys the same boundary as the receiver before it starts a listener.
    let parsed = decode_inner(text.as_bytes(), audio)?;
    Ok((parsed, text.into_bytes()))
}

fn tree(value: &Value, depth: usize, nodes: &mut usize, audio: bool) -> Result<(), String> {
    *nodes = nodes.checked_add(1).ok_or("MIDI document too complex")?;
    if depth > 32 || *nodes > 250000 {
        return Err("MIDI document too complex".into());
    }
    match value {
        Value::String(text) if text.len() > 1024 => return Err("MIDI document text is too long".into()),
        Value::Array(items) => {
            if items.len() > 16384 {
                return Err("MIDI document collection too large".into());
            }
            for item in items {
                tree(item, depth + 1, nodes, audio)?;
            }
        }
        Value::Object(items) => {
            if items.len() > 64 {
                return Err("MIDI document object too large".into());
            }
            for (key, item) in items {
                if key.len() > 64
                    || (matches!(key.as_str(), "extensions" | "mappings")
                        || key == "state" && !item.is_null()
                        || key == "absolute_file" && !(audio && item.is_null())
                        || key == "file"
                            && !(audio && item.as_str().is_some_and(|p| p.is_empty() || crate::assets::valid_path(p))))
                {
                    return Err("MIDI copy contains unsupported metadata, state or file references".into());
                }
                // Bounds precede the native model reader and its reference validation.
                let limit = match key.as_str() {
                    "tracks" => 64,
                    "channels" => 128,
                    "contents" => 512,
                    "lanes" => 512,
                    "devices" => 16,
                    "scenes" | "slots" => 128,
                    _ => 16384,
                };
                if item.as_array().is_some_and(|a| a.len() > limit) {
                    return Err("MIDI document collection too large".into());
                }
                tree(item, depth + 1, nodes, audio)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
pub fn decode(bytes: &[u8]) -> Result<Project, String> {
    decode_inner(bytes, false)
}
pub fn decode_portable(bytes: &[u8]) -> Result<Project, String> {
    decode_inner(bytes, true)
}
fn decode_inner(bytes: &[u8], audio: bool) -> Result<Project, String> {
    if bytes.is_empty() || bytes.len() > MAX_DOCUMENT {
        return Err("invalid MIDI document size".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "MIDI document is not UTF-8")?;
    let value: Value = serde_json::from_str(text).map_err(|_| "invalid MIDI document JSON")?;
    tree(&value, 0, &mut 0, audio)?;
    if suhina_project_file::VERSION != NATIVE_VERSION || value["version"] != NATIVE_VERSION {
        return Err("unsupported native MIDI document version".into());
    }
    let project = suhina_project_file::from_json(text).map_err(|_| "invalid MIDI project")?;
    validate_inner(&project, audio, audio)?;
    if project.tracks.iter().any(|t| t.midi_input != input() || t.audio_input != audio_input())
        || project.channels.iter().any(|c| c.cue)
        || suhina_project_file::to_json(&project).as_bytes() != bytes
    {
        return Err("MIDI document is not the canonical, input-disabled sharing format".into());
    }
    Ok(project)
}

#[cfg(test)]
#[path = "midi_tests.rs"]
mod tests;
