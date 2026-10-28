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
all admission. Disconnect cannot resume a used invitation. One host and at most three guests are supported. Each admitted connection has a separate
authenticated PeerId and native sequence/cache; a handler exit removes only its own peer.

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
launch mappings are removed from the local copy and verified off at the receiver. Outside document and asset transfers, read/write/handshake exchanges have absolute three-second
deadlines. The host has at most four pending connection handlers; the normal authenticated
guest polls every 100 ms and the server enforces at least 50 ms per request. UI queues are bounded.
TLS, canonical execution and I/O run on workers, never the audio thread. A local peer can still
occupy pending slots until timeout; availability against a sustained LAN flood is not guaranteed.

Protocol 8 delivers at most 32 revision-stamped operations per response from a 32-entry journal.
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
operations. Its host-owned API admits only twenty-four exact musical command IDs; it is not a registered
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
side-effect review. Recording publication, linked transport, presence, automatic reconnect/resume, host
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

## Amendment: host publication of finished takes

Protocol 6 admits clip.insertAudio only through trusted host-side AudioData selection and the
explicit publishTake command. Ordinary LAN edit entry points reject this variant; canonical
submission independently refuses a guest author. No network message can select a local source
path. The pure native insertion never reads files and always stacks on an existing audio-only
track. Publication consumes one revision only after preflight and successful local collection.

After each State, a strict missing-assets manifest and pulled chunks precede UI publication.
The guest stages operations on a detached model; failed/cancelled transfers never reach its tab.
Known references must retain their verified metadata, and cumulative verified sounds stay within
64 files / 256 MiB even if clips are deleted. File transfers use the absolute 180-second deadline.
Local collection mappings are kept outside canonical state and added only by trusted worker
results. Saving/backups include these local paths on a copy; shared models remain identical.

## Shared track boundary

Protocol 7 admits track.createShared with exactly kind and name, from either admitted editor.
Its pure native handler can add only an audio skeleton or a MIDI skeleton with default FM Synth;
no file path, arbitrary device/preset, ID, group or hardware binding is accepted. Names are bounded
and have no control characters, inputs/monitoring are off, colours are explicit, and the full
portable track/channel/lane/device/document quotas are preflighted before revision or identity
allocation on the canonical host. The guest mirror and UI execute the same operation. Local theme/default normalisation is skipped for the bound tab to prevent mutations outside that journal. Ordinary
track.add and broader track mutations remain refused. See [the track contract](../spec/lan-track-creation.md).

## Verified guest publication

Protocol 8 offers strict sound metadata, author sequence, draft base, delivery cursor and target
on the authenticated connection; the host derives the author rather than receiving an ID. A
read-only trusted-core preflight enforces membership, roles, replay, revisions and counters;
the adapter also preflights native placement, quotas and existing-hash metadata before bytes.
Pull chunks into a fresh private folder and verify size, BLAKE3, bounded WAV, metadata and exact
re-encoding outside the canonical lock. Recheck before committing the guest-authored operation;
raw Edit/audio insertion is still refused. Queue the decoded sound with its operation so UI
backpressure cannot expose a clip before its sound. Pending leases/catalogs are bounded, and
failed/unclaimed data is cleaned. The guest validates its exact acknowledged operation before
retaining its local collected copy. The known upload is not echoed to its sender. No remote
source filename, path, device, role or arbitrary command is accepted. Direct shared recording
and automatic reconnect remain closed. See [the upload contract](../spec/lan-guest-takes.md).


## Explicit invitation renewal

Only the local hosting service can queue multiplayer.renewInvitation with no parameters. It
cannot eject or replace a connected guest. Canonical admission and rotation share one lock;
old pending hellos fail after rotation without consuming the new secret. Fresh OS-random
256-bit secrets get a new five-minute single-use lifetime. The bounded UI event must be accepted
before changing the stored hash/expiry/redemption bit, so backpressure does not strand the host
with an undisclosed replacement secret. Token errors never include the secret. The listener and
pinned certificate stay the same; no network renewal command exists. Protocol 12 permits rotation
while other guests are connected, refusing a full session. There is one outstanding token, so
rotation replaces an unused invitation; it never invalidates an authenticated connection.

A new admission downloads current verified music and assets into a new guest copy. Previous
unlocked tabs and solo edits remain local, without merging them. This permits explicit recovery
after a guest disconnect; automatic resume, host recovery and Internet routing remain closed.


## Shared device parameter boundary

Protocol 9 admits exactly device.setSharedParam with numeric track, integer top-level index
0..15, bounded ASCII parameter ID and one finite number. A pure native guard resolves and
checks the actual device before entering the ordinary setter: parameter-only FM Synth/Utility,
no opaque state, known parameter and already-canonical value. Thus no admitted edit can load
an asset, address a plugin, follow a group path or import a preset/gesture. The same guard runs
on detached canonical preflight, guest mirror and the authoritative desktop path. Missing or
invalid targets do not mutate the project or consume accepted counters. Base revisions bind
device indices; structural edits use the separate guarded chain commands. Existing manual automation override is replicated.
Generic device.setParam remains refused. [Contract](../spec/lan-device-parameters.md).


## Shared device chain boundary

Protocol 10 admits two exact scalar commands: addShared(track,kind) and
removeShared(track,device). Pure native guards restrict the entire target chain to top-level
FM Synth/Utility with no state, enforce 16/128 device limits on addition and refuse instrument
replacement or an instrument on a non-MIDI track. They pass no preset, position, nested path,
asset or plugin selector to the native handlers. Preflight/replay/revisions remain unchanged.
All confirmed chain changes clear desktop device/parameter selection, including hidden pages
and same-index replacement, so Refresh cannot redeem a draft against another device.
See the [chain contract](../spec/lan-device-chains.md).


## Shared launcher boundary

Protocol 11 admits scene.insert/rename/duplicate/delete, clip.createSharedSlot,
clip.copyToSharedSlot and clip.moveSlot. Exact numeric fields, one-based indices and bounded
names are checked before native execution. No clipboard, filesystem path, implicit row growth
or arbitrary batch is admitted. Copy refuses occupied or incompatible targets; moving uses
explicit native swap semantics. Detached portable validation checks complete scene/content/
clip/note/document quotas before canonical mutation or sequence consumption. Audio copies
reuse previously verified assets; no new asset path enters this operation. UI drafts freeze the
revision, and every scene insertion/duplication/deletion clears selections on hidden pages too.
Private launches remain local transport, rejected as wire edits. See
[the launcher contract](../spec/lan-launcher-editing.md).


## Multiple-guest boundary

Protocol 12 bounds admitted peers to three guests under the canonical admission mutex, before
allocating a PeerId or consuming the single-use token. The host still caps all socket handlers
at four. Renewing refuses capacity, backpressure and repeated secrets without replacing the
previous capability. A count-only membership update does not change musical revision or reset
any author's sequence. Every server State carries the count; guests refuse missing/out-of-range
values (valid 1..3) before emitting an update. No count, identity, role or renewal action from a
guest can override canonical membership. Guests receive no invitation for another guest.

Concurrent edit/upload drafts retain the existing revision preflight/recheck, cumulative audio
quotas and per-connection known-asset maps. A stale author can retry with refreshed revision;
other guests and source takes survive. Stopping a guest releases only its connection; stopping
the host terminates them all, retaining last local copies. See
[the multiple-guest contract](../spec/lan-multiple-guests.md). Internet and automatic resume remain closed.
