---
description: Revision-checked creation and removal of top-level FM Synth and Utility devices in restricted LAN sessions
read_when: shared device chain, multiplayer protocol 10, device.addShared, device.removeShared
type: spec
status: draft
last_verified: 2026-10-28
covers:
  - crates/session/src/shared_devices.rs
  - crates/session/src/commands/device.rs
  - crates/session/src/projects.rs
  - crates/collaboration/src/policy.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
  - crates/ui/src/app/multiplayer_devices.rs
  - crates/ui/src/app/multiplayer_device_chain.rs
---

# Shared device chains

Self-authored fork behaviour extending the guarded [parameter contract](lan-device-parameters.md).

## Native commands

`device.addShared` takes exactly numeric nonzero u32 `track` and `kind`, either fm-synth or
utility. The target channel must contain only parameter-only FM Synth/Utility, no opaque state.
Utility is appended. FM Synth is inserted before the effects, only on a MIDI-capable track
without an existing instrument; it never replaces an instrument, even a disabled one.
New devices use native defaults and are enabled. No preset, position, group path or asset is
accepted. Limits are 16 devices per channel and 128 total, matching the portable copy.

`device.removeShared` takes exactly numeric nonzero u32 `track` and integer `device` 0..15.
The target must be an existing top-level parameter-only FM Synth/Utility in an allowed chain.
It removes that device and its settings/automation, preserving other devices and all tracks,
clips, notes, collected sounds and hardware bindings. Removing FM Synth retains the notes;
the native engine supplies its basic sound when no instrument remains.

Both pure guards delegate to the existing native add/remove handlers after validation. They
perform no file/preset loading or plugin selection. Each successful edit is one native undo
step; ordinary undo remains blocked during sharing. Existing graph updates reach each player's
current instance and retain its independent transport. Structural edits can change the target
track's sound, unlike a parameter message; other tracks and original local tabs are retained.

## Ordering, identity and desktop

Protocol 10 adds strict WireEdit variants for these two commands to the same role/sequence/base
revision checks, detached portable preflight and 32-operation journal. Refusals change no
accepted counter. Current native file version 3 and portable-copy format 2 stay unchanged.
Protocol 9 and earlier are refused before document transfer.

Devices have no stable IDs in the current model: their top-level indices are revision-bound.
After **any confirmed device.addShared or device.removeShared**, the desktop clears every
selected device/parameter draft and requires explicit device selection. This applies even when
the page is closed, another track changed, the same index still exists, or the last device was
removed. Refresh must never redeem an old draft against the replacement at its former index.
Parameter-only changes keep the normal explicit refresh behaviour.

In **Multiplayer → Devices**, choose **Device track**, then **Add FM Synth** or **Add Utility**.
A new revision disables these buttons until **Refresh chain draft**; changing the track starts
an explicit new draft. FM Synth is disabled on non-MIDI tracks or when an instrument exists.
**Shared device → Remove selected device** submits removal at the device draft's revision.
A stale or in-flight draft cannot remove. The UI explains that settings are removed and MIDI
notes remain. Empty chains still expose addition controls. New devices then appear in the
existing SDK-generated parameter selector and controls.

## Validation and remaining work

Native tests cover exact keys/types, unsupported kinds/state, placement without replacement,
quotas, atomic refusal, undo, unchanged project/clip identities, audio and player continuity.
Trusted core and real TLS tests cover both authors, stale indices, exact replay, quota preflight,
parameter edits after structural changes, matching music/renders and current late joins.
Desktop tests click add/remove and verify invalidation across index shifts, repeated same-kind
devices, hidden pages and empty chains while retaining source tabs. GPU captures review controls.

Reordering, nested groups, other devices, plugins, presets, device enable/disable and shared
history remain separate work. Physical two-computer acceptance is deferred by the owner.

## Sources

- [Native device handlers](../../crates/session/src/devices.rs), placement and removal semantics.
- [LAN contract](lan-starter-session.md), admission and operation ordering.
- [Device parameter contract](lan-device-parameters.md), SDK drafts and guarded scalar edits.


## Compatibility amendment: Suhina update of 28 October

[Protocol 13](lan-upstream-20261028.md) supersedes earlier version pins in this contract: native
project v4, portable-copy format 3 and matching pinned Suhina engine identities are required.
Ordinary old project files remain readable through the studio reader, then share as v4. The
musical edit permissions and quotas above remain unchanged; newly added studio effects are not
implicitly permitted in shared tabs. All participants need the updated combined application.
