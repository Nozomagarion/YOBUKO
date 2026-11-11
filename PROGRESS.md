# Where YOBUKO stands

This is the project's single progress file: what changed, why, what was checked and what comes
next. Newest first. Planned work, written code, packaged builds and real-world tests are kept
apart. (Until 1 November 2026 this file was kept in French as `SUIVI_PROJET.md`; the full
original is in the git history.)

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
