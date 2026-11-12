# Where YOBUKO stands

This is the project's single progress file: what changed, why, what was checked and what comes
next. Newest first. Planned work, written code, packaged builds and real-world tests are kept
apart. (Until 1 November 2026 this file was kept in French as `SUIVI_PROJET.md`; the full
original is in the git history.)

## The studio's rounded look pushed closer to the DJ page — 12 November 2026 (branch `claude/project-thread-mkzv4x`)

**What changed.** With a YOBUKO theme, the studio now borrows more from the DJ page the owner
likes: the launcher, mixer, browser and detail area are outlined cards with the DJ page's 14 px
corners; clips and buttons are rounder and stand a little further apart; play, stop and record
are round buttons; and the picked view (Launcher or Arrangement) sits on a pill instead of an
underline. Every control is still there and still answers clicks in the same place; Suhina's
own themes keep their square look.

**Checked.** `cargo test -p suhina-ui` (all passed), `cargo clippy -p suhina-ui --all-targets
-- -D warnings` (clean); before/after render with `render_the_shell_for_review`. Not yet seen on
a physical machine.

**Next.** The owner looks at it in the app and says what to push further.

## A slider for the size of the whole interface — 12 November 2026 (branch `claude/project-thread-j96gyr`)

**Goal:** the owner can pick the size of YOBUKO's whole interface in Settings and see it change
while dragging.

- **Code written:** Settings › Appearance › **Window zoom** gets a slider (50 % to 200 %, 5 %
  steps) in front of the existing −, value and + buttons. While the slider is held, the whole
  window (studio, DJ, USB, multiplayer, Settings itself) grows or shrinks at once; the value is
  saved when it is let go. The slider moves with the window as it rescales, so the drag is
  measured in screen pixels: the value follows the hand and never jumps by itself. Ctrl/Cmd and
  + / − / 0 still work.
- **Checks passed (Linux, cloud container):** a new UI test (the window grows while the slider
  is held, a still pointer keeps the value, the value button still goes back to 100 %), all
  `suhina-ui` tests, clippy, the settings help export check and `docs.py check`.
- **Limits:** not tried on a real Mac or Windows screen yet; plug-in editor windows keep their
  own size.
- **Next criterion:** the owner drags the slider in a test build and finds a size they like.

## The DJ space gets Mixxx's deck and mixer functions, part 2 — 11 November 2026 (branch `claude/project-thread-fz5ul5`)

**Goal:** the second of three parts toward every Mixxx function in YOBUKO's own DJ space: key
lock, effects, beat sync, quantize, tap tempo, the sampler and coloured waveforms.

- **Code written:** each deck has **KEY LOCK** (the pitch stays when the tempo moves), **KEY −/+**
  (±12 semitones) and **KEY SYNC**; **Sync** now lines up the beats, not only the tempo;
  **QUANTIZE** snaps cues, hot cues and loops to the beat; **TAP** sets the tempo; **GRID** moves
  the beat grid. Two **effect units** (three effects each, eleven effects: echo, ping pong,
  reverb, filter, flanger, phaser, trans, autopan, bitcrusher, distortion, noise) go on deck A,
  deck B or the master, in time with the music. An **eight-slot sampler** plays library tracks
  over the mix; the pads switch between hot cues and the sampler. The waveforms are **coloured**
  (red lows, green mids, blue highs) and show the **beat grid**. The DDJ-FLX4's TAP, Beat FX
  section and SAMPLER pads work.
- **Checks passed (Linux, cloud container):** engine tests (key lock keeps the pitch and loops
  without a gap, an echo repeats on the beat and rings out, every effect plays without blowing
  up, a sampler slot plays at its level), session tests for `dj.deck.key`, `dj.fx.set` and
  `dj.sampler.*`, UI tests for the new buttons, a screenshot drawn by the real program.
- **Limits:** the effects are YOBUKO's own versions of the usual DJ effects, not copies of
  Mixxx's sound. No recording, four decks, Auto DJ, history, crates or microphone yet (part 3).
  Not tried with a real DDJ-FLX4, real speakers or on Windows and macOS.
- **Next criterion:** the owner tries the key lock, an echo and the sampler in a test build;
  then part 3.

