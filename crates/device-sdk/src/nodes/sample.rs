// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sample source adapter; DSP remains in the shared sample instrument kernel.
use super::NodeInfo;
use crate::native_node::NativeNode;
use crate::{DeviceSdkError, NodeKind, PreparedEffect, ProcessContext};
use std::any::Any;
use suhina_device_api::{Device, NoteEvent};
pub(crate) const SAMPLER: NodeInfo = NodeInfo {
    kind: "sampler",
    name: "Sampler",
    rust_variant: "Sampler",
    inputs: &[],
    outputs: &["out", "left", "right"],
    description: "Polyphonic sample source using the shared Sampler kernel. Receives timestamped instrument events; bind sample data and parameters by node id before processing. Audio can feed any signal node.",
    configuration: "none; source and controls are bound through PreparedEffect",
    example: r#"{"kind":"sampler"}"#,
};
pub(crate) fn prepare(kind: &NodeKind, rate: f64) -> Result<Option<Box<dyn NativeNode>>, DeviceSdkError> {
    match kind {
        NodeKind::Sampler => {
            let mut kernel = Box::new(crate::sampler::Sampler::new());
            kernel.prepare(rate).map_err(|e| DeviceSdkError(e.to_string()))?;
            Ok(Some(kernel))
        }
        _ => Ok(None),
    }
}

impl NativeNode for crate::sampler::Sampler {
    fn prepare_block(&mut self, context: &ProcessContext) {
        self.prepare_block(context);
    }
    fn process(&mut self, _inputs: [&[f32]; 5], buffer: &mut [f32]) {
        self.process_prepared(buffer);
    }
    fn event(&mut self, event: NoteEvent) {
        Device::event(self, event);
    }
    fn reset(&mut self) {
        Device::reset(self);
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl PreparedEffect {
    /// Bind a sample source off the audio thread. Replacement may release old resources.
    pub fn sampler_mut(&mut self, node: &str) -> Option<&mut crate::sampler::Sampler> {
        self.native_node_mut(node)?.as_any_mut().downcast_mut()
    }
}
