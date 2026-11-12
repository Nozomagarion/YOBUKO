---
description: The fork's Studio and DJ workspaces (two decks and a mixer in YOBUKO's window, driven by mouse or a DDJ-FLX4), local playlists and cues, rbxport analysis, engine audition and waveform preparation
read_when: DJ decks, DJ mixer, DDJ-FLX4, DJ library, playlists, cue points, audition, waveform preparation, rbxport integration, render a project for DJ preparation
type: spec
status: current
last_verified: 2026-11-11
covers:
  - crates/engine/src/decks.rs
  - crates/engine/src/deck_voice.rs
  - crates/engine/src/deck_fx.rs
  - crates/engine/src/deck_record.rs
  - crates/engine/src/decks_tests.rs
  - crates/session/src/dj_decks.rs
  - crates/session/src/dj_sampler.rs
  - crates/session/src/dj_colors.rs
  - crates/session/src/dj_record.rs
  - crates/session/src/dj_history.rs
  - crates/session/src/dj_auto.rs
  - crates/session/tests/dj_decks.rs
  - crates/dj
  - crates/session/src/dj.rs
  - crates/ui/src/app/dj.rs
  - crates/ui/src/app/dj_tests.rs
  - crates/ui/src/app/dj_decks.rs
  - crates/ui/src/app/dj_decks_tests.rs
  - crates/ui/src/app/dj_flx4.rs
  - crates/ui/src/app/dj_deck_tools.rs
  - crates/ui/src/app/dj_deck_beats.rs
  - crates/ui/src/app/dj_fx.rs
  - crates/ui/src/app/dj_sampler.rs
  - crates/ui/src/app/dj_record.rs
  - crates/ui/src/app/dj_auto.rs
  - crates/ui/src/app/dj_mic.rs
  - crates/dj/src/playlist.rs
  - crates/ui/src/app/dj_library.rs
  - crates/engine/src/preview.rs
  - crates/midi/src/lib.rs
  - crates/session/tests/dj.rs
  - crates/media/src/waveform.rs
  - crates/session/src/dj_waveform.rs
  - crates/session/tests/dj_waveform.rs
  - crates/session/tests/dj_grid.rs
  - crates/session/src/dj_preview.rs
  - crates/session/tests/dj_audition.rs
  - crates/ui/src/interaction/pick.rs
  - third_party/rbxport-analysis
  - crates/ui/src/app/dj_rbxport.rs
  - crates/ui/src/app/usb_space.rs
  - crates/ui/src/app/usb_space_tests.rs
  - apps/suhina/src/rbxport_panel.rs
  - apps/suhina/src/rbxport_panel
  - third_party/rbxport-app/src-tauri/src/yobuko_embed.rs
  - crates/ui/src/app/menu_bar.rs
  - crates/ui/src/app/shell.rs
  - crates/ui/src/app/shell_tests.rs
  - crates/ui/src/kit/piou.rs
  - scripts/rbxport_build.sh
  - third_party/rbxport-app/VENDORED.md
---

# DJ workspace

## Spaces, and the DJ space

