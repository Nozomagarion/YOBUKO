// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Contracts for trusted host kernels and whole-device wrappers.
use super::{NodeInfo, PortInfo};

pub(super) const NATIVE_EFFECT: NodeInfo = NodeInfo {
    kind: "native_effect",
    name: "Native effect",
    rust_variant: "NativeEffect",
    inputs: &[
        PortInfo { name: "in", default: Some(0.), unit: "signal" },
        PortInfo { name: "control1", default: Some(0.), unit: "signal" },
        PortInfo { name: "control2", default: Some(0.), unit: "signal" },
        PortInfo { name: "control3", default: Some(0.), unit: "signal" },
        PortInfo { name: "control4", default: Some(0.), unit: "signal" },
    ],
    outputs: &["out", "left", "right"],
    description: "Trusted native graph kernel, resolved off-thread by the host. Supports graph wiring and delayed feedback; never loads third-party code.",
    configuration: "effect: registered kernel id, 1..128 bytes",
    example: r#"{"kind":"native_effect","effect":"org.example.gain"}"#,
};

pub(super) const NATIVE_DEVICE: NodeInfo = NodeInfo {
    kind: "native_device",
    name: "Native device",
    rust_variant: "NativeDevice",
    inputs: &[PortInfo { name: "in", default: Some(0.), unit: "signal" }],
    outputs: &["out", "left", "right"],
    description: "Whole-device native wrapper. Effects wire audio_input -> native_device -> audio_output; instruments wire native_device -> audio_output. Requires the host's registered factory via prepare_native_at; no arbitrary code or feedback. Parameters preserve the native contract. Processing/reset/events use Device's real-time-safe API.",
    configuration: "device: stable registered device id, 1..128 bytes",
    example: r#"{"kind":"native_device","device":"utility"}"#,
};
