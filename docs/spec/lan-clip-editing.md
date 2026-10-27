---
description: Revision-checked MIDI clip arrangement edits in the restricted LAN session
read_when: shared clip placement, multiplayer protocol 8, clip creation duplication deletion, arrangement collaboration
type: spec
status: draft
last_verified: 2026-10-26
covers:
  - crates/collaboration/src/policy.rs
  - crates/multiplayer/src/protocol.rs
  - crates/multiplayer/src/transport.rs
  - crates/session/src/projects.rs
  - crates/ui/src/app/multiplayer.rs
  - crates/ui/src/app/multiplayer_notes.rs
  - crates/ui/src/app/multiplayer_clips.rs
---

# Shared MIDI clip placement

Self-authored fork behaviour built on the existing native clip commands. Protocol 8 retains
the operation journal of lan-note-editing.md; both installations must upgrade. Native file
version 3, portable-copy format 2, admission, musical quotas and independent players stay unchanged.

The Clips tab lists arrangement clips by stable clip ID, with their track, start, length and
enabled state. It provides six narrowly admitted native operations:

- `clip.create`: one existing track ID, explicit arrangement start and positive length, stack true.
- `clip.move`: one stable clip ID, absolute start, existing destination track ID, stack true.
- `clip.trim`: one stable clip ID and absolute start/end, stack true; moving the left edge
  follows native trim/offset semantics. No loop-setting parameter is admitted.
- `clip.duplicate`: exactly one stable clip reference and stack true; the copy follows the
  original and gets a new deterministic native clip ID. Contents use native copy-on-write.
- `clip.delete`: one stable clip ID. The native resolver also safely resolves a launcher placement;
  the initial UI exposes arrangement deletion only.
- `clip.setEnabled`: exactly one stable arrangement clip reference and a boolean enabled flag.

Creation, movement and trimming always stack, so no neighbouring clip is silently removed,
shortened or split. General track.add, file import, clipboard, arbitrary batch, audio stretch,
consolidation, recording, transport or launch command is admitted. Native validation checks
track compatibility and placement; the LAN adapter preflights each result against the complete
MIDI-copy quota before changing the host or consuming a sequence. Explicit starts/trim ends are finite,
0..120000 beats; lengths are positive and at most 120000 beats. End must follow start.

Each draft freezes the global revision. New shared operations mark the draft stale; Refresh
clip draft discards its selection before another edit. Stable IDs keep moves and deletions
from targeting a different clip after sorting. A deleted note editor selection must be cleared
explicitly instead of being silently redirected. Local transports and attached players survive
these commands; only the existing native player notification path changes the arrangement.

The host alone orders object creation. Applying the same accepted native command to the same
initial model allocates the same clip/content IDs and counters on every replica. Quota checks
include new contents, duplicated placements and copy-on-write after editing a duplicate. A late
join receives the current arrangement and notes, followed by a contiguous operation suffix.

Validation uses real TLS, both directions, creation then note editing, cross-track moves,
trimming, duplication/COW, deletion, enabling, stale requests, quota refusals, exact ID/model
convergence, late admission and real independent renderers. UI tests click the visible actions
and check that the original project and stale drafts remain protected. Physical two-computer
testing remains the later user acceptance step. The audio contract below extends this journal for verified sounds.

## Amendment: initial collected audio

[The audio contract](lan-audio-assets.md) adds verified initial audio assets under protocol 8.
The same clip operations act on audio; new audio clips are empty and cannot acquire a new file
through these commands. The UI lists both MIDI and audio destination tracks. Native note commands
refuse audio clips. Audio stretch and plugins stay closed. Protocol 8 additionally orders verified verified finished-take placement via clip.insertAudio, as specified in the audio contract.

Protocol 8 additionally admits bounded [shared track creation](lan-track-creation.md) through the same operation journal.
