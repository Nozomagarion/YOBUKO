// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure admission before the ordinary setter; remote edits can never select a plugin or file device.
use serde_json::{Value, json};
use suhina_model::DeviceKind;

use crate::{CommandError, Session};

pub(crate) fn set_param(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let track = params["track"]
        .as_u64()
        .filter(|id| *id > 0 && u32::try_from(*id).is_ok())
        .ok_or("shared device track must be a nonzero numeric id")?;
    let index = params["device"]
        .as_u64()
        .filter(|n| *n < 16)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("shared device index must be 0..15")?;
    let param = params["param"]
        .as_str()
        .filter(|s| {
            !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_'))
        })
        .ok_or("invalid shared parameter id")?;
    let value = crate::commands::number(params, "value")?;
    let channel = crate::commands::device_channel_read(session, params)?;
    let device = channel.devices.get(index).ok_or("shared device is missing")?;
    let DeviceKind::Builtin(kind) = &device.kind else {
        return Err("only parameter-only FM Synth and Utility can be edited in a shared session".into());
    };
    if !matches!(kind.as_str(), "fm-synth" | "utility") || device.state.is_some() {
        return Err("only parameter-only FM Synth and Utility can be edited in a shared session".into());
    }
    let info = suhina_device_registry::info(kind).ok_or("shared device metadata is missing")?;
    let spec = info.param_index(param).and_then(|i| info.params.get(i)).ok_or("unknown shared device parameter")?;
    if spec.clamp(value) != value {
        return Err("shared parameter value must be in range and canonical for its type".into());
    }
    if device.params.len() > 512 || (device.params.len() == 512 && !device.params.iter().any(|p| p.id == param)) {
        return Err("too many shared device parameters".into());
    }
    // Only these validated built-ins reach the native fast parameter-message/automation-override path.
    crate::devices::set_param(session, &json!({"track":track,"device":index,"param":param,"value":value}))
}

fn allowed_chain(devices: &[suhina_model::Device]) -> bool {
    devices.len() <= 16
        && devices.iter().all(|d| {
            d.state.is_none()
                && matches!(&d.kind, DeviceKind::Builtin(kind) if matches!(kind.as_str(), "fm-synth" | "utility"))
        })
}
fn track(params: &Value) -> Result<u32, CommandError> {
    params["track"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| "shared device track must be a nonzero numeric id".into())
}
pub(crate) fn add(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let track = track(params)?;
    let kind = params["kind"]
        .as_str()
        .filter(|s| matches!(*s, "fm-synth" | "utility"))
        .ok_or("only FM Synth and Utility can be added to a shared chain")?;
    let channel = crate::commands::device_channel_read(session, params)?;
    if !allowed_chain(&channel.devices) {
        return Err("shared chains support parameter-only FM Synth and Utility".into());
    }
    if channel.devices.len() >= 16 {
        return Err("a shared channel holds at most 16 devices".into());
    }
    let count = session
        .project()
        .channels
        .iter()
        .try_fold(0usize, |sum, c| sum.checked_add(c.devices.len()))
        .ok_or("too many shared devices")?;
    if count >= 128 {
        return Err("a shared project holds at most 128 devices".into());
    }
    if kind == "fm-synth"
        && (!session.project().track(suhina_model::TrackId(track)).is_some_and(|t| t.content.notes)
            || channel.devices.iter().any(|d| matches!(&d.kind,DeviceKind::Builtin(id) if id=="fm-synth")))
    {
        return Err("FM Synth needs a MIDI track without an existing instrument".into());
    }
    // Deliberately pass no preset/path/position: the ordinary native handler stays file-free.
    crate::devices::add(session, &json!({"track":track,"id":kind}))
}
pub(crate) fn remove(session: &mut Session, params: &Value) -> Result<Value, CommandError> {
    let track = track(params)?;
    let index = params["device"]
        .as_u64()
        .filter(|n| *n < 16)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("shared device index must be 0..15")?;
    let channel = crate::commands::device_channel_read(session, params)?;
    if !allowed_chain(&channel.devices) {
        return Err("shared chains support parameter-only FM Synth and Utility".into());
    }
    channel.devices.get(index).ok_or("shared device is missing")?;
    crate::devices::remove(session, &json!({"track":track,"device":index}))
}