## Suhina and rbxport brought up to date — 11 November 2026 (branch `claude/project-thread-9g0703`)

**Goal:** YOBUKO carries everything Suhina and rbxport have released since the last sync, and a
fresh Windows installer is built from it.

- **Code written:** upstream Suhina `main` merged up to `6230850b` (185 commits: per-clip gain,
  pitch and warp with a gain envelope, tag folders in the browser, the new styling system, and
  more). The bundled rbxport app goes from 1.9.0 to 1.10.0 (`d0ff75b3`, 286 commits: selectable
  My Tag columns in the Track Filter, safer USB sync and export fixes, the audio output device
  remembered across launches, and more), with YOBUKO's patches put back (inbox, shared look, no
  self-update, own identifier). Shared multiplayer projects now use project format 11.
- **Limits:** the two rbxport library crates the studio itself links (`third_party/rbxport-analysis`
  and `third_party/rbxport-export`) stay at 1.9.0; the DJ features people use are in the bundled
  app, which is at 1.10.0. Mixxx is unchanged.
- **Next criterion:** the Windows installer built from this branch installs and opens the studio,
  Mixxx and rbxport on the owner's PC.

## The DJ space gets Mixxx's deck and mixer functions, part 1 — 11 November 2026 (branch `claude/project-thread-fz5ul5`)

