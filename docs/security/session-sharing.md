---
description: Security boundary of trusted collaboration and the restricted certificate-bound LAN starter adapter
read_when: multiplayer threat model, invitation security, peer authorization, session network boundary
type: system
status: draft
last_verified: 2026-10-25
covers:
  - crates/collaboration
  - crates/multiplayer
  - crates/session/src/projects.rs
  - crates/ui/src/app/multiplayer.rs
---

# Session sharing threat model

## Trusted core and remote boundary

`collaboration` remains a trusted in-process API. Its full Project clones and opaque
acknowledgements are not wire formats. Numeric PeerIds are local bookkeeping, never credentials.
The separate `multiplayer` adapter exposes only the [LAN starter contract](../spec/lan-starter-session.md).
Its [MIDI-copy decoder](../spec/lan-midi-copy.md) is the only remote initial-project boundary;
arbitrary native projects, plugin/microphone state and source/destination file paths remain refused.
The separate [audio contract](../spec/lan-audio-assets.md) admits only verified portable audio copies.

The host retains its canonical project and accepts only track rename, volume, pan and single-note add/update/delete and bounded clip placement operations on
at most 64 verified project tracks. Membership, role, exact parameter keys, IDs, numeric ranges and sequence/revision
checks precede execution. The adapter allocates the guest's PeerId after admission. A guest
cannot claim the owner's identity or change roles. Transport, history, files, commands/batches,
settings and library/plugin operations have no remote representation.

## Admission and confidentiality

Only an explicit Create LAN session action starts a listener, on one private/loopback/link-local
IPv4 address. There is no discovery, relay, DNS target, public binding or internet service.
Joining trusts the certificate embedded in the private invitation with normal rustls TLS 1.3
verification for suhina.local. A 256-bit OS-random secret is proven inside TLS, compared by
constant-time hash equality, expires after five minutes and is redeemed once. Stopping revokes
all admission. Disconnect cannot resume a used invitation. One host and one guest are supported.

The invitation is a bearer capability: a person with its complete contents can join during its
lifetime. Send it privately. Certificate mismatch, malformed data and failed admission do not
consume a legitimate invitation. Debug output is redacted; neither the invitation nor its secret
is written to application logs or project files. The user's explicit clipboard copy is intentional.
An attacker controlling that clipboard or either computer is outside this boundary.

## Resource and mutation limits

Length-prefixed typed JSON is capped at 65536 bytes before body allocation, with exact fields,
a maximum of 64 tracks and bounded names/ranges. Control characters in track names are refused.
Initial music has a separate 2 MiB quota and a 30-second transfer deadline: exact-offset chunks,
SHA-256, JSON-tree/count limits, native-version pinning and exact canonical round-trip bytes all
precede session construction. Opaque state and extensions, arbitrary file references, SDK graphs and external/foreign
devices are refused; only parameter-only FM Synth and Utility are allowed. Hardware inputs and
launch mappings are removed from the local copy and verified off at the receiver. Outside the document transfer, read/write/handshake exchanges have absolute three-second
deadlines. The host has at most four pending connection handlers; the normal authenticated
guest polls every 100 ms and the server enforces at least 50 ms per request. UI queues are bounded.
TLS, canonical execution and I/O run on workers, never the audio thread. A local peer can still
occupy pending slots until timeout; availability against a sustained LAN flood is not guaranteed.

Protocol 5 delivers at most 32 revision-stamped operations per response from a 32-entry journal.
A separate draft base revision prevents shifted note indices from being silently reused. Before
canonical mutation the native command is preflighted on a detached copy and the complete MIDI
quota/document boundary is checked, including copy-on-write growth. The guest verifies contiguous
operations on its detached mirror and checks the same boundary before notifying the UI. Ahead or
expired cursors disconnect; no snapshot replacement hides divergence. The service caps pending
UI operations at 128 and fails visibly on overflow. Admission snapshots current music and its
revision, so an old document cannot be combined with a new note history.

The [clip contract](../spec/lan-clip-editing.md) adds create/move/trim/duplicate/delete/setEnabled.
All creation, move, trim and duplication operations explicitly stack; a peer cannot request
implicit replacement of neighbours. Stable clip IDs replace positional addressing. Native ID
allocation and content copy-on-write are replayed deterministically and their quota growth is
preflighted before canonical mutation. No clipboard, arbitrary batch, new track, file import,
audio stretch or consolidation is exposed. The editor lists bounded pages of arrangement clips.

The desktop creates a dedicated starter or verified MIDI copy, preserving the original project. A Projects mutation gate prevents
normal UI and attached-agent commands from changing its shared model/history outside confirmed
operations. Its host-owned API admits only twelve exact control/note/clip command IDs; it is not a registered
agent command. Other project tabs stay local. Local audition and explicit save/export remain
available. Disconnect retains and unlocks the last local project; a failed authoritative apply
leaves the session rather than silently continuing with divergent state.

## Evidence and remaining boundaries

Automated tests cover real TLS loopback exchange, wrong certificate/secret/version, one-use and
expiry rules, replayed edits, stale revisions, malformed/duplicate fields, an incomplete handshake
timeout, bounded/chunked MIDI copies, invalid offsets/size/hash, cancellation, opaque/path-bearing
documents, stopped connections, desktop application of controls, original-project preservation,
mutation refusal and independent real audio renderers. Physical two-computer/firewall and cross-platform session acceptance remain
unverified. This prototype has not had an independent security audit.

Sharing arbitrary projects/media beyond the explicit portable MIDI/audio subset remains closed until portable asset manifests, path/symlink
confinement, size/hash/disk quotas, atomic finalization and interruption recovery are designed and
tested, aligned with project folder work (#606). Wider edits require identity allocation and
side-effect review. Recording publication, linked transport, presence, reconnect/resume, host
migration and internet routing need separate contracts. The restricted starter is not evidence
that these wider boundaries are safe or implemented.

## Initial audio asset boundary

Only sounds already loaded in the locally selected session are collected. Original source paths
do not reach guest file I/O: references use generated Audio/<BLAKE3>.wav paths, backed by an exact
manifest. Assets are canonical float32 WAV, mono/stereo 8000..192000 Hz, finite and bounded; RIFF
chunk lengths are checked before hound. Hash, size, metadata, canonical re-encoding and reference
coverage are all required. Budgets: 64 files, 64 MiB each, 256 MiB total and a 180-second deadline.
A receiver writes create-new files in a fresh private directory at a trusted local root, then
finalizes the document after the assets. Temporary-directory ownership crosses into the pending
snapshot; failures and unclaimed snapshots remove it. The desktop retains the completed folder
only after adding its tab. Tests exercise paths, hash/offset/size/metadata errors, malformed WAV,
cancellation, quota rejection, non-overwrite, cleanup, save/reopen and real TLS/audio equality.
Sharing new files/recordings during a connected session remains a separate closed boundary.
