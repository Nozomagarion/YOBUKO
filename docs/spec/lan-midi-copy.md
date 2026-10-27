---
description: Bounded MIDI-only copies of an open project over authenticated LAN, preserving the original and refusing audio and opaque devices
read_when: share current MIDI project, LAN project transfer, multiplayer protocol 9, MIDI copy limits
type: spec
status: draft
last_verified: 2026-10-26
covers:
  - crates/multiplayer/src/midi.rs
  - crates/multiplayer/src/transfer.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
  - crates/session/src/lib.rs
---

# A MIDI project copy in the LAN session

## Scope

The host can explicitly share a musical copy of the open project rather than generate the
starter. The original remains open and unchanged; both participants get a separate restricted
session tab. A copy has no source path and is marked unsaved, so closing prompts for a first
save even before a control edit; saving cannot target the original implicitly. Arrangement and
launcher clips, notes, stable IDs, tempo/meter, routing and mixer
state are retained. Track name, volume, pan, MIDI notes and arrangement clips are live shared edits;
[the note-editing contract](lan-note-editing.md) defines their revision checks and delivery.
[The clip contract](lan-clip-editing.md) adds bounded arrangement operations. The explicit Share project + audio option follows the separate
[audio asset contract](lan-audio-assets.md); plugins remain refused; host publication of finished takes is specified in that contract.

A supported source has 1–64 tracks, at most 128 channels, 512 contents, 4096 clips and 16384 notes.
Only the file-free FM synth and Utility built-ins, with parameters but no opaque state, are
supported initially, with at most 128 devices total. Mixer gain is bounded from silence to
+6 dB, pan to -1..1 and send levels to 0..1. Audio regions, audio tracks, device groups, SDK graphs,
other built-ins and
external/foreign devices are refused explicitly, even when disabled. No unsupported music is
silently removed. Opaque extensions, hardware input mappings and launcher key/MIDI mappings are
local metadata and are omitted from the new copy. MIDI and microphone monitoring are disabled
in that copy. The source project, its files and its settings are never changed.

## Document and admission

Protocol 8 uses portable-copy format 2 for initial audio assets and retains live notes,
revision-checked arrangement clip operations and snapshot revisions. Old invitations
are refused; both apps need this protocol. The existing certificate-bound TLS admission, expiry,
one-use secret, owner-ordered controls and local transport rules remain in force.

The musical document uses the existing `.suhina` serializer (native format version 3), in its
canonical encoding, with a maximum of 2 MiB. It is data in memory, not a remote filename or disk
import. Before calling the native reader, bound the JSON tree, strings and collection sizes.
After parsing, check the MIDI/device/input subset and require the exact canonical bytes to match
the writer. This rejects unknown/duplicate fields and normalization loss, including newer native
formats. The MIDI-copy format must change if its canonical encoding/subset changes incompatibly.
No network-supplied path, extension blob or plugin state reaches project preparation.

After authentication the host offers a current snapshot's revision, byte length and SHA-256. The guest pulls
4096-byte chunks at exact offsets, inside typed bounded frames; it verifies length, hash, native
format and the allowed subset before creating any session tab. A transfer has an absolute
30-second deadline and cancellation shuts down its socket. Each control frame is capped at
64 KiB, enough for 64 bounded track names. A bad length/hash/offset/version or an incomplete
transfer fails visibly and opens no partial project. There are no temporary files or assets.

The document is an immutable snapshot of the current canonical music at admission, with its
revision. Controls and notes can change during download: a contiguous, bounded operation suffix
is verified and applied to the detached receiver before a tab opens. A journal gap fails the
join. Subsequent operations go through the same native commands on the existing local session;
its player is never replaced to synchronize edits. Track IDs/order stay fixed and are checked
against the control snapshot. No silent fallback to the starter is allowed.

## Validation

Use real TLS loopback tests with a non-demo source, non-contiguous track IDs and documents larger
than one frame. Verify the received musical model, independent real renders, edits in both
directions and preservation of the original. Exercise oversized/truncated/wrong-hash documents,
wrong offsets, unknown/duplicate fields, audio/path-bearing inputs, opaque state, cancellation
and incompatible protocol. UI tests cover the current-project action and rejection without a new
project/listener. Physical two-computer testing remains a later user acceptance step.

This is deliberately an interim MIDI-only exchange, not the asset manifest/package of
[ADR 0026](../decisions/0026-project-folder-and-package.md). Audio sharing must use that portable
asset direction and its path/hash/quota/atomic-finalization boundary rather than widening this
reader to arbitrary projects or file paths.

## Audio mode

The MIDI-only selection remains strict and performs no audio collection. Share project + audio
selects already-loaded audio explicitly, normalizes references to content-addressed WAV copies
and sends the same bounded native document followed by a manifest and audio chunks. The portable
decoder admits only empty file references or generated Audio/<hash>.wav names, no absolute paths.
Both modes use portable-copy format 2; a guest waits for the complete verified audio set and
initial operation suffix before a tab opens. See [the audio contract](lan-audio-assets.md).

Protocol 8 additionally admits bounded [shared track creation](lan-track-creation.md) through the same operation journal.
