---
description: Bounded portable audio snapshots over authenticated LAN, with content hashes and isolated persistent project folders
read_when: multiplayer audio files, protocol 5, shared assets, portable audio manifest
type: spec
status: draft
last_verified: 2026-10-26
covers:
  - crates/multiplayer
  - crates/session/src/lib.rs
  - crates/ui/src/app/multiplayer.rs
---

# Initial audio files in a shared LAN project

Self-authored fork behaviour. ADR 0026's document + content-addressed manifest + collected audio
direction is adopted for this restricted adapter. The ADR's general native folder/package API
is not yet implemented in this checkout; this slice does not introduce a new native file version
or claim that its general zip-package workflow exists.

Protocol 5, portable-copy format 2 extends existing MIDI/clip/control operations. Initial music
may contain audio regions. FM Synth/Utility parameters remain the only allowed devices; sampler
states, external plugins, streamed stems and file-backed devices remain refused. Empty audio
clips are allowed. Later publication of new recordings and new assets is separate work.

The person explicitly selects Share project + audio. Only audio already decoded and available
in the selected session is copied; no network input names a source file to read. Missing audio
fails without opening an invitation. Decoded mono/stereo 8000..192000 Hz samples are preserved
as canonical float32 WAV copies using hound. Original encodings, source path references, file tags and unrelated
files are not part of the asset exchange; musical track/clip names remain in the project. Nonfinite or over-16 absolute sample values are refused. This is a
sound-preserving portable copy of loaded audio, not a byte archive of original MP3/FLAC/WAV files.

A strict manifest maps BLAKE3 hashes to Audio/<64 lowercase hex>.wav, byte size, sample rate,
channels and frame count. At most 64 assets, 64 MiB per WAV and 256 MiB total; document <=2 MiB
and all existing model/device/clip/note limits still apply. Identical bytes are sent once.
Every nonempty audio reference must exactly match a manifest asset; absolute paths and streams
are removed/refused and all local hardware inputs remain off. Asset identity/metadata are checked
against the verified waveform. No arbitrary relative path, drive prefix, traversal or symlink is
accepted from the peer.

After authenticated admission, transfer the current portable native document and revision,
then a bounded manifest and exact-index/offset chunks pulled over TLS. Asset exchanges have an
absolute 180-second deadline and cancellation closes sockets. The receiver checks the manifest
and reference set before reserving disk space, writes create-new .part files in a fresh private
session directory, verifies size and BLAKE3 before decoding, bounds RIFF chunk sizes before
hound, requires canonical re-encoding, and caps decoded samples by the manifest byte budget.

Audio/<hash>.wav files, assets.json and project.suhina form the collected session folder.
The document is finalized last, and no tab opens until the entire set and contiguous operation
suffix are verified. A temporary-directory lease removes incomplete/abandoned transfers. The
UI retains the folder only after adding the new tab successfully. Completed folders stay in the
application data Shared projects directory so leaving, saving elsewhere and reopening can still
find the sound. Existing folders, original projects and their samples are never replaced.

The receiving session gets its verified audio pool before attaching playback. Musical document
references remain identical on both machines; local folder paths stay outside canonical state.
Saving uses the native relative/absolute reference rewriting. Canonical preflight and mirrors
continue to forbid new asset references through the exact command allowlist. Existing supported
clip operations can act on audio, with stack-only placement; native note commands refuse audio.

Tests use real TLS and isolated filesystem roots: missing files on the guest machine, mono and
stereo, identical render output, independent players, persistence through save/reopen, dedup,
late admission, edits, malformed manifests/paths/metadata/waves/hashes/chunks, quotas, cancellation,
failed finalization and preservation of originals. Physical two-computer testing stays deferred.
