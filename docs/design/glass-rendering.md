---
description: Experimental live modal backdrop and optical coatings for track and clip surfaces
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

The original static experiment below remains available. There is also an experimental
native integration, disabled by default, with per-surface strengths under
Appearance › Liquid (experimental).

Run with `--renderer wgpu` and raise a strength above 0 to opt in. The old
`SUHINA_LIQUID_PREVIEW` environment switch is no longer needed.
`--new-instance --demo` opens a separate review window without replacing the running project.
Do not force-close the user's existing project to try it.

`apps/suhina/src/liquid.rs` replays this frame's already-painted egui shapes into its own
offscreen renderer **before** the Settings modal and dimmer are painted. The source and
horizontal intermediate are sRGB attachments, sampled in linear light; the final vertical
pass explicitly encodes the result for egui's gamma-encoded native-texture contract.
The blur is separable, 19 taps per direction, sigma 3 downsampled pixels. The longest target
side is bounded at 1024 physical pixels. Three targets, sampler and bind groups are retained
until their dimensions change; the main renderer's texture ID is reused when resized.
No swapchain readback, previous frame or recursive rendering is used.

The UI-facing provider and independent Panel/Track/Clip options live in
`crates/ui/src/kit/liquid.rs`. Settings uses the live blurred backdrop under a theme tint.
Arrangement and Session track headers and clip backgrounds get a lightweight optical
coating (a continuous reflection with a brighter upper body and darker lower edge),
painted before labels, controls,
notes and waveforms. These coatings do **not** perform per-clip backdrop blur or refraction.
Surfaces smaller than 12 logical pixels stay solid. The three strengths are independently
saved percentages: 0 is off, 100 is the strongest supported material. Panels interpolate
from opaque to a 55% theme tint at full strength; track and clip gradients keep the source
hue and draw all text/notes/waveforms sharply on top. Missing settings default to 0.

The wgpu host installs its provider even while all strengths are 0; no backdrop capture
is requested until Panel liquid is raised. OpenGL has no provider and disables the controls.

OpenGL, disabled animations and the built-in high-contrast theme keep solid surfaces.
The accessibility gate is checked after running settings/theme changes too. If replay
would require an unsupported image texture or a foreign paint callback, the panel falls
back to solid rather than displaying an incomplete or stale backdrop.

Remaining costs/limits: capture currently clones/tessellates UI shapes and copies/compares
the current CPU font atlas, uploading the atlas only when it changes. This is UI-thread
work, never audio callback work, but needs profiling and a more efficient atlas update
path before the material can leave its experimental status. Foreign image support, playback/scroll/zoom
measurements, interactive visual QA and true edge refraction are still outstanding.
A GPU-only blur does not imply that the whole capture is free.

The native hardware test was run explicitly on Windows: constant RGB preservation,
linear-light checkerboard mean, impulse symmetry and a changed frame at a new size passed.
GPU readback exists only in that test, not in the application path. The two kit tests cover
default/tiny/reduced-effects solid rendering and removal of an old backdrop on failure.
Settings-window (7, including live independent strength controls and persistence) and constrained-pane (16) regression tests also passed, as did Clippy
for both the application and UI with all targets. These checks do not replace visual QA
or playback performance measurements.

```sh
cargo test -p suhina --bin suhina liquid::tests -- --include-ignored
cargo test -p suhina-ui --lib kit::liquid::tests
```

The two-pass shader in `crates/ui/src/kit/glass_blur.wgsl` now runs in the standalone
`glass_preview` native example. It uploads a fixed synthetic backdrop once, keeps three
768 by 432 RGBA textures and re-runs the horizontal/vertical GPU blur only when Blur changes.
Controls are drawn sharply over the material; Tint and a solid fallback can be compared.
The example built and its native window started on Windows with no GPU validation error
reported in stderr. This older example has not been re-profiled or connected to the DAW;
the native integration above uses its separate `apps/suhina/src/liquid.wgsl` instead.

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
