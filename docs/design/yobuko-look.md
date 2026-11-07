---
description: One look for YOBUKO's three programs (studio, Mixxx, rbxport) and the YOBUKO themes
read_when: YOBUKO theme, look.json, Mixxx skin, rbxport colours, mode line, seal
type: spec
status: draft
last_verified: 2026-11-07
covers:
  - crates/ui/themes/yobuko/**
  - crates/ui/src/app/look.rs
  - crates/dj/src/look.rs
  - crates/dj/src/mixxx_skin.rs
  - third_party/rbxport-app/src/lib/yobukoLook.ts
---

# YOBUKO's look

YOBUKO is three programs: the studio (Suhina), live DJ mixing (Mixxx) and USB sticks
(rbxport). They should read as one app, whichever theme the musician picks.

## The style: Ink & Echo

The default theme, **YOBUKO › Ink & Echo**, keeps Suhina's calm warm graphite and square
corners. YOBUKO's identity sits in two small marks shown in every window:

- a **2 px line along the top of the window in the colour of the app mode** (blue Studio,
  green Multiplayer, pink DJ; rbxport, the DJ side's USB tool, is always pink);
- the **red 呼子 seal** (`crates/ui/brand/yobuko-seal.png`): left of the mode pill in the
  studio, and in place of Mixxx's logo in Mixxx.

## Themes

The YOBUKO theme has variants for every taste, each five colours (background, panels, text,
accent, selection) in `crates/ui/themes/yobuko/variants/`:

| Variant | Kind |
|---|---|
| Ink & Echo | default, dark graphite |
| Mountain Night, Piou Pop | the README's night violet and candy colours |
| Midnight | near-black, for dark rooms |
| Matcha, Lagoon, Lantern | dark, green / teal / warm |
| Neon Candy | playful |
| Washi | light paper |
| Sakura, Mint, Lavender | light pastels |

Any other theme (Nord, Catppuccin, a theme the musician made) is followed too.

## How the other programs follow

1. Whenever the theme or the mode changes, the studio writes the colours in use to
   `<data>/YOBUKO/look.json` (`crates/dj/src/look.rs`, `app/look.rs`). It writes it each frame
   only once the `YOBUKO` data folder exists, and always just before Mixxx or rbxport starts.
2. **Mixxx**: before starting Mixxx, YOBUKO copies the LateNight skin of that Mixxx into the
   profile as `skins/YOBUKO`, moves every colour of it onto the look (greys follow their
   lightness up the look's ramp from background to text, so a light theme turns Mixxx light;
   reds stay alarms; other colours take the accent), replaces the logo with the seal and
   selects it (`[Config] ResizableSkin YOBUKO`, `Scheme PaleMoon`). Links inside the copy name
   its folder in full, as Mixxx resolves `skins:` in its own install only. A Mixxx the DJ
   installed on their own keeps any other skin they chose. The copy is remade only when the
   look or that Mixxx changes; a running Mixxx takes a new theme when it is next started.
3. **rbxport** reads the file at start and every 2 s and moves its interface tokens with the
   same rule; waveform, cue, phrase and colour-label colours keep their rekordbox meaning.
