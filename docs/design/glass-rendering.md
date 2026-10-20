---
description: Initial glass rendering investigation and an unintegrated GPU blur shader prototype
read_when: glass effect, backdrop blur, rendering materials, Aero
type: plan
status: draft
last_verified: 2026-10-20
covers:
  - crates/ui/src/kit/**
  - apps/suhina/src/main.rs
  - apps/suhina/**
---

# Glass rendering experiment

## Current state

The application creates eframe with either wgpu or OpenGL. Its Window::ui delegates to
App::ui and does not own the final framebuffer. There is no custom render callback or
offscreen backdrop path in the UI crate. Painting translucent rectangles alone will not
blur previously drawn controls. The current build contains no glass effect.

The two-pass shader in `crates/ui/src/kit/glass_blur.wgsl` now runs in the standalone
`glass_preview` native example. It uploads a fixed synthetic backdrop once, keeps three
768 by 432 RGBA textures and re-runs the horizontal/vertical GPU blur only when Blur changes.
Controls are drawn sharply over the material; Tint and a solid fallback can be compared.
The example built and its native window started on Windows with no GPU validation error
reported in stderr. Image-level correctness, performance measurements and live DAW backdrop
integration are still outstanding. The main application does not load this shader yet.

Run `cargo run -p suhina --example glass_preview` from the worktree. No new dependencies were added.
The preview uses fixed-size gamma-space RGBA textures to match egui native texture support;
it does not yet establish physically correct linear-space or translucent-source handling.

## Integration requirements

1. Render the content behind the glass into a separate texture before compositing chrome.
   Decide which regions are behind each panel; simply blurring the previous screen frame
   causes stale content and feedback.
2. Downsample that backdrop and blur into two reusable textures, never reading and writing
   the same texture in one pass. Use a linear clamp sampler. Expand each captured region
   by the kernel footprint so edges do not develop seams.
3. Composite the blurred backdrop through the panel mask, then apply theme tint, opacity
   and a restrained highlight. Draw text and controls afterward, sharply.
4. Reuse allocations while size and renderer stay the same. Resize/recreate only off the
   audio thread. Do not use a per-frame CPU screenshot/readback path.
5. Start opt-in on stationary chrome with bounded affected area. OpenGL initially uses a
   solid fallback; implement its blur only after the wgpu experiment is measured.
6. Keep material settings independent from color themes and layout. Missing settings mean
   solid materials. Reduced effects/high contrast must force a readable solid fallback.

## Verification gate

Validate WGSL and test constant colors, an impulse and a checkerboard on the GPU. Check
linear color space and premultiplied alpha, resizing, high DPI and stale-frame artifacts.
Measure frame time during playback, scrolling and zooming against the solid baseline.
Only then connect the effect to a user-facing Appearance setting and build a preview.

## Smooth versus stepped experiment

The smooth mode uses a bounded 49-sample Gaussian in each direction (sigma at most 8).
This avoids the sparse-sample ghost bands of the earlier large-radius experiment. A separate
Stepped texture toggle retains the five-sample sparse kernel as an explicit artistic option.
Both modes run only on a blur/mode change in the static preview. The denser kernel needs GPU
measurement before use on a live UI; no frame-rate claim has been established.
