---
description: Revision-checked shared MIDI note commands and bounded operation delivery over LAN
read_when: shared notes, multiplayer protocol 10, operation journal, note.update
type: spec
status: draft
last_verified: 2026-10-26
covers:
  - crates/session/src/notes.rs
  - crates/session/src/commands/note.rs
  - crates/collaboration/src/policy.rs
  - crates/multiplayer/src/protocol.rs
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/transfer.rs
  - crates/ui/src/app/multiplayer.rs
  - crates/ui/src/app/multiplayer_notes.rs
---

# Live MIDI notes in a restricted shared session

Self-authored fork behaviour; no proprietary implementation or format is involved.

The existing MIDI-copy subset gains single-note add, update and delete commands. Every request
names a stable clip ID and the exact project revision the person edited. Note indices are native
zero-based indices, so a stale revision must be refused before any mutation; indices must never
be silently rebased after another participant deletes a note. The shared editor remains inside
the multiplayer panel; ordinary project mutations and undo remain blocked while connected.

`note.update` atomically sets one note's time, positive length, MIDI key 0..127, velocity 1..127
and muted flag. It preserves channel and release velocity, uses native copy-on-write semantics,
and forms one undo step. All values are validated before changing the note. The LAN admission
allowlist admits only one note per request and stable clip addressing. Arbitrary commands,
batches, local paths and other MIDI operations remain refused.

Protocol 3 adds a revision-stamped operation journal capped at 32 operations. Polls include the
last verified revision; edit requests separately name their draft base revision. Responses
include every contiguous accepted operation since the verified revision and the current mixer
controls. Duplicate request retries must not append another operation. A cursor ahead of the
host or older than the journal fails visibly; no silent replacement of the playing session.
The receiving worker validates and applies operations to a playerless mirror before emitting
them. The UI applies the same native commands to the existing session and its attached player.

Admission transfers a current canonical MIDI snapshot with its revision, then operations since
that snapshot. Changes during download are either delivered contiguously or fail before a tab
opens. The original project, independent transports, native file version 3 and portable-copy format
2 stay as specified in lan-midi-copy.md. Both installations now need protocol 10, which also admits the
[clip operations](lan-clip-editing.md). The journal was introduced by protocol 3.

Before accepting a native command, preflight it on a detached canonical copy and verify all MIDI
copy quotas and the 2 MiB canonical document limit. Failed/stale/oversized edits change neither
host revision nor sequence. All queues, operation lists and frames remain bounded. Lag and
malformed operations disconnect visibly while preserving the last local project.

Validation covers atomic native edits and undo, copy-on-write, stale index refusal, journal
replay and gaps, joining after edits, real encrypted bidirectional note edits, independent
players and actual audio renders. Physical two-computer testing is deferred to user acceptance.

Protocol 8 additionally admits bounded [shared track creation](lan-track-creation.md) through the same operation journal.


## Compatibility amendment: Suhina update of 28 October

[Protocol 13](lan-upstream-20261028.md) supersedes earlier version pins in this contract: native
project v4, portable-copy format 3 and matching pinned Suhina engine identities are required.
Ordinary old project files remain readable through the studio reader, then share as v4. The
musical edit permissions and quotas above remain unchanged; newly added studio effects are not
implicitly permitted in shared tabs. All participants need the updated combined application.
