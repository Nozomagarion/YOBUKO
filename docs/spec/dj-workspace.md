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
  - crates/ui/src/app/dj_playlists.rs
  - crates/ui/src/app/dj_playlist_tests.rs
  - crates/session/tests/dj.rs
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

Cue editing, decks, USB export, LINK and shared DJ collections are not implemented
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

## Local playlists (fork extension)

The person can create, rename and delete named playlists, add catalog tracks once, remove them
and move them to a final one-based position. Playlists hold stable track ids in order; reanalysis
keeps those ids. Removing a playlist or an entry never moves, edits or deletes audio or projects.
The collection selector offers All tracks and the saved playlists; each list is paginated.
Deletion needs an explicit confirmation in the UI. Draft names are tied to the catalog revision:
a changed revision requires refreshing the draft before saving it. Shared DJ playlists are deferred.

Catalog format 2 adds playlists, a monotonically checked revision and a non-reused playlist-id
counter. Limits are 128 playlists, 128 bytes per trimmed nonempty name without control characters,
2000 unique entries per playlist and 16000 entries in total, still within the 4 MiB catalog limit.
Unknown ids, duplicate entries, unknown fields, invalid counters and unsupported versions fail.
Format 1 opens without writing; the first successful mutation keeps a byte-exact sibling
`dj-library-v1-backup-*.json` before publishing format 2. A malformed catalog is never replaced.

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