**Goal:** the owner wants every function of Mixxx in YOBUKO's own DJ space, drawn in the rounded
look, not a short selection. The work comes in three parts; this is the first (decks, mixer,
library and the controller's pads). Upstream Suhina must stay easy to pull in, so the DJ code
lives in YOBUKO's own files.

- **Code written:** each deck now has **8 hot cues** (coloured pads, kept with the track in the
  DJ library), **loops** (IN, OUT, RELOOP/EXIT, beat loops of 1/32 to 64 beats with /2 and ×2),
  **beat jumps**, **nudge** buttons, **back to start**, **EJECT** and a tempo range button
  (±8, ±16, ±50 %); the waveforms mark the cues and the loop. The mixer gained a **Trim** knob
  and a **meter** per deck, a **Master** level with a stereo meter, the **headphones' level**
  and their **cue/master blend**. The library can be **searched**, **sorted** by title, BPM, key
  or length, and shows **playlists** (new playlist, add to / remove from, delete). The
  **DDJ-FLX4** drives the new parts too: SHIFT, hot cue, beat loop and beat jump pads, loop
  buttons, CUE/LOOP CALL, the quick jumps, TRIM and the headphones mix; its jog wheels now
  nudge a playing deck. The decks play through a player of YOBUKO's own
  (`crates/engine/src/deck_voice.rs`), and Suhina's preview player is back to upstream's code.
- **Checks passed (Linux, cloud container):** engine tests for loops without a seam, the nudge,
  trim, master, meters and the headphones blend; session tests for `dj.deck.loop`,
  `dj.deck.bend`, the trim and the mixer's new values; UI tests for the hot cues, loops, beat
  jumps, tempo range, eject, the mixer and the library's search, sort and playlists; a
  screenshot drawn by the real program.
- **Limits:** still no key lock, effects, beat-aligned sync, sampler, tap tempo, recording or
  four decks (parts 2 and 3). A hot cue cannot be set while the library analyses a file. Not
  tried with a real DDJ-FLX4, real speakers or headphones, or on Windows and macOS.
- **Next criterion:** the owner checks in a test build that the decks and mixer have the
  buttons they use in Mixxx; then part 2 (key lock, effects, beat sync and grid, sampler).

## Soft, rounded studio like the YOBUKO One mock-up — 11 November 2026 (branch `claude/project-thread-mkzv4x`)

**Goal:** the owner found the studio "all square" with a thin font, and liked the rounder look of
the YOBUKO One mock-up much better. The real studio should look like the mock-up, in every one of
the twelve YOBUKO themes.

- **Code written:** with a YOBUKO theme (`yobuko` and its eleven variants), the studio draws
  **soft shapes** (`crates/ui/src/kit/shape.rs`): clips, track headers, scene and stop buttons are
  rounded and stand a little apart with no grid lines between them; the launcher, the mixer, the
  browser and the detail area are rounded **cards** on a deeper background; mixer strips, buttons,
  value fields, dropdowns, menus and windows have round corners; fader caps are flat soft tabs;
  browser rows light up as rounded pills. Clip, track and strip names use **Manrope Bold**, which
  reads better than the regular weight. Nothing moves: every button still answers clicks in the
  same place, so the tests and the muscle memory stay valid. Suhina's own themes stay square.
- **Checks passed (Linux, cloud container):** the UI tests; screenshots drawn by the real program
  in Ink & Echo and Sakura, and Suhina Studio to check it is unchanged.
- **Limits:** the toolbar layout and text sizes are unchanged (making them larger would move
  controls); the DJ page is restyled in its own branch.
- **Next criterion:** the owner opens a test build and says whether it now feels like the mock-up.

## DJ mixing inside YOBUKO's window — 11 November 2026 (branch `claude/project-thread-fz5ul5`)

**Goal:** the owner wants live DJ mixing in the same window as the studio, never in Mixxx's
separate window (asked again on 11 November after the shell merged). A program cannot put
another program's window inside its own on macOS, so YOBUKO now draws its own decks.

- **Code written:** the **DJ** tab shows two decks and a mixer in YOBUKO's window. Each deck has
  its track's waveform (the whole track and a close view around the playhead), **Cue**, **Play**,
  **Sync** and a tempo slider; the mixer has **High**, **Mid**, **Low** and **Filter** knobs,
  headphones and a level per deck, and a crossfader. The DJ library sits under them with **Load
  A** / **Load B**; audio files and folders dropped on the page, the Music folder and projects
  sent to DJ go into it. A **Pioneer DDJ-FLX4** plugged in drives it all (Play, Cue, Sync, Load,
  browse, faders, EQ, filter, tempo, crossfader, jog wheels) while the DJ space shows. The sound
  is the engine's two decks, which gained a speed, a three-band EQ and a filter each. Mixxx is
  still one button away (**Open Mixxx instead**).
- **Checks passed (Linux, cloud container):** engine tests for the speed, the EQ kills and the
  filter; session tests for the new `dj.deck.rate` and `dj.deck.tone` commands; UI tests for the
  page, loading, Sync, drops into the library and the FLX4 (with a simulated controller); a
  screenshot drawn by the real program.
- **Limits:** not tried with a real DDJ-FLX4, real speakers or headphones, or on Windows and
  macOS. No key lock (the pitch follows the tempo), no loops, hot cues, effects or recording yet;
  the controller's lights stay off. Beat-matching is by tempo only: Sync does not line up the
  beats.
- **Next criterion:** the owner mixes two tracks with the FLX4 in a Windows test build and says
  whether it feels right.

## One shell for every space, and Piou — 11 November 2026 (branch `claude/project-thread-fz5ul5`)

**Goal:** the studio, live DJ mixing and USB export should feel like one program that is pleasant
to move around in, with the themes kept, and coming back into a shared session should take one
click. The whole proposal is a clickable mock-up ("YOBUKO One"); it is built in three steps.

- **Code written (step 1, the studio):** the mode pill became four **tabs** at the top right,
  **Studio**, **Multiplayer**, **DJ** and **USB**; the tab of the space showing is filled in its
  colour. **Piou**, the mascot, lives beside the tabs and on each space's page: it nods on the beat
  while the project plays, calls while a room waits for friends, hops when one arrives, worries
  while a dropped connection is being restored and sleeps when nothing moves. It also says in a
  short note what happens away from the page that would tell it (a friend joined, the connection
  dropped, a track reached the DJ library). While a shared session runs, its state shows beside
  the tabs in every space. The Multiplayer page before a session is two cards, **Open a room**
  and **Join a friend**, with **Rejoin last session** for the invitation just used; the long
  explanations wait behind **How sharing works**. USB export has its own page instead of a button
  on the DJ page.
- **Checks passed (Linux, cloud container):** the UI tests, including new ones for the tabs, the
  USB space, Piou's moods and notes and the rejoin button; screenshots of the four spaces drawn by
  the real program.
- **Limits:** Piou is redrawn from shapes (the first bird pictures are no longer available as
  separate files), and the owner has not yet said whether this drawing is the one to keep.
  **Rejoin last session** is remembered until YOBUKO closes, not after a restart. Mixxx and
  rbxport still open as their own windows without the tabs.
- **Planned, not written:** step 2, the same bar in rbxport's window and over Mixxx, with the
  window staying in place when the space changes; step 3, friends remembered after a first
  session so that they join an open room without any code.
- **Next criterion:** the owner tries the mock-up and a test build of step 1 on Windows and says
  whether the direction and Piou are right.

## Ready-to-install releases and an English repository — 1 November 2026 (branch `feature/online`)

**Goal:** friends download one file from the Releases page and double-click it; the repository
reads in English.

- **Code written:** each tagged release now gets `YOBUKO-Mac.dmg` (drag YOBUKO to
  Applications), `YOBUKO-Windows-Setup.exe` (per-user installer with a Start menu entry), the
  Linux AppImage and `.deb`, and install notes, attached by the release workflow's `publish` job.
  The multiplayer CI builds the same disk image and installer on every pull request. The French
  README, guide and progress file were replaced by English ones.
- **Limits:** the Mac app is not notarised and the Windows installer is not signed, so both
  systems warn on first launch (the release notes explain the two clicks to get past it).
- **Next criterion:** a release whose page lists the four installers, installed on a Mac and a
  Windows PC.

## Rekordbox USB export and "Open rbxport" — 1 November 2026 (branch `feature/online`, PR #6)

**Goal:** export the DJ library to a USB stick that Pioneer CDJ/XDJ players read, from inside
YOBUKO, and open the rbxport app with one button.

- **Code written:** rbxport's exporter crates are vendored in `third_party/rbxport-export/`. The
  DJ workspace has **Export to USB stick (Rekordbox)**: each track is re-analysed, a confirmed
  beat grid wins, the first eight cues become hot cues A–H. **Open rbxport** starts the installed
  rbxport app, or opens its download page.
- **Checks passed:** CI green on macOS, Windows and Linux.
- **Not yet done:** a test on a real CDJ.

## Three workspaces — Studio, Multiplayer, DJ

The top right of the window has three buttons: **Multiplayer**, **Studio** and **DJ**, to switch
between the collaboration panel, the production view and the DJ library.

## One code for the whole group — 31 October 2026 (branch `feature/online`)

**Goal:** the host gives one code and several friends join the same session.

- **Code written:** the same code (short or long) admits everyone who has it, for thirty minutes,
  up to **seven guests** (eight people with the host). **New invitation** only replaces a code
  that reached the wrong person. **Protocol 18**: everyone needs this version.
- **Checks passed (Linux, local):** seven guests with one code, an eighth refused, then a seat
  freed and taken again; three guests through the same short code; interface tests. A real
  Internet test on GitHub runners joined three guests (Linux, macOS, Windows) to one host with
  one short code in 6.4 s.
- **Not yet done:** a real test between several homes.

## New name — 30 October 2026

The app is now called **YOBUKO** (a traditional name of Yamabiko, the echo yōkai); technical
`suhina-*` names are unchanged.

## Plug-ins in shared sessions — 30 October 2026 (branch `feature/online`)

**Goal:** share a real project, with its effects and plug-ins.

- **Code written:** the shared copy accepts every built-in effect and instrument, groups and
  CLAP, VST3 and VST2 plug-ins with their settings (up to 4 MiB per plug-in, 32 MiB in total).
  Each computer runs its own plug-ins; a guest sees the list of those they lack. Every built-in
  effect's parameters can be adjusted together. **Protocol 17.**
- **Checks passed (Linux, local):** copy of a 3 MB VST3 state online and on LAN, a Reverb
  adjusted from the guest, host files refused (Sampler, NAM), missing plug-ins listed in the UI.
- **Not yet done:** adjusting a third-party plug-in's knobs together, freezing a track to audio
  for a guest without the plug-in, sharing Sampler samples.

## Branches merged — 30 October 2026 (branch `feature/online-merge`)

**Goal:** heavy projects, short codes and resuming after a drop in one version.

- **Code written:** `feature/online-heavy`, `feature/online-shortcode` and `feature/online-robust`
  merged; **protocol 16**. A guest who drops while sounds are still arriving in the background
  comes back alone, reloads a fresh copy and downloads only the rest of the sounds: what already
  arrived stays in **Sound cache**.
- **Checks passed (Linux, local, simulated drops):** 140 multiplayer tests, run three times, strict
  clippy, multiplayer interface tests.

## Sturdier multiplayer — 30 October 2026 (branch `feature/online-robust`)

**Goal:** make sessions survive connection drops before the test between two homes.

- **Code written:** a guest whose connection drops shows **Reconnecting…** and comes back alone
  for a minute, without a new code; the host keeps their seat for two minutes. An edit in flight
  is resent and applied once. A refusal says why (different version with both identities, code
  expired, used, replaced, session full), and the guest tells "the host ended the session" apart
  from a network drop. Spec: [docs/spec/session-resume.md](docs/spec/session-resume.md).
- **Checks passed (Linux, local, simulated drops):** 109 multiplayer tests including full sessions
  online and on LAN with 150 ms latency and 64 KB/s, drops during an edit, an absence longer than
  the history, and a permanent drop.
- **Limits:** resuming does not survive a host that quits or crashes; one pending request is
  resent; an absence of more than 32 edits reloads a full copy.

## Heavy shared projects — 30 October 2026 (branch `feature/online-heavy`)

**Goal:** share projects with gigabytes of sound, online and on LAN.

- **Code written:** limits raised to 4096 sounds, 2 GB per sound and 16 GB per session. Sounds
  travel in 1 MB pieces read from and written to disk, each verified before being kept (BLAKE3
  tree), as raw bytes. Each computer keeps received sounds in **Sound cache** (20 GB at most,
  oldest removed first): a sound already received is never resent, and an interrupted transfer
  resumes from the last verified piece. When more than 32 MB is missing, the guest opens the
  project at once and the sounds arrive afterwards. Spec:
  [docs/spec/heavy-sessions.md](docs/spec/heavy-sessions.md).
- **Checks passed (Linux, local):** 118 multiplayer tests, including a 40 MB sound locally and
  online, a corrupt piece refused, resuming after a drop, hostile limits refused, cache cleanup.

## Link playback — 29 October 2026 (branch `feature/online-linked`)

**Goal:** everyone hears the same thing at the same moment, without streaming audio.

- **Code written:** a **Link playback** box in the multiplayer panel. Ticked, the buttons become
  **Play together**, **Stop together** and **Everyone from beat 16** and act on every computer.
  The host schedules the start 0.3 s ahead; each guest estimates the host's clock and starts at
  the same instant. Spec: [docs/spec/linked-playback.md](docs/spec/linked-playback.md).
- **Checks passed:** a local online session where host and guest starts match within 30 ms.
- **Limits:** sound-card latency is not compensated; tempo changes are not followed; the Studio
  transport bar and launched clips stay local.

## Online multiplayer by code — 29 October 2026 (branch `feature/online`)

**Owner's priority: play together over the Internet, not only on a local network.** The host
creates a session, copies a code and sends it; the other person, even 200 km away, pastes it and
joins. No account, no router settings.

- **Code written:** peer-to-peer transport with [iroh](https://github.com/n0-computer/iroh)
  (encrypted QUIC, hole punching, a public relay fallback that reads nothing). **Over the
  Internet** is the default; **On this local network** remains. Spec:
  [docs/spec/online-sessions.md](docs/spec/online-sessions.md).
- **Checks passed (Linux, local):** 92 multiplayer tests including full four-person online
  sessions; strict clippy; `cargo deny`.

## Before 29 October 2026 (summary)

- **Owner's direction (29 October):** finish multiplayer inside Suhina's codebase and keep it
  working as Suhina evolves; participants install and update the same app. DJ deck/tempo/loop
  development started by mistake was stopped.
- **LAN multiplayer beta:** one host and up to three guests on a local network, shared
  MIDI/clips/scenes/tracks, FM Synth and Utility, shared loaded sounds and published takes;
  3,308 Rust tests passed at `91cf0521`.
- **DJ workspace:** local catalog and playlists, tempo/key analysis with rbxport code, cue points,
  waveform, beat grid editing, two A/B decks with crossfader.
- **Upstream Suhina** changes are merged regularly into the fork; `main` mirrors upstream.

## Next steps

1. A real session between homes with friends, on the released build.
2. Test the Rekordbox USB export on a real CDJ/XDJ.
3. Adjusting third-party plug-in knobs together.
