---
description: Shared projects with gigabytes of sounds - chunked streaming from disk, per-chunk BLAKE3 verification, a per-computer sound cache, resumed transfers and opening before the sounds arrive
read_when: heavy project, large samples, sound cache, resume transfer, protocol 16, chunk tree, multiplayer audio limits, gigabytes of audio
type: spec
status: draft
last_verified: 2026-10-30
covers:
  - crates/multiplayer/src/assets.rs
  - crates/multiplayer/src/sound.rs
  - crates/multiplayer/src/cache.rs
  - crates/multiplayer/src/transfer.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
---

# Heavy shared sessions

Self-authored fork behaviour. It extends the [audio asset contract](lan-audio-assets.md) and
[guest takes](lan-guest-takes.md) so a project with several gigabytes of samples can be shared
over the LAN or [online](online-sessions.md). Everything that contract says about portable
canonical float32 WAV copies, content-addressed `Audio/<BLAKE3>.wav` references, strict
manifests and peer paths still holds; only the limits, the wire format of sound bytes and the
storage change. Protocol 16 (ALPN `suhina/multiplayer/16`), shared with
[session resume](session-resume.md); this change was developed as protocol 15 alongside it.

## Limits

The caps live in one place (`crates/multiplayer/src/assets.rs`) and are validated before any
allocation, disk reservation or request:

| What | Limit |
|---|---|
| Distinct sounds in a session (initial copy and later takes) | 4096 |
| One canonical WAV copy | 2 GiB |
| All distinct sounds of a session | 16 GiB |
| Sound cache on each computer | 20 GiB, least recently used removed first |
| Transfer chunk | 1 MiB |
| Manifest entries per frame | 128 |
| Wait for one chunk or request | 60 s, renewed by every chunk |

The 180-second absolute deadline for a whole sound exchange is gone: a transfer of gigabytes is
bounded by the per-chunk stall time instead. Decoded sounds are still held in memory by the
engine, as every loaded sound is; what no longer sits in memory is the encoded WAV copy, on
either side, at any time.

## Canonical copy and its chunk tree

A shared sound is the 68-byte WAVE_FORMAT_EXTENSIBLE float32 header followed by the samples, the
bytes hound writes for the same audio, so its BLAKE3 hash is unchanged from earlier protocols.
It is written to disk in 1 MiB blocks while it is hashed; the sender also records the BLAKE3
chaining value of every 1 MiB chunk (a power of two of BLAKE3's 1 KiB chunks, so each transfer
chunk is a whole BLAKE3 subtree). Merging those values as BLAKE3's tree does gives the root hash,
which must equal the manifest hash. A sound of one chunk has no tree and is checked against its
hash directly.

The receiver checks the tree's root against the manifest before it accepts any chunk, then each
chunk against its chaining value at its offset before writing it. A forged tree, an altered,
moved or resized chunk is refused at once. After the last chunk the whole file is read once more
while decoding: header equal to the canonical header for the manifest's rate and channels,
finite samples within ±16, and the whole-file hash.

## Wire format

Frames keep the length-prefixed JSON of earlier protocols and their 64 KiB limit. Sound bytes are
no longer base64 inside JSON: a small JSON header names the chunk, and the chunk's raw bytes
follow it on the stream. This removes base64's one-third overhead and the per-chunk round trip.

- **Manifest:** `Assets {total, manifest}` pages of at most 128 entries; `total` is at most 4096
  and every page continues the previous one.
- **Download:** the guest sends `AssetDownload {asset, offset, tree}` for one offered sound, from
  a whole-chunk offset, asking for the tree only when it has none. The host answers with
  `AssetTree {asset, leaves}` and `leaves × 32` raw bytes when asked, then `AssetChunk {asset,
  offset, len}` + `len` raw bytes for every chunk to the end. Each sound is served at most once
  per exchange. `AssetsDone` ends the exchange.
- **Upload (guest takes):** the host sends `UploadRequest {offset, tree}`; the guest answers
  with `UploadTree` and `UploadChunk` the same way, then the host sends its State. A host that
  already holds the sound answers with State and no request.
