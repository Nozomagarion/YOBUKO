<!-- Copyright (C) 2026 The Suhina contributors; SPDX-License-Identifier: GPL-3.0-or-later -->
# Suhina theme package: author and AI guide

## What this package is

A theme changes Suhina's native egui interface through colours, SVG control artwork,
meter appearance, background images, and connected CSS styles and layouts. It contains
data, not executable plug-ins. Audio, parameter gestures and commands remain native.
Use this guide when creating, editing, explaining or sharing a theme, including when
asking an AI assistant to modify one. Inspect the supplied files before making changes.

## Files and why they are separate

```text
my-theme/
  my-theme.json       # identity, palette, skin, layout and resource references
  style.css           # canonical native properties mirrored with JSON
  advanced.css        # optional preserved modern CSS source
  assets/*.svg        # control artwork
  background.png      # optional background image
  README.md           # this guide
  examples/dark-gloss.css  # optional paint-only example; not enabled automatically
```

Installed themes keep `<id>.json` beside the `<id>/` resource directory instead.
Relative image and stylesheet paths in JSON are authoritative. Keep the whole
package together; do not move only the JSON when it references external resources.
Drop the ZIP on Suhina to import it, then select it in Settings → Appearance → Themes.
Open the themes folder from that page to edit the installed files.
The template ZIP also has `template.json` and reference palettes in `examples/`;
the importable example is `my-theme/`.

## How and when to edit each file

1. Duplicate a bundled theme in the editor or start with the supplied example.
2. Set the JSON `name` to a descriptive name. Theme file ids use letters, digits and `-`.
3. For palette changes, edit JSON `colors` or native `style.css` tokens. For example:

   ```css
   :root { --accent: #7FD1B9; --window-background: #14161A; }
   meter { style: segmented; segments: 24; }
   toolbar { offset-transport: 48px; }
   ```

   Native CSS token names replace JSON token dots with hyphens. JSON colours use
   `#RRGGBB` or `#RRGGBBAA`, alpha last. Missing palette tokens inherit Suhina Dark.
4. For conditional styles, selectors, variables, effects or connected Flex/Grid
   layout, edit `advanced.css`. JSON `skin.css` mirrors this source losslessly.
5. For a new control silhouette, edit an SVG and reference it in `skin.assets`.
   For layout positions, the toolbar's right-click → Edit toolbar mode is usually
   easier: drag the actual controls on the 8-point grid. Done saves; Cancel or
   Escape discards. Entering/leaving edit mode alone preserves the layout.
6. Save one source at a time and inspect Suhina's warnings and the actual interface.
   Files are watched approximately once a second. Independent JSON/CSS edits merge;
   conflicting values preserve both files and require correction before saving.
   Canonical `style.css` is normalized; its comments/formatting are not preserved.
   `advanced.css` retains authored source. Conditional CSS is not flattened into
   unconditional JSON palette tokens.
7. Test normal, hover, active and disabled states, narrow/wide windows, interaction,
   clipping and contrast. Export and reimport the ZIP to check portability.

To try the supplied dark metallic look, use **Load dark gloss example** in the
advanced editor and then Apply, or copy `examples/dark-gloss.css` into `advanced.css`.
It demonstrates original gradients, rounded SVG-backed buttons, box/text shadows
and coloured transport controls while preserving native geometry. Back up your
current CSS before replacing it with an example.

## SVG inheritance and safe defaults

Supported roles are `fader.cap`, `fader.rail`, `knob.body`, `knob.pointer`, `button`,
`play`, `stop`, `record`, `solo`, `arm` and `cue`. State variants append `.hover`,
`.active` or `.disabled`. Example JSON override:

```json
{"skin":{"assets":{"play":"assets/play.svg","button.hover":"assets/button.hover.svg"}}}
```

Lookup uses the theme's state SVG, then its base SVG, then Suhina's default SVG.
Themes only need to define the images they replace. Removing an override restores
inheritance. Knob pointers rotate with the parameter and fader caps follow its value.
Use self-contained SVG paths and shapes; native labels stay native. `currentColor`
uses the current theme/control colour. Default body/rail/cap SVGs additionally use
`data-suhina-ink` to retain their semantic palette colour when copied or exported.

`themes/_defaults/assets/` is a protected reference library. Editing its valid artwork
creates a numbered `Suhina Default (modified N)` user theme before restoring the
original. Select that new theme in the gallery to use the edit. Missing default
files and `_defaults/suhina-default.zip` are recreated from SVGs embedded in the app.
Malformed edits are preserved in a recovery ZIP before restoration when within
the resource limit; oversized or inaccessible files remain untouched with a warning.
Edit an ordinary user theme's own `assets/` for direct live changes.
Portable exports include inherited artwork so another machine receives the same images.