The app has four spaces, each a tab at the right end of the menu bar (or of its own row when
the menu bar is hidden): blue Studio, green Multiplayer, pink DJ and amber USB. The tab of the
space showing is filled in its colour, and clicking another switches to it; View > Studio
workspace / Multiplayer / DJ library / USB export do the same from the native menu
(`crates/ui/src/app/shell.rs`). Left of the tabs sit YOBUKO's seal, Piou the mascot
(`crates/ui/src/kit/piou.rs`) and, while a shared session runs, its state ("Room open · 2
friends", "Reconnecting…"), which opens Multiplayer when clicked.

Piou's mood follows the app: it nods on the beat while the project plays, calls while a room
waits for friends, hops when one arrives or a track reaches the DJ library, worries (headphones
greyed) while the connection is being restored, and sleeps after five minutes without a key or
a click. Its headphones take the space's colour. What happens away from the page that would say
it (a friend joining while the studio shows, the connection dropping) is also told in a note
beside Piou at the bottom right of the window for four seconds; the note takes no click. With
animations off Piou is still.

The DJ space mixes in YOBUKO's own window (`crates/ui/src/app/dj_decks.rs`); nothing opens
beside it. Entering it stops the active project's transport and sample preview, hides the
studio's control bar, and shows:

- **Two decks**, A (pink) on the left and B (mint) on the right. Each shows its track's title
  and tempo (the track's BPM times the deck's speed), the whole track with the part played
  dimmed (a click or a drag goes there), a close waveform of eight seconds around the playhead,
  which stays in the middle, the time played and left, **Cue**, **Play** and **Sync**, and a
  **Tempo** slider (±8 %, ±16 % or ±50 %: a click on the range button goes to the next, and the
  range widens by itself when the speed is beyond; a double click returns to the track's own
  speed). The pitch follows the speed. Both waveforms mark the hot cues in their pads' colours,
  the loop (green while it plays), LOOP IN waiting for LOOP OUT, and the cue point.
  - **EJECT** takes the track out of a deck that does not play; **|◀** goes back to the start;
    **−** and **+**, held, nudge a playing deck 4 % slower or faster to bring the beats
    together (`dj.deck.bend`).
  - **Hot cues 1–8**: a pad with no cue puts one where the deck is; a lit pad jumps there;
    a right-click deletes. They are the track's cue points in the DJ library named
    `Hot cue 1` … `Hot cue 8` (`dj.cue.add`, `dj.cue.delete`), so they stay with the track.
  - **Loops**: **IN** marks where a loop starts, **OUT** ends it there and plays it,
    **RELOOP / EXIT** lets a playing loop go or plays the last one again (from its start when
    the deck is outside it). **LOOP n** is a loop of n beats from the nearest beat (pressed again,
    it lets go); **/2** and **×2** halve or double it, and the loop that plays with it. A loop
    crosses its end without a seam: the last 2 ms fade into what follows its start.
  - **Beat jump**: **◀** and **▶** jump n beats back or on (a loop that plays moves with it);
    a click on **JUMP n** doubles n, a right-click halves it. Beats come from the track's beat
    grid when it has one, else from its BPM; without either, beat loops and jumps are off.
  - **Cue**: stopped, it sets the cue point where the deck is; playing, the deck goes back to
    the cue point and stops there. Loading a track puts the cue point at its start.
  - **Sync**: the deck plays at the other deck's tempo. Both tracks need a known BPM.
- **The mixer** between them: per deck a **Trim** knob (−12 to +12 dB, the middle 0), **High**, **Mid** and **Low** knobs (the middle is flat,
  the right end +6 dB, the left end −40 dB, a cut that falls along a curve so the first half of
  the turn trims), a **Filter** knob (left of the middle a low-pass closing down to 150 Hz,
  right of it a high-pass opening up to 8 kHz, the middle off), **Phones** (the deck in the
  headphones, on a separate cue output only), a level fader and a meter beside it (after the
  trim, EQ and fader, before the crossfader: green, yellow from −6 dB, red from 0 dB); in the
  middle **Master** (the decks' level on the main output) with a stereo meter, the headphones'
  level and **Cue/Mst** (what the headphones hear: the cued decks at the left, the master at
  the right, a blend between); the crossfader under them. A knob is dragged up or down; a
  double click puts it back.
- **The library** under them: each track with its BPM, key and length, and **Load A** /
  **Load B** (refused on a deck that plays). A click selects a row. **Search** keeps the
  tracks whose title, key or BPM hold every word typed; a click on **TITLE**, **BPM**, **KEY**
  or **TIME** sorts by it, again the other way, a third time back to the library's order. The
  **Playlists** on the left show all tracks or one playlist; **+ New playlist** makes one, and a
  row's right-click menu loads it on a deck, adds it to a playlist or removes it from the one
  shown; a playlist's right-click deletes it. Audio files and folders
  dropped anywhere on the page, **Add my Music folder** (the person's `Music` folder) and
  **Send this project to DJ…** (a render) go into it, analysed one file at a time, with the
  count said on the page. A folder gives at most 1000 audio files; files already in the
  library are left out.

Nothing in the DJ space opens another program's window: Mixxx is not started. Switching to
another space leaves the decks playing.

### The DDJ-FLX4

While the DJ space shows, a MIDI port whose name contains `FLX4` is taken for the decks:
`LiveInput::capture` hands what it sends, unread, to the window, and it no longer plays an
armed track. Going to another space gives it back. The MIDI numbers are those of the
open-source DDJ-FLX4 mapping for Mixxx by Robert904 and ElHanko (MIT), vendored in
`third_party/mixxx-ddj-flx4-mapping` (`crates/ui/src/app/dj_flx4.rs`):

| Control | Message | Does |
|---|---|---|
| PLAY/PAUSE, CUE, BEAT SYNC | note 0x0B, 0x0C, 0x58 on channel 1 (deck A) or 2 (deck B) | Play, Cue, Sync |
| Mixer CUE | note 0x54, channel 1 or 2 | Phones |
| LOAD | note 0x46 (A) or 0x47 (B), channel 7 | loads the selected library row |
| BROWSE | CC 0x40, channel 7, relative | moves the selected row |
| Channel fader | CC 0x13, channel 1 or 2 | level |
| EQ HI, MID, LOW | CC 0x07, 0x0B, 0x0F, channel 1 or 2 | High, Mid, Low |
| CFX (filter) | CC 0x17 (A) or 0x18 (B), channel 7 | Filter |
| TEMPO | CC 0x00, channel 1 or 2 | ±8 %, read as the mapping reads it |
| CROSSFADER | CC 0x1F, channel 7 | crossfader |
| JOG (top or side) | CC 0x21–0x23, channel 1 or 2, 64 still | playing, a nudge of 1 % of the speed a step, for 120 ms after the last; stopped, a scrub of 20 ms a step |
| SHIFT | note 0x3F, channel 1 or 2, read pressed and let go | held, a hot cue pad deletes |
| SHIFT + CUE | note 0x48, channel 1 or 2 | back to the start |
| TRIM | CC 0x04, channel 1 or 2 | Trim |
| HEADPHONES MIX | CC 0x0C, channel 7 | Cue/Mst |
| LOOP IN, LOOP OUT, RELOOP/EXIT | note 0x10, 0x11, 0x4D (0x50 with SHIFT), channel 1 or 2 | IN, OUT, RELOOP / EXIT |
| CUE/LOOP CALL ◀ ▶ | note 0x51, 0x53, channel 1 or 2 | /2, ×2 |
| Quick jumps | note 0x3E, 0x3D, channel 1 or 2 | 16 beats back, on |
| Pads, HOT CUE mode | notes 0x00–0x07 on channel 8 (A) or 10 (B); 9 or 11 with SHIFT | hot cue 1–8; with SHIFT, deleted |
| Pads, BEAT LOOP mode | notes 0x60–0x67, channel 8 or 10 | loops of 1/4, 1/2, 1, 2, 4, 8, 16, 32 beats |
| Pads, BEAT JUMP mode | notes 0x20–0x27, channel 8 or 10 | 1, 2, 4 and 32 beats back (left) and on (right) |
| Pads, SAMPLER mode | notes 0x30–0x37, channel 8 or 10; 9 or 11 with SHIFT | plays sampler slot 1–8 from its start (an empty slot loads the selected library row); with SHIFT, stops it |
| SHIFT + mixer CUE (TAP) | note 0x68, channel 1 or 2 | taps the deck's tempo |
| BEAT FX SELECT (SHIFT: back) | note 0x63 (0x64), channel 5 | next (previous) effect in unit 1, slot 1 |
| BEAT ◀ ▶ | note 0x4A, 0x4B, channel 5 | halves, doubles unit 1's beats |
| BEAT FX ON/OFF (SHIFT) | note 0x47 (0x43), channel 5 or 6 | switches unit 1, slot 1 (switches all of unit 1 off) |
| BEAT FX CH SELECT | note 0x10 channel 5, 0x11 channel 6; 0 when it leaves | puts unit 1 on deck A, deck B or both (the master when neither) |
| BEAT FX LEVEL/DEPTH | CC 0x02, channel 5 or 6 | unit 1's mix; with SHIFT, slot 1's knob |

Only the most significant 7 bits of the 14-bit controls are read. The KEYBOARD, PAD FX and pitch
pad modes and the jog wheels' touch are not read yet.
The controller's lights are not driven. Not verified on the hardware yet.

The USB space shows rbxport inside YOBUKO's window (`crates/ui/src/app/usb_space.rs`), with
**Add this project to the library…** and **Share library now** in its strip. YOBUKO ships its
own copy of rbxport, built from `third_party/rbxport-app` by `scripts/rbxport_build.sh`:
`YOBUKO.app/Contents/Helpers/rbxport.app` on macOS, `rbxport/rbxport[.exe]` beside the program
on Windows and Linux. Without it (a development build) an rbxport installed on its own is used;
without any, the space offers **Get rbxport**.

Entering the space starts rbxport with `YOBUKO_EMBED=<port>:<token>`
(`apps/suhina/src/rbxport_panel.rs`, `rbxport_panel/link.rs`). rbxport connects back to that
local port, sends the token, and keeps its own windows hidden and off the taskbar or Dock
(`third_party/rbxport-app/src-tauri/src/yobuko_embed.rs`); they load a bridge page
(`public/yobuko-bridge.html`) instead of the interface. YOBUKO opens one child web view per
rbxport window over the space's rectangle and serves rbxport's interface into it from rbxport,
through the link. Messages are a 4-byte big-endian length and JSON:

- YOBUKO to rbxport: `invoke` (a command the page called, run in the matching hidden window),
  `asset` (a file of the interface or an `rbl://` request), `menu` (a menu item picked),
  `closed` (a tab closed), `quit`.
- rbxport to YOBUKO: `hello`, `reply`, `callback` (events and channels), `page` (a window
  opened: Preferences, sync, log and report become tabs), `focus`, `gone`, `eval`, `menu` (its
  menu tree) and `asset` answers.

The page runs with a stand-in for Tauri's page interface (`rbxport_panel/shim.js`): window
calls act on the tab, dialogs are YOBUKO's file and message dialogs, a drop on the page reaches
rbxport's drop handling and a drag out of a track list goes through YOBUKO's drag-out. While
the space is shown, rbxport's menus are in YOBUKO's menu bar (the system bar on macOS, or the
strip when the window has no menu bar) and its shortcuts act. The web view hides under an open
menu or popup and when another space is chosen; rbxport keeps running until YOBUKO quits. When
YOBUKO's look is on, rbxport's controls, cards and dialogs get the DJ space's rounded shapes
(`src/styles/yobuko-soft.css`).

A render sent to DJ is also written as a `.paths` request into `<data dir>/YOBUKO/rbxport-inbox`,
which YOBUKO's copy of rbxport imports (`third_party/rbxport-app/src-tauri/src/yobuko.rs`).

## Commands and storage

`dj.library` polls the worker and returns tracks, playlists, cues, revision, preview status, busy,
error, analysis and format.
`dj.import` queues one audio file, with an optional `sourceProject` path. All DJ operations are Library
commands on Projects and are denied to MCP agents: managing the local DJ collection is the
person's action. Typed file parameters retain their path classification. No Rekordbox database
is opened or written. The CLI sees the same registry; it needs a configured library folder
and repeated library polling to wait for an asynchronous job.

The first library request starts one `suhina-dj` worker. It reads `dj-library.json` beside
`browser.json`, then handles one import at a time. Open, decode, analyse and save run on that
worker. The UI receives snapshots over a channel. Changing the configured file library drops
the old worker, clears library selections/drafts and starts the next catalog lazily. An invalid existing catalog is reported
without overwriting it. A failed import does not prevent another file from being attempted.

The initial catalog format 1 contains at most 2000 tracks and is at most 4 MiB. Each track has a local id,
absolute audio path, title, optional source project, duration, optional BPM and key. Duplicate
ids/paths and unsupported formats are refused. Save validates the complete next catalog, writes
a unique sibling temporary file, flushes it and persists it over the destination. Only after
a successful save is the in-memory catalog replaced. Audio remains at its source path.

The importer accepts WAV, AIFF, FLAC, MP3 and Ogg, up to 20 minutes, mono or stereo, at up to
96 kHz. It uses Suhina's bounded decoder and averages channels for rbxport analysis. It rejects
nonfinite samples and files whose size or modification time changes during analysis. Reimport
of the same canonical path retains its id and source-project association. Unknown tempo/key
remain unknown. Analysis accuracy is not a promise of a correct beat grid for every song.

## Rendering from Studio

With a project open, DJ offers **Render project to DJ library…**. This opens the normal export
dialog; **Export** uses the existing project.export worker. On success its exact output path
and original saved source project are queued for analysis, even if another tab is now selected.
A failed/cancelled render adds nothing. A normal File > Export does not add to the DJ library.
An unsupported/oversized DJ import reports an error; the completed audio render stays on disk.
The local catalog is separate from the .suhina file, so this is not a project-format migration.

## Integrated subset

Only rbxport's offline `rbl-analysis` source is integrated, at the exact commit and per-file
hashes in `fork-sources.json`. The original Rust sources are unchanged; its Cargo manifest is
adapted to avoid the other workspace's GUI, databases and development dependencies. Original
licence files are preserved and included in distribution packages.

Decks, USB export, LINK and shared DJ collections are not implemented by this slice.
The later fork extensions below add cue editing and waveform preparation. The DJ workspace
adds no network listener; LAN collaboration has its separate [sharing contract](session-sharing.md).

## Check in a development build

1. Open Suhina with no project. Click **DJ** at the right of the menu bar. **DJ library** and **Import
   audio…** appear. Click Import, choose a short WAV you own, and wait for Loading / analysing
   to finish. Its title, duration and any detected BPM/key appear. A failure appears as text.
2. Import the same file again. One row remains. Quit and reopen with the same settings folder;
   DJ shows the saved row. Neither import changes the audio file.
3. Click Studio. Use File > Open to open a project you own, return to DJ and choose **Render
   project to DJ library…**. Choose a new WAV destination, then Export. The completed render
   appears in DJ. For a saved project, **Open source project** reopens that project.
4. Repeat through File > Export instead; this ordinary export adds no row. Cancel the export
   dialog and check that it adds nothing. Choose a missing/non-audio file and check the visible
   error and preservation of existing rows. Return to Studio and verify the project is intact.

## Local playlists (fork extension)

The person can create, rename and delete named playlists, add catalog tracks once, remove them
and move them to a final one-based position. Playlists hold stable track ids in order; reanalysis
keeps those ids. Removing a playlist or an entry never moves, edits or deletes audio or projects.
The collection selector offers All tracks and the saved playlists; each list is paginated.
Deletion needs an explicit confirmation in the UI. Draft names are tied to the catalog revision:
a changed revision requires refreshing the draft before saving it. Shared DJ playlists are deferred.

Catalog format 2 introduced playlists, a monotonically checked revision and a non-reused playlist-id
counter. Limits are 128 playlists, 128 bytes per trimmed nonempty name without control characters,
2000 unique entries per playlist and 16000 entries in total, still within the 4 MiB catalog limit.
Unknown ids, duplicate entries, unknown fields, invalid counters and unsupported versions fail.
Format 1 opens without writing; the first successful mutation keeps a byte-exact sibling
`dj-library-v1-backup-*.json` before publishing the current catalog format. A malformed catalog is never replaced.

All writes run on the catalog worker through a bounded channel. The `dj.playlist.create`,
`.rename`, `.delete`, `.addTrack`, `.removeTrack` and `.moveTrack` commands require the displayed
revision (plus playlist/track ids and name/position as appropriate). The worker rejects stale
revisions. Before committing, it takes a short advisory file lock and compares the complete
current disk catalog with its expected snapshot, so two cooperating app instances cannot silently
overwrite each other. A failure keeps the previous saved and in-memory catalogs. `dj.reload`
reloads the local catalog when idle, including after a repaired file or another instance's edit.
These commands have the same person-only Library policy as import; no audio callback does I/O.
The snapshots include playlists and revision; a completed creation reports its new id once.

Acceptance: create two playlists, add several imported/rendered tracks, change their order and
name, remove an entry and delete one playlist; reopen with the same settings and verify the
remaining ordered list, original audio and source-project links. Also test legacy migration,
stale edits, malformed data, failed saves and concurrent app instances. Physical machine tests
remain separate from automated model, worker and interface tests.

## Audition and named cue points (fork extension)

Selecting Prepare beside a catalog track opens a local preparation panel without playing it.
Play from position and Stop audition use the existing engine preview voice at −6 dB, routed to
the configured cue/headphone output or otherwise the main output. A time slider sets the next
start position; it does not rewrite audio. The actual engine monitor reports playback position.
The Studio project/undo history and LAN transport formats are unchanged. Leaving DJ, changing
settings folders, reloading the catalog or pressing Stop cancels a pending audition; a late
worker result cannot start playback. Changing the active project while loading also cancels it.
No microphone or additional output is opened by Stop. Missing output/files are visible errors.

`dj.preview.play` takes track and from (seconds); `dj.preview.stop` is independent of catalog
busy status. Audio validation/decode and a one-file cache live on the existing bounded DJ worker.
Metadata changes invalidate the cache; decoded duration must match the catalog within 0.05 s,
otherwise reimport is required. Only validated mono/stereo files up to 20 min/96 kHz are accepted.
Decoded Arc data is sent to the existing controller on the caller thread, never from the audio
callback. The library snapshot includes audition track, playing, loading, position and error.
No output is opened until the person explicitly requests Play.

Catalog format 3 adds per-track cue sets with stable, non-reused ids and named positions in
seconds, sorted by (time,id). Limits are 16 cues per track, 8000 total, names of 1–64 trimmed
bytes without control characters, finite positions >=0 and strictly before the stored duration.
`dj.cue.add`, `.rename`, `.move`, `.delete` require the displayed catalog revision and track id;
editing existing cues also requires cue id. Writes use the existing checked atomic publication.
The editor freezes its draft revision; stale drafts need an explicit refresh. Cue deletion is
confirmed. A failed write preserves the draft and saved catalog. Reanalysis preserves cues and
playlist references; shortening audio past a cue refuses reimport rather than silently deleting it.

Formats 1 and 2 open without writes. The first mutation saves an exact sibling backup with
the original format in its filename before publishing the current format, preserving playlists/counters. Format 3 introduced cues;
the grid extension below writes format 4 and also backs up format 3.
Future/unknown fields and invalid cue references are refused. Play/Stop never persist a catalog.
Cue sets are local, not shared over LAN and not exported to a proprietary database or hardware.
Quantised cues, decks and USB/LINK export remain deferred. Constant beat-grid editing is
described below. Waveform preparation
is described below.

Acceptance: audition generated audio through an offline renderer, seek from a cue, stop while
loading, leave DJ and switch projects without late playback; output failures must keep projects
intact. Create/rename/move/delete cues with actual UI buttons, preserve their names/positions
across playlists and restart, refuse stale/invalid edits and verify legacy backups byte-for-byte.


## Waveform preparation (fork extension)

Prepare queues `dj.waveform.load` with a track id. The existing bounded DJ worker reuses the
metadata-checked audition decoder/cache and computes Suhina's existing multiresolution
min/max/RMS summary, moved without a second algorithm to `media::waveform`. UI drawing still
uses `kit::waveform`, including stereo lanes and the existing linear/dB preference. Opening
preparation does not open an audio device, play sound or write the catalog/audio/project.

The library snapshot reports waveform track/loading/error and frame/rate/lane metadata. Native
clients obtain the immutable Arc summary through Projects; raw samples are not serialized in JSON.
Only one prepared waveform is retained. Loading is bounded by the existing 20 minute, mono/stereo,
96 kHz audition limits. Clear/reload/settings changes invalidate the pending request; a late reply
cannot replace a newer selection. Import invalidates a prepared summary to avoid keeping a picture
of replaced audio. Reopening Prepare rechecks metadata; no filesystem polling occurs while idle.
`dj.waveform.clear` works while busy, releases the prepared summary and never opens output.
Both commands retain the person-only Library policy.

The full-track waveform shows named cue markers, the chosen next start position and actual
playback position. Clicking or dragging it changes only the local start-position draft; Play from
position explicitly auditions there. Keyboard users retain the time slider. An endpoint click is
clamped before the track end. Read/decoding errors are displayed with a Reload waveform button;
cues remain usable. Closing preparation or leaving DJ cancels the view request and discards it.
No waveform is persisted, no catalog migration or network format change is introduced. Zoom,
variable-tempo grids, quantisation, decks and hardware export remain separate work. Constant
beat-grid editing is the following extension.

Acceptance: generated stereo audio with independent impulses preserves both lanes and known
positions, waveform loading opens no output and writes no files, malformed/missing/changed audio
reports an error, clear during load prevents a late result, reload resets state, cue markers and
click/drag seek share the same seconds mapping, playback starts only on Play. Existing waveform
geometry/budget tests remain applicable; capture the preparation UI for visual review.


### Check waveform preparation

1. Open a development build with no project. For a repeatable audio file, run this in a terminal
   (Python 3 standard library only); it creates an eight-second stereo WAV in the temporary folder:

   ```sh
   python3 - <<'PYTHON'
   import math, os, struct, tempfile, wave
   path = os.path.join(tempfile.gettempdir(), "suhina-dj-waveform-check.wav")
   with wave.open(path, "wb") as out:
       out.setnchannels(2)
       out.setsampwidth(2)
       out.setframerate(8000)
       samples = bytearray()
       for i in range(64000):
           t = i / 8000
           gain = 0.2 if t < 2 else 0.6 if t < 4 else 0.35
           samples += struct.pack("<hh", *(int(32767 * gain * math.sin(math.tau * f * t)) for f in (110, 165)))
       out.writeframes(samples)
   print(path)
   PYTHON
   ```

2. Click **DJ**, then **Import audio…**, choose the printed file and wait for analysis. Click its
   **Prepare** button. Two waveform lanes appear, quieter for the first two seconds, louder for
   the next two. No sound starts. The numeric start-position slider remains available.
3. Click the middle of the waveform. **Start position seconds** becomes approximately 4.000 s;
   dragging changes it without sound. Click **Play from position** to listen from there and see
   the green position advance. Click **Stop audition**; the sound stops and the selected start stays.
4. Enter a cue name and a cue position, then **Add cue**. A named marker appears at that time.
   Change its position with **Move cue to position** and verify the marker moves as well.
5. Click **Close preparation**, reopen Prepare, then **Reload waveform**. A fresh waveform loads;
   cues remain. Leave via **Studio** during loading and verify no late audio starts. Delete the
   generated WAV outside the app, then reopen Prepare: a visible waveform error appears instead
   of a stale drawing. Restore/recreate the file and use **Reload waveform** to recover.

The automated UI checks cover preparation, a centre click, no implicit playback, close and reload
errors; GPU capture covers the two lanes and markers. Actual headphone routing still needs the
separate physical test.


## Editable constant beat grid (fork extension)

The local preparation panel stores an optional grid per track: a finite 20–400 BPM tempo and
`first_beat` seconds (finite, nonnegative, strictly before duration). This anchor is beat 1 of
bar 1; this first slice uses four beats per bar and constant tempo, with no ticks before the
anchor. It does not warp, quantise, change playback speed or edit audio/cues. The measured
`Track.bpm` remains separate. Reanalysis keeps the manual grid and refuses shortening audio past
its anchor rather than removing it. At most one grid exists per catalog track, up to 2000 grids.

`dj.grid.set` takes track, displayed revision, bpm and firstBeat seconds. `dj.grid.clear` takes
track and revision. Both are person-only Library commands on the existing bounded catalog
worker, with checked revision, disk snapshot comparison and atomic publication. Invalid,
unknown, stale or failed edits preserve disk/model. Clear requires an existing grid and UI
confirmation; it does not delete cues, playlist entries or audio.

Catalog format 4 adds a required grids array. Formats 1/2/3 open without writes, preserving all
track, playlist and cue identifiers and counters; the first successful mutation keeps an exact
sibling `dj-library-vN-backup-*.json` before publishing format 4. Unknown fields, duplicate or
unknown grid tracks, invalid numbers and future formats are rejected. Earlier applications
cannot read format 4: retain backups and previous audio/project files for recovery.

Prepare distinguishes analysed tempo and the saved grid. The local draft initially copies the
saved grid or uses analysed BPM (120 when unknown) and anchor zero. Preview beat grid or editing
the draft shows an explicitly unsaved grid on the waveform; Save beat grid persists it. Use start
as first beat copies the chosen start position, without playing audio. Half/Double tempo operate
only within bounds. Changes elsewhere make drafts stale: explicit Refresh grid draft reloads
saved values, never retargeting an old draft to a newly selected track. Failed writes keep drafts;
successful writes reset them. Reload restores the saved grid; deletion has its own confirmation.

Grid tick generation is pure, bounded and in file seconds. Drawing uses at most 2048 lines and
an additional width budget; dense overviews show a subset of bar starts with original bar numbers.
Cues, selected start and actual playback cursor stay above the ticks. Draft and saved grids are
visually distinct. No audio device is opened by grid operations. Zoom, variable tempo/meter,
quantised cue playback and hardware export remain later work.

The distribution marker retains family `dj_catalog=1` and adds `dj_catalog_read_min=1`,
`dj_catalog_read_max=4`, `dj_catalog_write=4`. The new updater requires a candidate to read all
formats 1–4 and write a format >=4 within its declared reader range, rejecting older packages
without these capabilities before replacing files. Older updater clients can accept the new
package through the unchanged family contract; no shared update channel is published here.

Acceptance: independent impulse fixtures at known seconds, exact tick/bar positions, bounded
long-track drawing, roundtrip and byte-exact v3 backup, stale/invalid/locked/concurrent writes,
reimport preservation and shortening refusal, actual UI preview/save/refresh/delete/restart,
agent denial, no output or project mutations and explicit package compatibility tests.


### Check grid editing

1. Use the generated eight-second WAV from **Check waveform preparation** above. Open Suhina
   without a project, click **DJ**, **Import audio…**, select that file, wait, then **Prepare**.
   The panel distinguishes analysed BPM and **no saved grid**. Nothing plays.
2. Set **Grid BPM** to 120 and **First beat seconds** to 0.250. Click **Preview beat grid**.
   Amber ticks start at 0.250, 0.750, 1.250 seconds; bar 2 begins at 2.250. The draft is not saved.
3. Click **Save beat grid**. The saved summary shows 120 BPM/0.250 s; ticks turn blue. Quit/reopen
   with the same library folder, click DJ and Prepare again: the values and ticks remain.
4. Click a new position on the waveform, then **Use start as first beat**. Only the draft moves.
   **Half grid tempo** and **Double grid tempo** adjust that draft within 20–400 BPM;
   **Use analysed tempo** copies analysis without modifying its stored value. Save explicitly.
5. Make a cue edit while a grid draft exists. **Save beat grid** becomes disabled until
   **Refresh grid draft**, which reloads the saved values. Changing tracks starts a fresh draft.
6. Click **Delete beat grid…**, then **Cancel grid deletion**: saved values remain. Repeat and
   **Confirm delete grid**: ticks and saved grid disappear; cues, playlists and audio remain.

Check legacy migration on a test copy of a format-3 catalog: opening/previewing writes nothing,
and the first successful edit leaves an exact `dj-library-v3-backup-*.json`. The automatic
migration and concurrent-write tests cover refusal paths; do not experiment on a sole real catalog.


## Two local performance decks (fork extension)

A/B are two independent deck players (`crates/engine/src/deck_voice.rs`, YOBUKO's own, so
Suhina's preview voice stays as upstream has it) on the existing Renderer and audio output,
outside the Studio graph/undo/project file. They read with the model's interpolating
`AudioData::frame_at` and the engine's 2 ms click guard. Each has its own bounded
command/retirement queues and monitor;
no decoding, allocation, locks, file access or destruction of audio occurs on the callback.
Pause retains the exact source position; Play resumes the same source or starts again after
end/stop. Seek is in source seconds, before the decoded end. Stop rewinds; Clear also unloads.

Load A/B queues decode on the existing DJ worker, without opening output or starting audio.
Files share audition validation/cache, with a 384 MiB decoded sample budget per deck (a
16-minute stereo track at 48 kHz; two loaded decks may retain 768 MiB, and one pending load
another 384 MiB). The worker computes the deck's waveform with its audio
(`Projects::dj_deck_peaks`), for the window to draw. The bounded decoder
checks headers and accumulated decoded samples. The catalog's wider import/audition limits
remain unchanged. A failed replacement keeps the previous loaded source stopped and reports
its error; a token prevents cancelled/old worker results from retargeting another load.

Only Play may open the chosen output, which remains the active Session output or the existing
empty preview Session when no project is open. A block-smoothed linear crossfader (0=A, 1=B),
channel levels 0–1, a fixed −6 dB of headroom and the master level (0–1, smoothed) feed main.
A single bounded mixer message changes both sides, the master and the headphones together. Each deck plays at its own speed, 0.5 to 2 (`dj.deck.rate {deck,rate}`; the
pitch follows unless the key is locked, see part 2 below), and through its own low shelf (250 Hz), mid bell (1 kHz) and high
shelf (3 kHz), −40 to +6 dB, then a filter (`dj.deck.tone {deck,low,mid,high,filter}`), before
its level; a flat deck skips them; the trim (−12 to +12 dB, `trim` in `dj.deck.tone`, kept when
left out) is a gain with the fader. `dj.deck.bend {deck,amount}` nudges a deck's speed by
−0.25 … 0.25 on top of its rate until it is sent 0. `dj.deck.loop {deck,from?,to?,on?}` sets a
loop in source seconds (both or neither, at least 10 ms) and plays it (`on`, true when left
out) or lets it go; a loop let go is kept for reloop, and loading a track clears it. The
player rounds a loop at its end, cross-fading its last 2 ms into its start; a seek beyond the
loop plays on from there. Speed, tone and the loop are kept per deck in the session and
handed to the engine on Play. Meters: each deck's peak after its tone, trim and fader, and the
master's, kept as the loudest sample since the last read (`DeckMeter::take`); the library
snapshot reads them (`meter`, `masterMeter`), so a read resets them. Key lock, effects and the
sampler are described in the next sections, recording, the microphone and Auto DJ in part 3;
no scratching is included.
CUE A/B is post-channel level, pre-crossfader, on the separate configured cue pair only; absence
of a separate pair never leaks an otherwise muted deck into main. The headphones hear the cued
decks times `phones × (1 − phonesMix)` and the master times `phones × phonesMix`; the existing
cue volume scales it all. Source samples and mix controls are sanitised/bounded; summing decks is not a general limiter.

Commands: `dj.deck.load {deck,track}`, `.play`, `.pause`, `.stop`, `.clear` (deck A/B), `.seek
{deck,from}`, `.rate {deck,rate}`, `.tone {deck,low,mid,high,filter}`, `.stopAll {}` and
`dj.mixer.set {crossfade,levelA,levelB,cueA,cueB,master?,phones?,phonesMix?}` (the last three
keep their values when left out), `dj.deck.loop {deck,from?,to?,on?}` and
`dj.deck.bend {deck,amount}`. These are
person-only Library commands. Playback controls and mixer work while another worker job is
busy. Pausing/stopping/clearing never opens output; full queues are visible errors and leaving
DJ retries stopping rather than claiming silence. Library snapshots include loaded metadata, key, trim, the loop, the meters,
source position, playing/loading and per-deck errors plus mixer values.

`dj.deck.stopAll` stops both decks and cancels pending loads. Switching project/output invalidates
old engine bindings and requires another explicit Play; it cannot start a late decoded result.
Reloading the library or changing its folder stops and clears decks; stop state is retained
across a folder reset if a queue is full. Deck choices and mixer are transient, not catalog
metadata: project v4, catalog v4 and LAN protocol 13/portable v3 remain unchanged.

Acceptance: two different authored signals at their own rates/positions, pause/resume/seek,
EOF/replay, independent stop, crossfade endpoints/midpoint and smoothing, cue before fader and
no main leakage without cue outputs. Check stop/loading/context cancellation, missing/oversized
files, output errors, queue pressure/retirement, no project/catalog writes and actual UI controls.
Offline Renderer tests are the audio oracle; physical headphones and two computers remain separate.


### Check A/B performance

1. Open Suhina with no project. Generate the eight-second WAV using **Check waveform preparation**
   above, click **DJ**, **Import audio…**, select the printed file and wait for analysis.
   In its library row click **Load A** (accessible name **Load track 1 to deck A** when it is track 1).
   The **Performance · decks A / B** pane below the lists shows its title; loading makes no sound.
2. Click **Play deck A** in that pane. Position advances and the file plays. Click **Pause deck A**:
   position stops. Click **Play deck A** again: position resumes. At the end, Play starts again.
3. In the same library row click **Load B**. It loads silently while A continues. Click **Play deck B**:
   its position advances independently. Move **Crossfader A / B** from 0 to 1: 0 hears A only,
   1 hears B only, the middle mixes both. **Deck A level** and **Deck B level** adjust each side.
4. Pause A, move the crossfader to 1, then resume A: its position advances but no A sound briefly
   returns. Set **Deck A seek seconds** to 2, click **Seek deck A**: only A moves to that position.
   **Stop deck A** stops and rewinds it while B continues. **Unload deck A** clears its title.
5. Without a separate cue output pair, **Cue deck A/B** are disabled. Configuring that pair in
   Audio settings enables cues before the crossfader; actual headphone routing needs a separate
   physical test. The automated Renderer test checks no cue leakage into main without that pair.
6. Click **Studio** in the top bar: both decks stop. Return to **DJ**: no automatic restart.
   Starting a load then returning to Studio must likewise not play its late result. No project
   file or catalog edit is produced by deck controls. Missing files and the decoded 128 MiB cap
   give an error while keeping a previously loaded source stopped.

Sync matches tempos from the saved beat grid and aligns the beats (next section). Automated
UI checks use a real offline Renderer; the GPU image uses fixture metadata for visual review.

## Deck and mixer functions, part 2 (fork extension)

This part adds key lock, effects, beat-aligned sync with quantize, tap tempo, a sampler and
waveforms coloured by band. It lives in YOBUKO's own files, so upstream Suhina merges stay easy.

**Key lock and key.** `dj.deck.key {deck, lock?, shift?}` locks a deck's key (its pitch stays
when its speed changes) and moves it by −12 … +12 semitones (`shift`), each kept when left out
and handed to the engine on Play. The deck player stretches through `suhina_stretch` when the
key is moved, or when the key is locked and the speed is not 1; otherwise it reads the source
directly, as before. The stretcher's buffers are made when the deck is built, so `process`
never allocates; it is primed again on Play, Seek, a shift, Stop and a loop change, so the first
block starts at once and a loop wraps without a gap. The snapshot adds `keyLock` and
`keyShift`. The page shows the key (Camelot or musical), **KEY LOCK**, **KEY −**, **+**, and
**KEY SYNC**, which moves a deck's key to the other deck's, read from both tracks' keys.

**Beat sync, quantize, tap and grid.** Sync matches the tempo from each track's saved beat
grid, then moves the deck by the difference of the two beat phases, with `dj.deck.seek {deck,
by}` (a relative move from wherever the deck is when the move reaches the audio; `from` stays an
absolute position). **QUANTIZE** snaps the cue point, hot cue set, loop in/out and a hot cue
jump of a playing deck to the nearest beat of the grid. **TAP** (or SHIFT + CUE on the
controller) averages the last eight taps (from the fourth; a pause over 2 seconds starts again)
into the grid's tempo; **GRID**
puts the grid's first beat at the playhead and ◀ ▶ nudge it; both save through the existing
grid command. The waveforms draw the grid's beats when they are at least 6 pixels apart, bars
brighter.

**Effects.** Two effect units (`crates/engine/src/deck_fx.rs`), each with three slots in a row,
a dry/wet mix, a length in beats (1/16 … 32) at a tempo, and where it goes: deck A, deck B
(after its EQ and filter, before its fader) or the master. Effects: Echo, Ping Pong, Reverb
(Suhina's reverb device), Filter, Flanger, Phaser, Trans, Autopan, Bitcrusher, Distortion and
Noise. Each slot has an on switch and one knob. The control thread builds a changed effect and
hands it over; the audio thread only swaps it. Switching on or off fades over a block; Echo,
Ping Pong and Reverb keep ringing out after they are switched off, until they are quiet (at
most 10 seconds). `dj.fx.set {unit, slot?, effect?, on?, meta?, mix?, beats?, bpm?, deckA?,
deckB?, master?}` changes a unit, keeping what is left out; the snapshot adds `fx` (both units)
and `effects` (the list of ids and names). The page keeps each unit's tempo with the deck it
is on, or else a playing deck.

**Sampler.** Eight slots, each holding a library track that plays from its start straight into
the master, before the master effects and level, at its own level 0 … 1. `dj.sampler.load
{slot, track}` decodes on the DJ worker like a deck (it is refused while another DJ job runs);
`dj.sampler.play {slot}` opens the output like a deck's Play and starts the slot from its start,
again if it plays; `.stop`, `.clear` and `.gain {slot, gain}` stop it, empty it (keeping its
level) and set its level. The snapshot adds `samplers` (track, title, seconds, loading, playing,
position, gain, error). The page shows the eight slots under the decks when there is room
(**Sampler** toggles them); each deck's pads switch between hot cues and the sampler
(**HOT CUE** / **SAMPLER**); a library row's menu loads a track into a slot.

**Coloured waveforms.** When a deck loads, the DJ worker also measures the track's lows (below
250 Hz), mids and highs (above 2.5 kHz), split by two 24 dB/octave crossovers from
`suhina-dsp`, a hundred times a second, on one scale with the mids raised ×2 and the highs ×4
so a quiet hi-hat still shows (`crates/session/src/dj_colors.rs`, at most 200 000 columns). The waveforms colour each column
from them: red for kicks and bass, green for voices and synths, blue for hats; played columns
are dimmed.

### Check part 2

1. Load two tracks with different tempos on A and B and play both. On deck A click **KEY LOCK**,
   then move its tempo: the speed changes, the pitch does not. Click **+** twice: the pitch
   rises two semitones. **KEY SYNC** moves A's key to B's.
2. Click **Sync** on deck B: its tempo matches A's and their kicks land together. Click
   **QUANTIZE**, then set a hot cue between two beats: it lands on the nearer beat.
3. Click **TAP** four times in time: the tempo shown follows the taps.
4. Click **Effects** in the DJ space's header. In **FX 1**, pick **Echo**, switch it on and put
   it on **A**: deck A echoes. Switch it off: the echo rings out. Put **FX 2** on the master with
   **Reverb**: both decks get reverb.
5. Right-click a library row, **Load in the sampler › Slot 1**. Click the slot: the track plays
   from its start over the decks; its slider changes its level. Switch deck A's pads to
   **SAMPLER**: pad S1 plays it too, right-click stops it.
6. The waveforms show red where the kicks are and blue on the hi-hats, with thin beat lines when
   zoomed in.

Automated: engine tests for the key lock, effects and the sampler on a real Renderer; session
tests for `dj.deck.key`, `dj.fx.set` and `dj.sampler.*`; UI tests for the key row, tap,
quantize, the effect units and the sampler; a unit test for the band colours. Not tried with a
real DDJ-FLX4 or real speakers.

## Deck and mixer functions, part 3 (fork extension)

**Recording the mix.** While a recording runs, the audio thread copies what the DJ master sends
(after the master effects and level) into a ring of four seconds made with the decks
(`crates/engine/src/deck_record.rs`); a writer thread empties it into a 16-bit stereo WAV
file. Quiet moments are kept, so the file runs as long as the recording. Frames the ring has
no room for are counted (`dropped`), never waited for; a WAV file stops short of 4 GB with an
error. `dj.record.start {folder}` makes the folder and a file `Mix YYYY-MM-DD HH-MM-SS.wav` in
it, and opens the output like Play; `dj.record.stop` finishes the file. Closing the app
finishes it too. The page's **REC** button records into `Music/YOBUKO Recordings` (the home
folder when there is no Music folder), lights up and counts the time; its tip names the last
file. The snapshot adds `recording {on, seconds, path, dropped, last, error}`.

**Crates.** A library list is a playlist or a crate (`kind: "crate"`, left out for a playlist,
so a library written before crates reads and writes unchanged; catalog format 4 stays).
`dj.crate.create {name, revision}` makes one; the playlist commands add, remove, rename and
delete in both. The library's side bar shows **PLAYLISTS** and **CRATES** with **+ PLAYLIST**
and **+ CRATE**, and a row's menu has **Add to playlist** and **Add to crate**.

**History.** Each track a deck starts playing after a load is written, with the time
(`HH:MM`), into that day's history in `dj-history.json` beside the library (format 1, at most
366 days of 2000 plays; titles kept, so a day still reads after a track leaves the library). It
is written at once through a file beside it, so it never waits for the library's worker; a
damaged file is reported and never written over. The library snapshot adds `history {days,
error}` (the newest 60 days, newest first); `dj.history.delete {day}` forgets a day. The side
bar shows **HISTORY**, one entry per day.

**Auto DJ and intro / outro.** Auto DJ (`crates/session/src/dj_auto.rs`) plays a queue of
library tracks on the two decks by itself. While one deck plays, the next track loads on the
other. At the playing track's outro (its cue named `Outro`, else the fade's length before its
end) the other deck starts at its intro (its cue named `Intro`, else its start), and the
crossfader moves across over the fade (0–30 s, 8 by default) as the outgoing deck plays on;
then that deck stops. It steps each time the page reads the library, from the positions the
audio reports; a deck it has just started counts as playing until the audio says so. A deck
that cannot play stops Auto DJ with the reason; an empty queue stops it once the last track
has started fading out. Commands: `dj.autodj.set {on?, fade?}`, `.add {track}`, `.remove
{position}`, `.clear`, `.skip`; the snapshot adds `autoDj {on, fade, queue, next, fading,
error}`. The library's **Auto DJ** list shows the queue, with **ON**, **SKIP** and **FADE** ◀ ▶
in its bar; a row's menu has **Add to Auto DJ** and, in the list, **Remove from Auto DJ**. A
deck's track overview menu sets or clears the intro and outro at the playhead; the waveforms
mark them.

