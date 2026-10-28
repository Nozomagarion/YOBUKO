---
description: Revision-checked scalar parameter edits for existing FM Synth and Utility devices in restricted LAN sessions
read_when: shared device controls, multiplayer protocol 10, FM Synth, Utility, device.setSharedParam
type: spec
status: draft
last_verified: 2026-10-27
covers:
  - crates/session/src/shared_devices.rs
  - crates/session/src/commands/device.rs
  - crates/session/src/projects.rs
  - crates/collaboration/src/policy.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer_devices.rs
  - crates/ui/src/detail/detail_view/parameters.rs
---

# Shared device parameters

Self-authored fork behaviour, extending the existing native parameter path and SDK metadata.

## Native command and admission

`device.setSharedParam` takes exactly numeric nonzero u32 `track`, integer `device` 0..15,
text `param` (1..64 ASCII letters/digits, dot or underscore), and finite numeric `value`.
The device is a zero-based index in that track's top-level channel chain, not a stable ID.
The host checks the draft revision before execution; structural edits follow the guarded [chain contract](lan-device-chains.md).
Name/path addressing, nested groups, gestures, batches, files and preset/state loading are refused.

Only existing parameter-only built-in FM Synth or Utility devices with no opaque state are
accepted. The parameter must exist in the native registry and the value must already equal
its canonical clamp: in range, integer for integer/choice parameters, 0 or 1 for toggles.
Unknown parameters, fractional choices and out-of-range values fail without mutation. At most
512 stored parameters are allowed. Generic device.setParam remains outside the shared allowlist.

The pure guard delegates to the existing native setter only after validation. This preserves
parameter messages to the existing player, native undo semantics and manual override of the
same parameter's automation lane. No plugin process, asset/path loading or audio-thread I/O
is introduced. A disabled device accepts stored values but does not become enabled.

## Wire and desktop

Protocol 9 adds the strict WireEdit variant device.setSharedParam with those four fields.
Both authors use the same role, base revision, sequence/replay, detached portable preflight and
32-operation journal as the other edits. It consumes one revision on acceptance. Current
portable-copy format 2 and native project version 3 stay unchanged. Protocol 8 and earlier
are refused at invitation/hello before project transfer.

The **Devices** page lists supported devices from the bound shared project. Its parameter
list and control come from the existing SDK panel/registry, including ranges, units, defaults,
integers, choices and toggles. **Find parameter** filters by name/id/group. Selecting a device
or parameter creates a draft at the observed revision. Editing the widget changes only the
local draft; **Apply shared parameter** submits one command. Confirmed values are separate.
A stale revision disables Apply until **Refresh device draft** reloads the confirmed value;
a confirmed add/remove discards the entire selection and requires explicit device selection,
so an old index can never redirect a draft to its replacement;
selecting another target intentionally creates a new draft. An in-flight edit disables Apply.

Ordinary device panels/automation cannot bypass the shared mutation gate. The authoritative
Projects API admits the guarded command, retaining its native validation. Neither a stale draft
nor a deleted/unsupported target silently redirects to another device. Rejoining obtains the
current parameters in a new verified snapshot; old local copies and playback positions survive.

## Validation and limits

Native tests cover both devices, types/ranges, unsupported/opaque devices, exact addressing,
atomic refusal, automation override, undo and parameter-message playback. Trusted-core tests
cover roles, stale requests, exact replays and unchanged author counters on refusal. Real TLS
tests cover bidirectional edits, matching renders, independent players, late join/reinvitation,
invalid fields and portable preflight. Desktop tests click Apply and Refresh, retain source tabs,
show current confirmed values and refuse stale drafts. A separate GPU capture checks the panel.

Reordering/enabling devices, other instruments/effects, external plugins,
presets, automation editing, continuous shared gestures and simultaneous multi-parameter edits
remain separate work. Physical two-computer acceptance is still deferred by the owner.


## Sources

- [Native device decision](../decisions/0004-built-in-devices.md), metadata and parameter-message semantics.
- [SDK panel implementation](../../crates/device-sdk/src/builtin.rs), generated parameter controls.
- [Native setter](../../crates/session/src/devices.rs), manual parameter and automation override behaviour.
- [LAN session contract](lan-starter-session.md), authentication and revision ordering.
