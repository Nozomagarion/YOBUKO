// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{NodeInfo, PortInfo};

pub(crate) const GRAIN_WINDOW: NodeInfo = NodeInfo {
    kind: "grain_window",
    name: "GrainWindow",
    rust_variant: "GrainWindow",
    inputs: &[
        PortInfo { name: "position", default: Some(0.0), unit: "signal" },
        PortInfo { name: "duration", default: Some(1.0), unit: "signal" },
    ],
    outputs: &["out", "left", "right"],
    description: "Hann grain lifetime window, zero outside duration.",
    configuration: "none",
    example: r#"{"kind":"grain_window"}"#,
};
fn grain_window(x: [f64; 5]) -> f64 {
    {
        if x[1] > 0. && x[0] >= 0. && x[0] < x[1] {
            0.5 - 0.5 * (std::f64::consts::TAU * x[0] / x[1]).cos()
        } else {
            0.
        }
    }
}

pub(crate) fn prepare(kind: &super::NodeKind) -> Option<Box<dyn crate::native_node::NativeNode>> {
    match kind {
        super::NodeKind::GrainWindow => Some(super::processor::boxed(grain_window)),
        _ => None,
    }
}
