---
description: The fork's Studio and DJ workspaces, separate local catalog, asynchronous rbxport analysis and rendered-project bridge
read_when: DJ library, rbxport integration, render a project for DJ preparation
type: spec
status: current
last_verified: 2026-10-26
covers:
  - crates/dj
  - crates/session/src/dj.rs
  - crates/ui/src/app/dj.rs
  - crates/ui/src/app/dj_tests.rs
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

`dj.library` polls the worker and returns tracks, busy, error, analysis and format.
`dj.import` queues one audio file, with an optional `sourceProject` path. Both are Library
commands on Projects and are denied to MCP agents: managing the local DJ collection is the
person's action. Typed file parameters retain their path classification. No Rekordbox database
is opened or written. The CLI sees the same registry; it needs a configured library folder
and repeated library polling to wait for an asynchronous job.

The first library request starts one `suhina-dj` worker. It reads `dj-library.json` beside
`browser.json`, then handles one import at a time. Open, decode, analyse and save run on that
worker. The UI receives snapshots over a channel. Changing the configured file library drops
the old worker and starts the next catalog lazily. An invalid existing catalog is reported
without overwriting it. A failed import does not prevent another file from being attempted.

Catalog format 1 contains at most 2000 tracks and is at most 4 MiB. Each track has a local id,
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

Playlists, cue editing, decks, USB export, LINK and shared DJ collections are not implemented
by this slice. Multiplayer remains the trusted in-process experiment in
[session sharing](session-sharing.md); no network listener is added.

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