## What's possible now

| Area | Supported customization |
|---|---|
| Palette | JSON/native CSS tokens, transparency, project-safe creation defaults |
| Artwork | SVG roles and states, moving faders and rotating knob pointers |
| Meters | `continuous`, `segmented` (4–64 segments), `needle`; signal remains dBFS |
| Materials | Optional bounded PNG background, opacity/blur; panel/track/clip gradient and liquid settings |
| Toolbar | Native layout plus anchored offsets, explicit order/visibility, connected Flex/Grid |
| Workspace | CSS layout of `#panel-browser`, `#panel-main`, `#panel-detail` under `#workspace` |
| Styles | Connected foreground/font size/radius, linear/radial/conic and repeating gradients, bounded inset/outset box shadows and shared native text shadows |
| Motion | Connected paint transitions/keyframes; native animation-off/high-contrast settings suppress motion |
| Selectors | Retained native class tree, child/sibling matching, connected hover/active/disabled/focus states, named object identities |

The advanced adapter uses Stylo for CSS and Taffy for connected layout. For example:

```css
.browser { color: #D8E8DF; background: linear-gradient(135deg, #243A30, #101C17); }
#workspace { display: grid; grid-template-columns: 240px minmax(0, 1fr);
             grid-template-rows: minmax(0, 1fr) 180px; gap: 8px; }
#panel-browser { grid-column: 1; grid-row: 1 / -1; }
#panel-main { grid-column: 2; grid-row: 1; }
#panel-detail { grid-column: 2; grid-row: 2; }
```

Workspace layout respects native panel visibility. CSS-owned sizes replace the
browser/detail splitters; removing the layout rules restores saved native sizes.
The toolbar exposes `browser`, `tempo`, `signature`, `grid`, `count-in`, `transport`,
`position`, `metronome`, `mapping`, `performance`, `views`, `agents` module ids.
Device SDK controls expose semantic classes such as `.device-panel`, `.knob`,
`.button` and `.device-control`. Stable named track/clip/slot/scene scopes also exist.
The native centred toolbar exposes the same module ids as CSS layouts. Its main
buttons additionally expose `#control-play`, `#control-stop`, `#control-record`,
`#control-browser-toggle` and `#control-metronome-toggle`. Paint-only rules preserve
the existing geometry. Button backgrounds (gradients/shadows) remain visible above
SVG body artwork, below glyphs and labels; CSS corner radius also clips that artwork.
Class coverage and host integration are still expanding: inspect the current spec
and test the actual target instead of inventing selectors.

## Limits: what an author or AI must not assume

This is an experimental native adapter, not a browser. Stylo parsing a property
does not mean egui paints it. Some raw widgets still need integration. Transforms,
filters, downloadable fonts, full typography, geometry animation and
the reduced-motion media feature are not connected. Bars and floating windows stay
host-managed; the toolbar's height stays native. Do not claim universal browser CSS
or container-query support. Notices are useful but are not a complete CSS validator.
Use the editor's Flex/Grid examples and check the current support specification.

Stylesheets are limited to 128 KiB, bounded nesting and native scope budgets.
SVGs are limited to 128 KiB each and 64 assets per theme. External/network images,
scripts, HTML, shell commands and SVG external resources are not loaded. Resource
paths must stay relative inside the package. SVG controls and effect textures are
cached with bounded resolution/counts. Box-shadow blur is an approximation.
Text shadows affect shared native text and SDK vector labels; ordinary egui labels
that bypass that helper are not covered. Up to four text-shadow layers use offsets
within ±256 points and at most 32 points of blur. Blur uses a fixed nine-tap weighted
approximation on cached glyphs, not an image convolution.
Keep assets small, use readable contrast, and retain useful native labels.

Track/clip colours belong to the project. `clip.midiDefault` and `clip.audioDefault`
apply when new tracks/clips are created; theme edits must not recolour existing ones.
Use `text.primary` contrast of at least 7:1 and hints of at least 3:1 where possible.

## For AI-assisted edits

Describe the requested visual change and choose the smallest supported change.
Preserve unrelated palette, assets, layout, project data and native behavior.
Return a complete portable package and explain modified files, preview limitations
and verification. Never invent a successfully tested feature or silently discard
conflicting edits. Back up the user's theme before replacing it.

Current specifications: https://github.com/suhina-daw/suhina-daw/blob/main/docs/spec/theme-skins.md
and https://github.com/suhina-daw/suhina-daw/blob/main/docs/decisions/0030-native-theme-css.md
(on a development branch, use that branch's corresponding documents).
