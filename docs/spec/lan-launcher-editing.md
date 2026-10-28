---
description: Host-ordered shared scene and launcher-slot editing with private local launches
read_when: shared launcher grid, scene editing, protocol 11, copying arrangement clips to slots
type: spec
status: draft
last_verified: 2026-10-28
covers:
  - crates/session/src/shared_launcher.rs
  - crates/collaboration/src/policy.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer_launcher.rs
  - crates/ui/src/app/multiplayer.rs
---

# Shared launcher organization

Self-authored fork behaviour, extending the existing native launcher model and commands.
Protocol 11 requires matching applications; native project version 3 and portable format 2 stay.

Both participants organize scenes through scene.insert (explicit position and bounded name),
scene.rename, scene.duplicate and scene.delete. Scene positions are one-based and revision-bound:
1..128, insertion up to 129 only to let detached quota validation refuse growth past 128.
The last remaining scene cannot be deleted. Duplication copies clips and independent contents.

clip.createSharedSlot creates an empty native clip of the destination track's kind with an
explicit positive length. clip.copyToSharedSlot copies a stable source clip ID from arrangement
or launcher to an existing empty destination slot; it preserves its musical settings, gives it
new clip/content IDs and sets its placement start to zero. The original stays intact. Copied
notes are independent; audio reuses the existing verified asset without another file transfer.
Both commands take numeric track and scene identifiers only; destination scene is 1..128.
Occupied/missing/incompatible targets fail before mutation. Copy currently supports single-track
contents only (all lane targets None); explicitly routed multi-track patterns are refused rather
than unexpectedly playing their original tracks. No implicit scene growth or overwrite.
clip.moveSlot explicitly moves, or swaps two occupied compatible slots using native semantics;
clip.delete clears a slot by stable clip ID. Existing note edits also accept launcher clip IDs.

The Launcher page shows a paged grid of confirmed scene names and clip IDs, plus revision-bound
scene/slot drafts. Refresh discards the draft, and any confirmed structural scene edit clears
all launcher selections even when this page is hidden. No stale index can silently retarget a
later row. A move button says that occupied targets swap; copy never overwrites.

All accepted changes use the same host preflight, exact policy, contiguous journal, native
session notification and portable quotas (128 scenes, 512 contents, 4096 clips, 16384 notes,
2 MiB document). Refusal does not consume a sequence or revision. Native history stays local
and unavailable in the restricted tab. Original tabs, verified assets and attached players stay.

Launching/stopping a selected scene or slot is local transport only, never a wire edit; it
uses the existing native command on the bound active tab. Players may react to deleted/moved
material as in the studio, but their clocks are not synchronized or replaced. Other scene
tempo/signature settings, mappings, live streaming and Internet remain outside this extension.

Validation: native copy/undo/roundtrip and atomic refusals; policy malformed parameters; real
TLS in both directions, stale indices, duplication isolation, quota rejection, rejoin convergence,
independent renderers; UI clicks and hidden-page invalidation. Physical two-computer testing
remains deferred by the owner.


## Compatibility amendment: Suhina update of 28 October

[Protocol 13](lan-upstream-20261028.md) supersedes earlier version pins in this contract: native
project v4, portable-copy format 3 and matching pinned Suhina engine identities are required.
Ordinary old project files remain readable through the studio reader, then share as v4. The
musical edit permissions and quotas above remain unchanged; newly added studio effects are not
implicitly permitted in shared tabs. All participants need the updated combined application.
