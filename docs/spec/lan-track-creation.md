---
description: Revision-checked audio and MIDI track creation in a connected LAN project, with deterministic identities and local inputs off
read_when: multiplayer shared tracks, protocol 10, add shared audio track, MIDI FM Synth track
type: spec
status: draft
last_verified: 2026-10-27
covers:
  - crates/collaboration/src/policy.rs
  - crates/multiplayer
  - crates/session/src/files.rs
  - crates/session/src/commands/track.rs
  - crates/session/src/projects.rs
  - crates/ui/src/app/plugin_host.rs
  - crates/ui/src/app/multiplayer.rs
  - crates/ui/src/app/multiplayer_tracks.rs
---

# Add tracks during a shared session

Self-authored fork behaviour. Protocol 8 adds one typed operation, track.createShared, to the
existing host-ordered journal. Both the host and admitted editor guest may request it. The
ordinary track.add command remains closed over LAN: its optional device, preset, SDK graph,
sample and grouping arguments are not exposed. Native project format 3 and copy format 2 stay.

The operation has exactly two fields: kind is audio or midi, and name is a nonblank UTF-8
string of at most 256 bytes with no control characters. No supplied track/channel/lane IDs,
device paths, plugins, arbitrary devices, groups, hardware input or mappings are accepted.
The canonical host preflights native creation and the complete portable-project quotas before
consuming a revision, IDs or author sequence. The session still has at most 64 tracks including
returns/groups, 128 channels, 128 devices, 512 arrangement lanes and 128 scenes. Stale requests
are refused; retry/delivery rules stay unchanged.

The native command reuses regular track insertion: top-level, after existing regular tracks
and before returns, routed to the main output if present, with one empty arrangement lane and
one launcher column containing an empty slot per existing scene. Track/channel/lane identities
are allocated identically on all replicas. Audio tracks have no device. MIDI tracks have a
parameter-only default FM Synth, so subsequently created notes are audible without plugin
exchange. MIDI channel volume is 0.2; audio volume is 1.0. Both inputs and monitoring are off.
The new track has a fixed explicit type colour so personal creation preferences do not choose
its colour. No existing clips, routing, device states or original project tabs are replaced.

The same pure command is applied to the detached guest mirror and the existing restricted
desktop session. It is one native undo step when used locally; while connected only the host
operation journal changes the shared tab. Existing player objects and playback positions remain.
Late admission includes created tracks, channels, devices and their current clips. Existing
controls verify the resulting ordered track identities after operations, never resize the old
model implicitly. While a tab is bound to collaboration, the window must skip its local
track/clip creation preferences and inherited-colour normalisation; otherwise a UI frame can
mutate stored colours outside the journal. These preferences still apply to ordinary local tabs
and resume on the retained copy after leaving. New audio tracks immediately appear in the take-publication destination
list; new MIDI tracks accept existing shared clip/note edits.

Multiplayer gains a Tracks tab with a name field, Audio/MIDI choice, and Add shared track. A
draft freezes its revision; a newer revision requires Refresh track draft, which clears the
name. The button is disabled while pending, for invalid names, and at the 64-track limit.
Deletion, reorder/grouping, arbitrary instrument selection and hardware arming remain closed.

Checks cover both authors over TLS, deterministic model/IDs, stale/concurrent requests, exact
wire fields, track/channel/device/lane quotas without mutation, hardware off, empty slots,
new MIDI clip/note rendering, new audio track then take publication without reconnecting,
late join, independent attached players, visible controls and original-tab preservation.
Physical two-computer acceptance remains deferred at the owner's request.


## Compatibility amendment: Suhina update of 28 October

[Protocol 13](lan-upstream-20261028.md) supersedes earlier version pins in this contract: native
project v4, portable-copy format 3 and matching pinned Suhina engine identities are required.
Ordinary old project files remain readable through the studio reader, then share as v4. The
musical edit permissions and quotas above remain unchanged; newly added studio effects are not
implicitly permitted in shared tabs. All participants need the updated combined application.
