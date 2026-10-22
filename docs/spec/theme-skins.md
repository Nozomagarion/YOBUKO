---
description: Portable theme skins with SVG controls, level meter styles and movable control-bar modules
read_when: theme skin, SVG controls, theme package, movable toolbar, meter style
type: spec
status: draft
last_verified: 2026-10-22
covers:
  - crates/ui/src/kit/skin.rs
  - crates/ui/src/app/themed_toolbar.rs
  - crates/ui/src/settings/skin_editor.rs
  - apps/suhina/src/theme_files.rs
  - crates/ui/src/kit/native_css.rs
  - crates/ui/src/kit/theme_css.rs
  - crates/ui/src/kit/style_class.rs
---

# Theme skins

A theme owns its control artwork, level-meter appearance and control-bar layout.
Changing a bundled theme creates an editable copy. Duplicating, renaming, resetting,
exporting and importing preserve the complete theme. Old colour-only themes retain
Suhina's existing layout and controls.

The optional `skin` object in theme JSON contains `assets`, `meter`, and `toolbar`.
SVG assets may be inline SVG strings in JSON or relative `.svg` file paths beside
that JSON. Portable ZIPs put each theme and its artwork in a separate folder.
Artwork changes presentation only: command dispatch, keyboard access, value ranges,
gestures and undo remain owned by the application. Missing or invalid artwork uses
Suhina's normal drawing. External images, network resources and SVG scripts are not
loaded. SVGs are parsed and rasterized once per asset, scale and palette, never on
the audio thread; texture cache size and raster dimensions are bounded.

Asset roles include fader cap and rail, knob body and pointer, button surface and
play/stop/record/solo/arm/cue icons. State suffixes `.hover`, `.active`, `.disabled`
optionally override a base role. `currentColor` uses the control's foreground.
The fader cap moves along the rail; the knob pointer rotates with its value.

Meter styles are continuous, segmented and needle. They use the existing stereo
peak/RMS monitor; presentation never changes measurement or sound. The clip light
and click-to-reset work with every style. Segment count is bounded. Needle displays
use peak for the thin pointer and RMS for the heavier pointer. These are dBFS level
displays, not calibrated analogue VU or loudness measurements.

Toolbar modules have stable names and a saved order/visibility. The built-in layout
is used when no toolbar is supplied. Editing exposes labelled drag handles rather
than stealing normal control gestures. Escape cancels a drag. Hidden modules can be
restored from the editor, and resetting the layout restores the default. Narrow
windows scroll the customized toolbar horizontally; controls do not overlap.

Theme ZIP import validates the entire input before writing. Paths must be relative,
remain within the theme folder, contain no parent traversal or symlinks, and name
supported assets. Reads, file counts, SVG sizes and total expanded data are bounded.
Imports choose unused theme ids rather than overwriting existing themes.

## Native CSS in this draft

The initial reader supports `:root` palette variables (`--window-background` maps to
`window.background`), class/descendant colour, background and border colours, integer
pixel font sizes and corner radii. `.meter` accepts `style` and `segments`; `.toolbar`
accepts comma-separated `order` and `hidden` module lists. Control `image` declarations
name package-relative SVGs, including state suffixes. `.window` accepts a relative PNG
`background-image`, `background-opacity` and `background-blur` (0–100).

These are a bounded native subset, not complete CSS. Variables with `var()`, arbitrary
modern selectors, queries, layout algorithms and animations are not implemented by this
reader. The advanced target and engine research are in
[ADR 0030](../decisions/0030-native-theme-css.md).

Every edited theme has `<id>.json`, `<id>/style.css` and `<id>/assets/` in the themes
folder. CSS and JSON changes are observed within the existing one-second poll interval.
An editor change writes both; an external CSS change updates JSON; an external JSON
change updates CSS. Current saves canonicalize CSS from the native model, so comments
and formatting are not preserved. Independent field edits merge; conflicting changes to
one field preserve both files and report the field. A malformed stylesheet or missing
resource does not overwrite the saved JSON or replace the currently loaded theme.

ZIP export places each JSON, style.css, SVG and background PNG together under its theme
id. Import validates resources before writing any theme, and gives colliding ids a suffix.
The in-app CSS editor accepts native values and styles; package-relative image URLs are
resolved when importing or reloading theme files. The SVG editor accepts vector markup.

## Advanced CSS adapter (experimental)

`skin.css` retains an authored modern stylesheet verbatim in JSON. Export and native
storage additionally write `advanced.css`, referenced by `advanced-stylesheet`.
Editing either representation mirrors to the other without changing comments or
formatting. Conflicting source edits preserve both files. This source remains separate
from canonical `style.css`, which maps literal palette and skin values into native JSON.
The same byte limit and package path restrictions apply to both stylesheets.

The advanced editor supplies a Flex example and calls the Stylo/Taffy adapter. Stylo
computes variables, layers, inheritance, CSS colours, value expressions and viewport
media conditions for class scopes. Taffy computes Flex/Grid rectangles for the actual
visible toolbar modules (`#tempo`, `#transport`, etc.); the same rectangles bound their
native input targets. The host still defines toolbar height. Scoped class trees do not
yet expose real project objects, siblings or interactive pseudo-class states. CSS
painting of gradients, shadows, transforms, filters and animations remains unsupported.
This integration is experimental, not a claim of universal CSS conformance.
