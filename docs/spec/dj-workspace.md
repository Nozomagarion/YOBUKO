---
description: The fork's Studio and DJ workspaces, local playlists and cues, rbxport analysis, engine audition and waveform preparation
read_when: DJ library, playlists, cue points, audition, waveform preparation, rbxport integration, render a project for DJ preparation
type: spec
status: current
last_verified: 2026-10-26
covers:
  - crates/engine/src/decks.rs
  - crates/engine/src/decks_tests.rs
  - crates/session/src/dj_decks.rs
  - crates/session/tests/dj_decks.rs
  - crates/ui/src/app/dj_decks.rs
  - crates/ui/src/app/dj_decks_tests.rs
  - crates/dj
  - crates/session/src/dj.rs
  - crates/ui/src/app/dj.rs
  - crates/ui/src/app/dj_tests.rs
  - crates/ui/src/app/dj_playlists.rs
  - crates/ui/src/app/dj_playlist_tests.rs
  - crates/session/tests/dj.rs
  - crates/media/src/waveform.rs
  - crates/session/src/dj_waveform.rs
  - crates/session/tests/dj_waveform.rs
  - crates/ui/src/app/dj_grid.rs
  - crates/ui/src/app/dj_grid_tests.rs
  - crates/session/tests/dj_grid.rs
  - crates/ui/src/app/dj_waveform.rs
  - crates/session/src/dj_preview.rs
  - crates/session/tests/dj_audition.rs
  - crates/ui/src/app/dj_prepare.rs
  - crates/ui/src/app/dj_prepare_tests.rs
  - crates/ui/src/interaction/pick.rs
  - third_party/rbxport-analysis
---

# DJ workspace

The multiplayer fork adds Studio and DJ buttons at the right of the menu bar. DJ opens a local library
without requiring an open project. When the menu bar is hidden (the Mac default), a compact
workspace row keeps both buttons visible; View > DJ library is also available in the native menu. Studio returns to the existing Session/Arrangement view.
Entering DJ stops the active project's transport and sample preview through existing commands;
changing views does not edit the project. Studio keyboard shortcuts are suspended in DJ mode.
The output engine, project model and collaboration prototype remain unchanged.

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

A/B are two independent sample voices on the existing Renderer and audio output, outside the
Studio graph/undo/project file. They reuse the Preview sample reader, interpolation, rate
conversion and click guards. Each has its own bounded command/retirement queues and monitor;
no decoding, allocation, locks, file access or destruction of audio occurs on the callback.
Pause retains the exact source position; Play resumes the same source or starts again after
end/stop. Seek is in source seconds, before the decoded end. Stop rewinds; Clear also unloads.

Load A/B queues decode on the existing DJ worker, without opening output or starting audio.
Files share audition validation/cache, with an additional 128 MiB decoded sample budget per
deck (256 MiB retained decks, one pending load may retain another 128 MiB). The bounded decoder
checks headers and accumulated decoded samples. The catalog's wider import/audition limits
remain unchanged. A failed replacement keeps the previous loaded source stopped and reports
its error; a token prevents cancelled/old worker results from retargeting another load.

Only Play may open the chosen output, which remains the active Session output or the existing
empty preview Session when no project is open. A block-smoothed linear crossfader (0=A, 1=B),
channel levels 0–1 and fixed −6 dB trim feed main. A single bounded mixer message changes both
sides together. No automatic sync, pitch change, loops, scratching or recording is included.
CUE A/B is post-channel level, pre-crossfader, on the separate configured cue pair only; absence
of a separate pair never leaks an otherwise muted deck into main. The existing cue volume scales
it. Source samples and mix controls are sanitised/bounded; summing decks is not a general limiter.

Commands: `dj.deck.load {deck,track}`, `.play`, `.pause`, `.stop`, `.clear` (deck A/B), `.seek
{deck,from}`, `.stopAll {}` and `dj.mixer.set {crossfade,levelA,levelB,cueA,cueB}`. These are
person-only Library commands. Playback controls and mixer work while another worker job is
busy. Pausing/stopping/clearing never opens output; full queues are visible errors and leaving
DJ retries stopping rather than claiming silence. Library snapshots include loaded metadata,
source position, playing/loading and per-deck errors plus mixer values.

Leaving DJ stops both decks and cancels pending loads. Switching project/output invalidates
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

These are original-speed decks: the saved beat grid does not yet sync their tempos. Automated
UI checks use a real offline Renderer; the GPU image uses fixture metadata for visual review.
