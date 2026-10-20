---
description: The Settings window's pages and their headed sections, which setting is on which page, how a page scrolls, the search field, and the one table the window is drawn from
read_when: Settings window, settings page, where a setting goes, adding a setting, settings scroll, settings search, settings table, settings_rows, page order, General page, Appearance page
type: spec
status: current
last_verified: 2026-10-19
covers:
  - crates/ui/src/settings/settings_view.rs
  - crates/ui/src/settings/settings_rows.rs
  - crates/ui/src/settings/track_sizes.rs
  - crates/ui/src/settings.rs
  - crates/ui/tests/settings_window.rs
---

# The Settings window

Options › Settings (Ctrl+,) opens the Settings window over the main window. How it is drawn
(sizes, colours, the controls) is in [components](../design/components.md) › Settings; this spec
says what is on each page, how the pages scroll and how a setting finds its place.

## Sources

Suhina's own layout: no other program was observed for it. The grouping follows what the user is
doing when they look for a setting (setting up audio, editing in the piano roll, browsing
samples), not which part of the code reads it.

## Pages and sections

The list on the left shows the pages in this order. Each page has headed sections; every setting
is on exactly one page, under one heading.

| Page | Section | Settings |
|---|---|---|
| General | Projects | When opening a project, At start, Projects folder, Templates folder, Start template (shown with At start › Template) |
| General | NAM | NAM models folder, IR folder — where the two NAM devices' dropdowns list their files ([nam spec](nam-amp-and-cab.md) › Folders) |
| General | Backups | Backups, Clean up backups, Last backup (read only) |
| General | Agents | Let agents work on open projects (off by default; [ADR 0028](../decisions/0028-mcp-in-the-running-app.md)) |
| General | Dropping files | Several files, Warp mode |
| General | Updates | Check for updates, Channel (Test builds only in developer mode), Developer mode ([spec](updates.md)) |
| Audio | Output | Driver, Device, Sample rate, Buffer size (or Engine block), Exclusive mode, Input |
| Audio | Cue | Cue output, Cue volume |
| Audio | Now | Output, Latency, Key to sound, Input now (read only), List devices again |
| MIDI | MIDI inputs | Each MIDI input port, On or Off |
| Plug-ins | Editors | Editors (Window, Device view) |
| Plug-ins | Library | Folders (each on or off, with its format; add with a format, remove), Scan and Rescan all with a count of what was found, Plug-in manager (opens the [plug-in manager](plugin-manager.md)) |
| Arrangement | Clips | Allow sample stacking, Allow MIDI stacking, Waveform and note colour (Auto, Light, Dark, Custom), Custom waveform and note colour, Audio clip colour strength |
| Arrangement | Tracks | Track headers, Track header colour strength |
| Arrangement | Track sizes | Default track size, Default return size, Advanced track sizes, Tiny height, Compact height, Small height, Medium height, Large height |
| Session and mixer | Launching | Select on launch |
| Session and mixer | Mixer | Mixer controls |
| Clip editor | Piano roll | Pulled to the top, Piano roll grid, Toolbar size |
| Clip editor | Mouse | Left click draws, Right click deletes a note, Shift+right click splits, Alt+right plays from the point |
| Transport | Play and stop | Play from, Stop |
| Detail area | Devices | Device chain |
| Browser | Browser | Browser side |
| Browser | Previews | Auto-play, Preview volume |
| Browser | Sample map | Map layout, Map preview on hover, Map: longest sample |
| Browser | Tags | Tag limit |
| Appearance | Liquid (experimental) | Panel liquid, Track liquid, Clip liquid: independent 0–100% strengths, 0% off |
| Appearance | Window | Zoom, Animations |
| Appearance | Waveforms | Waveform scale |
| Appearance | Graphics | Renderer |
| Appearance | Theme | The themes, the theme in use, its colours |
| Shortcuts | Keys | The preset, every action's keys, Reset all |

Where a setting is shown never changes how it is saved: each keeps the key it has in
`settings.json` (or `keymap.json` for keys and the mouse switches), so a settings file written
before a setting moved loads with the same values.

## Scrolling

Each page scrolls when it is taller than the room under the title, at any window size: with the
mouse wheel over the page, by dragging its scroll bar on the right, and by moving the keyboard
focus (Tab) to a control out of view, which scrolls it into view. Opening another page shows it
from its top. A list that fills the space left on its page (the theme colours, the shortcuts, the
plug-ins found) is at least 160 px tall, so in a short window the page scrolls down to it, and
the list scrolls on its own.

## Search

A field above the list of pages, "Search settings", filters the settings by what is typed. A
setting matches when every word typed is in its name, its note, its section or its page (case
does not matter). While the field has text, the page shows only the matching settings, from
every page, under headings that name the page and section ("Browser › Previews"), and no page
in the list is lit; "No setting matches" when none does. Clicking a page empties the field and
shows that page; Escape in the field empties it (a second Escape closes Settings).

## The table

The window is drawn from one table, `ROWS` in `crates/ui/src/settings/settings_rows.rs`: an entry per
setting with its page, its section, its name, its note, the keys it is saved under, and how its
control is drawn. A page shows its entries in the table's order, starting a section where the
section changes. Most entries are one row (a name, a note and a segmented control or a field); a
few are a block that draws several rows or a list (the audio output, the themes, the shortcuts).
Adding a setting to the window is adding its entry under its section; moving one is changing its
page and section. Tests check that every saved setting is on exactly one page.

Named row sizes and their ordering, live application and persistence are specified in
[Arrangement editing](arrangement-editing.md#named-row-sizes). Pixel values are hidden until
Advanced track sizes is enabled; the same bounded controls remain editable in both modes.

Track sizes is an interdependent search group: a match on any of its settings shows the whole
section, so searching for the default also exposes Advanced and per-size tuning. Default changes
and tuning apply on the next frame, including while Settings is open. Changing a default replaces
height overrides for that kind of row (regular tracks or returns); preset tuning preserves manual
heights that do not match a named size.

## Experimental surface materials

Appearance › Liquid (experimental) has three independent percentage sliders: Panel liquid,
Track liquid and Clip liquid. All default to 0 (solid/flat), update the material on the next
frame and persist as `liquid_panel_strength`, `liquid_track_strength` and
`liquid_clip_strength`. Input is clamped to 100; invalid values preserve the default and
produce the usual settings warning. Older files leave all three off.

Panel liquid controls how much of the Settings backdrop shows through the theme tint.
Track liquid controls the light-to-dark reflection on MIDI/audio track headers in both
views; Clip liquid controls the same coating on clip bodies. Labels, controls, notes and
waveforms are drawn afterward. The latter two are surface reflections, not per-clip blur.
The renderer is described in [Glass rendering experiment](../design/glass-rendering.md).

Controls are unavailable in a host without the GPU provider. Selecting GPU in Renderer
requires restarting the app if it is currently running on OpenGL. Disabled animations or
the built-in high-contrast theme keep the material off without erasing saved strengths.
The feature remains experimental: full playback performance qualification and refraction
are not complete.