- **Document:** after `Project {…}` the guest sends one `Download {offset: 0}` and the host
  streams `Chunk {offset, len}` + raw bytes in 32 KiB pieces; SHA-256 is checked at the end.
- **Background:** `Fetch {seen_revision, asset, offset, tree}` replaces a poll while the guest
  still receives sounds of its initial copy; see below.

## Sound cache

Each computer keeps verified sounds in **Sound cache** inside its shared project storage (the
application data **Shared projects** folder), named only by their validated 64-hex hash:
`<hash>.wav` for a complete sound, `<hash>.part` for a partial copy and `<hash>.tree` for its
chunk tree. No name a peer sends reaches the file system, symbolic links and folders under those
names are ignored, and files the cache did not write are left alone.

- A guest offered a manifest uses every cached sound that still decodes and verifies; it only
  requests the sounds its cache lacks, so a sound already received is never sent again. The
  request list is the announcement of what it lacks: one request per missing sound, bounded by
  the manifest.
- A cached copy that fails verification is removed and downloaded again.
- Session folders get hard links to cached files (a copy across volumes), so a session keeps
  its sounds when the cache later removes them.
- After each exchange the cache removes the least recently used entries until it fits its cap,
  sparing the sounds of the current exchange. Using a sound refreshes its date.
- A host keeps the sounds it serves in a private `outgoing-…` folder for the session, removed
  when the session ends; uploaded takes reach the host's cache the same way downloads reach a
  guest's.

## Resume

A cut transfer leaves the partial copy and its tree in the cache. The next attempt, in the same
session (background slices) or after rejoining with a new invitation, reopens the partial copy,
keeps the whole chunks that match a tree whose root is the sound's hash, truncates the rest and
asks only for the remainder, without the tree. A partial copy without a trustworthy tree starts
over. The same applies to uploads into the host's cache.

## Opening before the sounds arrive

When more than 32 MiB of the initial copy is missing from the guest's cache, the guest answers
the manifest with `AssetsDone` at once, opens the project, and receives those sounds in the
background: each poll becomes a `Fetch` for at most 8 chunks of one sound, answered by those
chunks and the usual State, so edits, publication and linked playback keep flowing between
slices. A finished sound is verified, linked into the open session's folder and delivered to the
view like a published take, in the State event that also carries operations; its regions are
silent until then. `receiving` in the service state shows the background progress. The host
serves a connection at most twice its initial copy in background slices. Smaller copies still
arrive before the project opens, as before.

Leaving before everything arrived keeps the partial copies in the cache for the next session.
Background slices name sounds by their index in the copy's manifest, which only the connection
that sent the copy knows: a host serves `Fetch` only on that connection, and refuses it on a
resumed one. A guest whose link drops while sounds are still pending therefore resumes with
`seen_revision: null` and gets a fresh copy ([session resume](session-resume.md)); its sound
cache keeps the sounds and partial chunks already received, so only the rest crosses again.
A copy saved before its sounds arrived refers to sounds its folder does not hold yet.

## Tests

`sound_tests.rs`: the streamed copy equals hound's bytes, tree roots equal the whole-file hash at
every size, forged trees and altered or moved chunks are refused, decoding checks header,
samples, size and hash. `cache_tests.rs`: least-recently-used trimming that spares current and
foreign files, resume after the last chunk matching the tree, invalid names. `assets_tests.rs`:
hostile manifests, pages and totals refused before allocation, a corrupted chunk refused with
only verified chunks kept, a cut download and upload resumed, a cached sound never requested
again, a corrupt cache entry replaced, a multi-chunk sound over a real socket cut and resumed,
background slices resumed after leaving, the host's background budget. `faults_tests.rs`: a
multi-chunk background sound cut midway by the fault injector, completed after the resume from
its partial copy, over `online-loopback` and the LAN.
`tests/heavy.rs`: a 40 MiB sound over the LAN and over `online-loopback`, opened before it
arrives, silent then identical, and a second session that takes it from the cache (the same file
on disk).
