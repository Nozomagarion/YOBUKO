// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Every built-in effect or instrument by its stable id (ADR 0004): what the model stores as
//! `DeviceKind::Builtin(id)` turns into a running device here. The engine builds device chains
//! through this crate; front ends list what can be added.

use std::sync::{Arc, OnceLock};

use suhina_device_api::{Device, DeviceInfo};
use suhina_device_sdk::DeviceSdkError;
use suhina_device_sdk::builtin::{self, Layout, Panel};

/// The built-in effects, instruments and MIDI effects, in the order a device browser lists them.
pub const DEVICES: [&DeviceInfo; 14] = [
    &suhina_device_fm_synth::INFO,
    &suhina_device_sampler::INFO,
    &suhina_device_utility::INFO,
    &suhina_device_auto_pan::INFO,
    &suhina_device_dj_eq::INFO,
    &suhina_device_vinyl_distortion::INFO,
    &suhina_device_overdrive::INFO,
    &suhina_device_compressor::INFO,
    &suhina_device_limiter::INFO,
    &suhina_device_nam_amp_modeler::INFO,
    &suhina_device_nam_cab_loader::INFO,
    &suhina_device_delay::INFO,
    &suhina_device_reverb::INFO,
    &suhina_device_pitch::INFO,
];

/// What the device with this id is, or `None` for an unknown id.
pub fn info(id: &str) -> Option<&'static DeviceInfo> {
    DEVICES.iter().copied().find(|info| info.id == id)
}

/// The layout of the device with this id, when it has one (ADR 0021 › Amendment: device panels
/// for built-in devices): the `.sd` UI part only, its parameters named by id. A device crate
/// gives it in Rust, or as `devices/<name>/panel.json` listed in [`LAYOUT_FILES`].
pub fn layout(id: &str) -> Option<Result<Layout, DeviceSdkError>> {
    if id == "sampler" {
        return Some(Ok(suhina_device_sampler::layout()));
    }
    LAYOUT_FILES.iter().find(|(file_id, _)| *file_id == id).map(|(_, text)| Layout::from_json(text))
}

/// The layouts kept as JSON next to a device crate, by device id.
pub const LAYOUT_FILES: &[(&str, &str)] = &[
    ("compressor", include_str!("../../../devices/compressor/panel.json")),
    ("overdrive", include_str!("../../../devices/overdrive/panel.json")),
    ("reverb", include_str!("../../../devices/reverb/panel.json")),
    ("limiter", include_str!("../../../devices/limiter/panel.json")),
    ("dj-eq", include_str!("../../../devices/dj-eq/panel.json")),
    ("auto-pan", include_str!("../../../devices/auto-pan/panel.json")),
    ("vinyl-distortion", include_str!("../../../devices/vinyl-distortion/panel.json")),
    ("delay", include_str!("../../../devices/delay/panel.json")),
    ("pitch", include_str!("../../../devices/pitch/panel.json")),
    ("fm-synth", include_str!("../../../devices/fm-synth/panel.json")),
];

/// The panel of the device with this id: its parameters from its `DeviceInfo`, laid out by its
/// layout, or the generated parameter list when it has none or its layout does not load.
/// Made once per id while Suhina runs, off the audio thread. `None` for an unknown id.
pub fn panel(id: &str) -> Option<&'static Panel> {
    /// Every device's panel, in the order of [`DEVICES`].
    static PANELS: OnceLock<Vec<Panel>> = OnceLock::new();
    let index = DEVICES.iter().position(|info| info.id == id)?;
    PANELS
        .get_or_init(|| {
            DEVICES
                .iter()
                .map(|info| {
                    layout(info.id)
                        .and_then(Result::ok)
                        .and_then(|layout| layout.panel(info).ok())
                        .unwrap_or_else(|| builtin::generated(info))
                })
                .collect()
        })
        .get(index)
}

