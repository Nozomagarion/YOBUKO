// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which studio edits a multiplayer session shares: the edits a shared tab sends to the
//! session's host, which runs them in order on its copy and on every participant's
//! (`docs/spec/shared-studio.md`).
//!
//! A shared edit must give the same project on every computer and touch nothing but the
//! project: no file, no clipboard, no hardware and no chance. So it is a project edit, it names
//! no file, and it is none of the commands below.

use serde_json::Value;

use crate::{CommandKind, ParamType, commands};

/// Edits that read or write this computer's files, clipboard, hardware or plug-ins, or render
/// audio: they stay out of a shared session, whatever their parameters.
const NOT_SHARED: &[&str] = &[
    // The clipboard is this computer's own.
    "clip.paste",
    "track.paste",
    "device.paste",
    "note.paste",
    "automation.paste",
    // Files: written, collected or pointed at.
    "project.collect",
    "project.relinkFiles",
    "clip.addFiles",
    "clip.consolidate",
    "device.addSd",
    "device.updateSd",
    "device.setNamFile",
    "device.loadSample",
    // Plug-ins are loaded from this computer's own installation.
    "device.addPlugin",
    "clip.setAra",
    // This computer's audio and MIDI inputs and controller mappings.
    "track.setAudioInput",
    "track.setMidiInput",
    "mapping.add",
    "mapping.remove",
];

/// Edits that draw random numbers: shared only with a `seed`, so every computer draws the same.
pub const SEEDED: &[&str] = &["device.randomize", "note.humanize", "note.randomizeVelocity"];

/// Most bytes a shared edit's parameters take, as JSON.
pub const MAX_SHARED_PARAMS: usize = 2048;

/// Whether `id` is an edit a shared tab sends to the session's host. Its parameters are checked
/// by [`shared_studio_params`].
pub fn shared_studio_command(id: &str) -> bool {
    !NOT_SHARED.contains(&id) && commands().iter().any(|spec| spec.id == id && spec.kind == CommandKind::EDIT)
}

/// The registry's own `&'static` id for a shared studio edit.
pub fn shared_studio_id(id: &str) -> Option<&'static str> {
    shared_studio_command(id).then(|| commands().iter().find(|spec| spec.id == id).map(|spec| spec.id)).flatten()
}

/// Checks the parameters of a shared studio edit `id` before anyone runs it: an object of at
/// most [`MAX_SHARED_PARAMS`] bytes, no parameter that names a file or a controller, nothing
/// nested deeper than four levels, and a `seed` for an edit that draws random numbers. The
/// command checks the rest when it runs.
pub fn shared_studio_params(id: &str, params: &Value) -> Result<(), String> {
    let spec = commands()
        .iter()
        .find(|spec| spec.id == id && shared_studio_command(id))
        .ok_or_else(|| format!("{id} is not shared in a multiplayer session yet"))?;
    let object = params.as_object().ok_or("a shared edit's parameters are an object")?;
    let size = serde_json::to_vec(params).map(|bytes| bytes.len()).unwrap_or(usize::MAX);
    if size > MAX_SHARED_PARAMS {
        return Err("this edit is too large to share; edit fewer items at once".into());
    }
    if depth(params) > 4 {
        return Err("a shared edit's parameters are nested too deeply".into());
    }
    for key in object.keys() {
        let Some(param) = spec.params.iter().find(|p| p.name == key) else {
            return Err(format!("{id} has no parameter {key}"));
        };
        if matches!(param.ty, ParamType::Path(_) | ParamType::Texts | ParamType::MappingTarget) {
            return Err(format!("{id} with a file ({key}) is not shared in a multiplayer session"));
        }
    }
    if SEEDED.contains(&id) && !object.contains_key("seed") {
        return Err(format!("{id} needs a seed in a multiplayer session"));
    }
    Ok(())
}

fn depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Value::Object(map) => 1 + map.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
#[path = "shared_studio_tests.rs"]
mod tests;
