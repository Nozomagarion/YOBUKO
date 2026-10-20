// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Experimental, current-frame modal backdrop. Appearance strengths default to zero.
//! Own offscreen renderer and targets: never sample the swapchain or a previous UI frame.

use std::sync::Mutex;

use eframe::{egui, egui_wgpu, wgpu};
use suhina_ui::kit::liquid::{Backdrop, Provider};
use wgpu::util::DeviceExt;

const MAX_SIDE: f32 = 1024.0;

pub struct Liquid {
    gpu: egui_wgpu::RenderState,
    state: Mutex<State>,
}

struct State {
    renderer: egui_wgpu::Renderer,
    targets: Option<Targets>,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    font: Option<egui::ColorImage>,
}

struct Targets {
    size: [u32; 2],
    source: wgpu::Texture,
    scratch: wgpu::Texture,
    output: wgpu::Texture,
    image: egui::TextureId,
    horizontal: wgpu::BindGroup,
    vertical: wgpu::BindGroup,
}

impl Liquid {
    pub fn new(gpu: egui_wgpu::RenderState) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("liquid linear Gaussian"),
            source: wgpu::ShaderSource::Wgsl(include_str!("liquid.wgsl").into()),
        });
        let pipeline = |format, entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("liquid blur"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let state = State {
            renderer: egui_wgpu::Renderer::new(device, wgpu::TextureFormat::Rgba8UnormSrgb, Default::default()),
            targets: None,
            horizontal: pipeline(wgpu::TextureFormat::Rgba8UnormSrgb, "linear_blur"),
            vertical: pipeline(wgpu::TextureFormat::Rgba8Unorm, "encoded_blur"),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("liquid clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            font: None,
        };
        Self { gpu, state: Mutex::new(state) }
    }
}

fn target_size(screen: egui::Rect, scale: f32) -> Option<[u32; 2]> {
    if !screen.is_finite() || screen.width() <= 0.0 || screen.height() <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let physical = screen.size() * scale;
    if !physical.is_finite() {
        return None;
    }
    let factor = (MAX_SIDE / physical.max_elem()).min(1.0);
    Some([(physical.x * factor).ceil().max(1.0) as u32, (physical.y * factor).ceil().max(1.0) as u32])
}

impl Targets {
    fn new(gpu: &egui_wgpu::RenderState, state: &State, size: [u32; 2], previous: Option<egui::TextureId>) -> Self {
        let device = &gpu.device;
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let source = texture("liquid current UI", wgpu::TextureFormat::Rgba8UnormSrgb);
        let scratch = texture("liquid horizontal", wgpu::TextureFormat::Rgba8UnormSrgb);
        let output = texture("liquid encoded output", wgpu::TextureFormat::Rgba8Unorm);
        let bindings = |pipeline: &wgpu::RenderPipeline, source: &wgpu::Texture, step: [f32; 4]| {
            let bytes: Vec<_> = step.into_iter().flat_map(f32::to_ne_bytes).collect();
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("liquid blur step"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("liquid blur inputs"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source.create_view(&Default::default())),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&state.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                ],
            })
        };
        let horizontal = bindings(&state.horizontal, &source, [1.0 / size[0] as f32, 0.0, 3.0, 0.0]);
        let vertical = bindings(&state.vertical, &scratch, [0.0, 1.0 / size[1] as f32, 3.0, 0.0]);
        let view = output.create_view(&Default::default());
        let mut renderer = gpu.renderer.write();
        let image = if let Some(id) = previous {
            renderer.update_egui_texture_from_wgpu_texture(device, &view, wgpu::FilterMode::Linear, id);
            id
        } else {
            renderer.register_native_texture(device, &view, wgpu::FilterMode::Linear)
        };
        Self { size, source, scratch, output, image, horizontal, vertical }
    }
}

impl Provider for Liquid {
    fn capture(&self, ctx: &egui::Context) -> Option<Backdrop> {
        let screen = ctx.content_rect();
        let size = target_size(screen, ctx.pixels_per_point())?;
        let mut state = self.state.lock().ok()?;
        if state.targets.as_ref().is_none_or(|targets| targets.size != size) {
            let previous = state.targets.as_ref().map(|targets| targets.image);
            state.targets = Some(Targets::new(&self.gpu, &state, size, previous));
        }
        // Read painted shapes at this point in this frame, before the modal layer exists.
        // Callback-based foreign rendering cannot safely be replayed into our target.
        let order: Vec<_> = ctx.memory(|memory| memory.layer_ids().collect());
        let transforms =
            order.iter().filter_map(|layer| ctx.layer_transform_to_global(*layer).map(|t| (*layer, t))).collect();
        let shapes = ctx.graphics(|layers| layers.clone().drain(&order, &transforms));
        let mut jobs = ctx.tessellate(shapes, ctx.pixels_per_point());
        let device = &self.gpu.device;
        let queue = &self.gpu.queue;
        let font = ctx.fonts(|fonts| fonts.image());
        if state.font.as_ref() != Some(&font) {
            state.renderer.update_texture(
                device,
                queue,
                egui::TextureId::Managed(0),
                &egui::epaint::ImageDelta::full(font.clone(), Default::default()),
            );
            state.font = Some(font);
        }
        // The UI's atlas covers text and vector meshes. An unsupported image/callback forces
        // a solid fallback instead of silently dropping live content from the backdrop.
        if jobs.iter().any(|job| match &job.primitive {
            egui::epaint::Primitive::Mesh(mesh) => mesh.texture_id != egui::TextureId::Managed(0),
            egui::epaint::Primitive::Callback(_) => true,
        }) {
            return None;
        }
        let scale = size[0] as f32 / screen.width();
        // Content rect coordinates are in points; translate to the offscreen origin.
        for job in &mut jobs {
            job.clip_rect = job.clip_rect.translate(-screen.min.to_vec2());
            if let egui::epaint::Primitive::Mesh(mesh) = &mut job.primitive {
                for vertex in &mut mesh.vertices {
                    vertex.pos -= screen.min.to_vec2();
                }
            }
        }
        let descriptor = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: scale };
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands = state.renderer.update_buffers(device, queue, &mut encoder, &jobs, &descriptor);
        let targets = state.targets.as_ref()?;
        {
            let view = targets.source.create_view(&Default::default());
            let background = egui::Rgba::from(suhina_ui::kit::theme::window_background()).to_array();
            let mut color = attachment(&view);
            color.ops.load = wgpu::LoadOp::Clear(wgpu::Color {
                r: f64::from(background[0]),
                g: f64::from(background[1]),
                b: f64::from(background[2]),
                a: 1.0,
            });
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("liquid live UI"),
                    color_attachments: &[Some(color)],
                    ..Default::default()
                })
                .forget_lifetime();
            state.renderer.render(&mut pass, &jobs, &descriptor);
        }
        for (target, pipeline, bindings) in [
            (&targets.scratch, &state.horizontal, &targets.horizontal),
            (&targets.output, &state.vertical, &targets.vertical),
        ] {
            let view = target.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("liquid linear blur"),
                color_attachments: &[Some(attachment(&view))],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bindings, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        Some(Backdrop { texture: targets.image, screen })
    }
}