/// A new device with this id at its defaults, prepared for 48 kHz, or `None` for an unknown
/// id. Allocates: call it off the audio thread.
pub fn create(id: &str) -> Option<Box<dyn Device>> {
    Some(match id {
        "sampler" => Box::new(suhina_device_sampler::Sampler::new()),
        "fm-synth" => Box::new(suhina_device_fm_synth::FmSynth::new()),
        "utility" => Box::new(suhina_device_utility::Utility::new()),
        "auto-pan" => Box::new(suhina_device_auto_pan::AutoPan::new()),
        "dj-eq" => Box::new(suhina_device_dj_eq::DjEq::new()),
        "vinyl-distortion" => Box::new(suhina_device_vinyl_distortion::VinylDistortion::new()),
        "overdrive" => Box::new(suhina_device_overdrive::Overdrive::new()),
        "compressor" => Box::new(suhina_device_compressor::Compressor::new()),
        "limiter" => Box::new(suhina_device_limiter::Limiter::new()),
        "nam-amp-modeler" => Box::new(suhina_device_nam_amp_modeler::NamAmp::new()),
        "nam-cab-loader" => Box::new(suhina_device_nam_cab_loader::CabLoader::new()),
        "delay" => Box::new(suhina_device_delay::Delay::new()),
        "reverb" => Box::new(suhina_device_reverb::Reverb::new()),
        "pitch" => Box::new(suhina_device_pitch::Pitch::new()),
        _ => return None,
    })
}

/// The file a file-backed device runs, already read and parsed or decoded (docs/spec/nam-amp-and-cab.md):
/// a model for the Amp Modeler, an impulse response for the Cab Loader. The session keeps a
/// map of these by the file's path; the engine hands the device the map's entry when it builds
/// its chains.
#[derive(Debug, Clone)]
pub enum DeviceAsset {
    Nam(Arc<suhina_device_nam_amp_modeler::NamModel>),
    Ir(Arc<suhina_dsp::Ir>),
}

/// The ids whose devices run a file stored in `Device::state` (docs/spec/nam-amp-and-cab.md).
pub fn is_file_device(id: &str) -> bool {
    matches!(id, "nam-amp-modeler" | "nam-cab-loader")
}

/// A new device with this id, its defaults, and `asset` already loaded — `None` for an id that
/// takes no file, or for an unknown id. Allocation: off the audio thread.
pub fn create_with(id: &str, asset: Option<&DeviceAsset>) -> Option<Box<dyn Device>> {
    match (id, asset) {
        ("nam-amp-modeler", Some(DeviceAsset::Nam(model))) => {
            Some(Box::new(suhina_device_nam_amp_modeler::NamAmp::with_model(Arc::clone(model))))
        }
        ("nam-cab-loader", Some(DeviceAsset::Ir(ir))) => {
            Some(Box::new(suhina_device_nam_cab_loader::CabLoader::with_ir((**ir).clone())))
        }
        _ => create(id),
    }
}

/// Largest preset file read.
pub const MAX_PRESET_BYTES: usize = 64 * 1024;

/// A device's values from a preset file (`docs/spec/browser.md` › Built-in effects and instruments, presets
/// and plug-ins).
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// The device it is for.
    pub device: &'static str,
    /// The preset's own name (a DX7 voice's name).
    pub name: String,
    /// Every parameter by its id, with the preset's value.
    pub params: Vec<(&'static str, f64)>,
}

/// The voices a preset file holds: a DX7 single voice (1) or bank (32). Each is `(number from
/// 1, name)`.
pub fn preset_voices(data: &[u8]) -> Result<Vec<(usize, String)>, String> {
    let voices = read_voices(data)?;
    Ok(voices.patches.iter().enumerate().map(|(i, p)| (i + 1, p.name().trim().to_string())).collect())
}

/// The preset `voice` (from 1) of a preset file: DX7 voice data for the FM synth.
pub fn preset(data: &[u8], voice: usize) -> Result<Preset, String> {
    let voices = read_voices(data)?;
    let count = voices.patches.len();
    let patch = voice
        .checked_sub(1)
        .and_then(|i| voices.patches.get(i))
        .ok_or_else(|| format!("voice {voice} is not in the file; it holds {count}"))?;
    let mut synth = suhina_device_fm_synth::FmSynth::new();
    synth.load_patch(patch);
    let info = &suhina_device_fm_synth::INFO;
    let params =
        info.params.iter().enumerate().filter_map(|(i, p)| synth.param(i).map(|value| (p.id, value))).collect();
    Ok(Preset { device: info.id, name: patch.name().trim().to_string(), params })
}

fn read_voices(data: &[u8]) -> Result<suhina_device_fm_synth::patch::Voices, String> {
    if data.len() > MAX_PRESET_BYTES {
        return Err(format!("a preset file is at most {} KiB", MAX_PRESET_BYTES / 1024));
    }
    suhina_device_fm_synth::patch::read(data).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
