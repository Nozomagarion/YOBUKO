// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native graph kernel contract and off-thread factory.
use crate::{DeviceSdkError, NodeKind, PreparedEffect, ProcessContext};
use std::any::Any;
use suhina_device_api::{Device, NoteEvent};

/// Prepared native stereo graph kernel. All methods except resource access run without allocation,
/// locking or I/O. Construction and disposal belong off the audio thread.
pub trait NativeNode: Send {
    fn prepare_block(&mut self, context: &ProcessContext);
    fn process(&mut self, inputs: [&[f32]; 5], buffer: &mut [f32]);
    fn event(&mut self, event: NoteEvent);
    fn reset(&mut self);
    /// Host resource binding only; never call in an audio callback.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

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

impl crate::DeviceGraph {
    /// Prepare an audio graph with trusted host kernels. The resolver runs only here, off-thread;
    /// it must return a fresh, sample-rate-prepared kernel or reject an unknown identifier.
    /// This API does not load plug-ins or change the whole-device native wrapper contract.
    pub fn compile_with_native_at(
        &self,
        sample_rate: f64,
        mut resolve: impl FnMut(&str, f64) -> Result<Box<dyn NativeNode>, DeviceSdkError>,
    ) -> Result<PreparedEffect, DeviceSdkError> {
        self.validate()?;
        if self.role == crate::DeviceRole::MidiEffect {
            return Err(DeviceSdkError("native kernels require an audio graph".into()));
        }
        PreparedEffect::new_with_native(self, sample_rate, &mut resolve)
    }
}