**Microphone.** One audio input is mixed into the DJ master after the decks and the sampler,
before the master effects and level. While **TALK** is on the music ducks by `duck` (0 not at
all … 1 silent) and the microphone plays at `gain`, both smoothed over 10 ms; it has its own
meter. `dj.mic.set {on?, channel? (1–64), gain?, duck?}` keeps what is left out; switched on
it opens the output like Play, and Play hands it to the audio. The mixer card's **MIC** row has
TALK, the input (**IN n**, stepping 1–8), the level and the ducking.

**Not yet:** four decks (C and D), Auto DJ's intro and outro ends (only where they start is
read), its shuffle and random tracks, and recording to another format than WAV.

### Check part 3

1. Click **REC** at the top right of the DJ page, play a deck for ten seconds, click **REC**
   again. A file `Mix … .wav` is in your Music folder › YOBUKO Recordings and plays what you
   heard.
2. In the library's side bar click **+ CRATE**; right-click a track › **Add to crate** ›
   **Crate 1**. Click **Crate 1**: the track is there.
3. Play a track on a deck. A day appears under **HISTORY** with the track.
4. Right-click two tracks › **Add to Auto DJ**. Click **Auto DJ** in the side bar, then **ON**:
   the first track plays on A, the second loads on B and fades in before the first ends.
   Right-click a deck's overview › **Set the outro at the playhead** to choose where it starts.
5. With a microphone set as the audio input, click **TALK**: your voice is heard and the music
   drops; move **DUCK** and **LEVEL**.

Automated: engine tests for the recording tap and the microphone; session tests for
`dj.record.*`, the history and crates, Auto DJ from outro to intro, and `dj.mic.set`; UI tests
for REC, the side bar, Auto DJ, intro and outro, and the MIC row. Not tried with a real
microphone or speakers.