fn attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait<F: std::future::Future>(future: F) -> F::Output {
        struct Wake(std::thread::Thread);
        impl std::task::Wake for Wake {
            fn wake(self: std::sync::Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &std::sync::Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
        let mut cx = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::park_timeout(std::time::Duration::from_millis(100));
        }
    }

    fn pixels(liquid: &Liquid) -> Vec<u8> {
        let state = liquid.state.lock().unwrap();
        let targets = state.targets.as_ref().unwrap();
        let row = (targets.size[0] * 4).div_ceil(256) * 256;
        let buffer = liquid.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test-only liquid readback"),
            size: u64::from(row) * u64::from(targets.size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = liquid.gpu.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            targets.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            targets.output.size(),
        );
        liquid.gpu.queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        liquid.gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let result =
            mapped.chunks(row as usize).flat_map(|row| row[..targets.size[0] as usize * 4].iter().copied()).collect();
        drop(mapped);
        buffer.unmap();
        result
    }

    #[test]
    #[ignore = "requires a native GPU; run explicitly for rendering changes"]
    fn gpu_preserves_constants_blurs_in_linear_light_and_updates_current_frame() {
        let instance = wgpu::Instance::default();
        let gpu =
            wait(egui_wgpu::RenderState::create(&Default::default(), &instance, None, Default::default())).unwrap();
        let liquid = Liquid::new(gpu);
        let ctx = egui::Context::default();
        let draw = |size: f32, pattern: u8| {
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size, size));
            let mut captured = None;
            let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(area), ..Default::default() }, |ui| {
                let ctx = ui.ctx();
                let painter = ctx.layer_painter(egui::LayerId::background());
                let color = match pattern {
                    0 => egui::Color32::from_rgb(100, 150, 200),
                    3 => egui::Color32::BLUE,
                    _ => egui::Color32::BLACK,
                };
                painter.rect_filled(area, 0.0, color);
                let mut pixels = egui::Mesh::default();
                if pattern == 1 {
                    for y in 0..size as u32 {
                        for x in 0..size as u32 {
                            if (x + y) % 2 == 0 {
                                pixels.add_colored_rect(
                                    egui::Rect::from_min_size(egui::pos2(x as f32, y as f32), egui::Vec2::splat(1.0)),
                                    egui::Color32::WHITE,
                                );
                            }
                        }
                    }
                } else if pattern == 2 {
                    pixels.add_colored_rect(
                        egui::Rect::from_min_size(egui::pos2(32.0, 32.0), egui::Vec2::splat(1.0)),
                        egui::Color32::WHITE,
                    );
                }
                painter.add(pixels);
                captured = liquid.capture(ctx);
            });
            // This test owns the font upload; there is no eframe integration consuming deltas.
            output.textures_delta.clear();
            captured.unwrap()
        };
        let first = draw(64.0, 0);
        let constant = pixels(&liquid);
        let center = (32 * 64 + 32) * 4;
        for (actual, expected) in constant[center..center + 4].iter().zip([100_u8, 150, 200, 255]) {
            assert!(actual.abs_diff(expected) <= 3, "constant colour changed: {actual} vs {expected}");
        }
        draw(64.0, 1);
        let checker = pixels(&liquid);
        assert!(
            (178..=198).contains(&checker[center]),
            "linear-light mean should be near 188, got {}",
            checker[center]
        );
        draw(64.0, 2);
        let impulse = pixels(&liquid);
        assert!(impulse[center] > 0 && impulse[center] < 100);
        assert!(impulse[center - 8].abs_diff(impulse[center + 8]) <= 3);
        let resized = draw(128.0, 3);
        assert_eq!(first.texture, resized.texture);
        let blue = pixels(&liquid);
        let center = (64 * 128 + 64) * 4;
        assert!(blue[center] < 3 && blue[center + 2] > 250, "old frame leaked into resized output");
    }

    #[test]
    fn backdrop_size_is_bounded_and_handles_dpi() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
        assert_eq!(target_size(screen, 2.0), Some([1024, 576]));
        assert_eq!(target_size(screen, f32::NAN), None);
        assert_eq!(target_size(egui::Rect::NOTHING, 1.0), None);
    }
}
