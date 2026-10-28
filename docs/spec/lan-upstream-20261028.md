---
description: Integrating Suhina 9717cc7 with native v4 automation while preserving LAN and DJ modules
read_when: upstream October 2026 integration, native format 4, portable format 3, protocol 13, engine identity
type: spec
status: draft
last_verified: 2026-10-28
covers:
  - fork-sources.json
  - crates/multiplayer/src/midi.rs
  - crates/multiplayer/src/protocol.rs
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/transfer.rs
---

# Suhina source update, 28 October 2026

This fork integrates the already merged official main at
9717cc717537010de76554062c64efab895f81cc, preserving merge ancestry from the previous pin
3b242ba649c5200d6a41ba6b64943a2752013b35. Open PRs including Saturator #1040 and XY #1028
are excluded. The rbxport analysis pin stays unchanged. No installed app or source project is replaced.

Official changes include eight-band EQ, DJ EQ/import refinements, device panels including FM
Synth, Bezier automation with native project format 4, ARM plug-in-host bindings, plugin-load
fixes and interface dependencies. The fork retains the exact limited FM Synth/Utility sharing
policy: the newly available studio effects are not automatically permitted in shared tabs.

Protocol 13 uses portable-copy format 3 and exact canonical native v4 JSON. Ordinary studio
opening of old native files continues through the upstream versioned reader; sharing first
serializes their in-memory model as v4. Old portable/v3 payloads, old invitations and mixed
protocols are refused before opening a shared tab. Saving new projects writes v4; older apps
cannot read those newly saved files. Original files are not rewritten during sharing.

Hello also names the exact pinned Suhina engine revision, compiled from fork-sources.json.
A missing, malformed or unequal identity is refused before admission, document transfer or
secret consumption. This is public source identity, not another authentication credential.
The pinned TLS certificate and single-use secret remain required. Every participant must use
this combined updated build; identical parameters must not hide different upstream engines.

Existing automation curves must survive canonical transfer, save/reopen and parameter override
without flattening or loss. Shared scalar parameter commands retain native override semantics.
No remote curve editor, new device permission, transport synchronization or live streaming is added.
Up to three guests, independent players, checked audio transfers, source tabs and DJ catalog stay.

Validation: migration of an authored v3 curve to v4; native v4 Bezier roundtrips and malformed
curve refusal; mismatched engine/format refusal without consuming a valid token; real TLS
with four participants, verified audio, independent renderers and parameter override; native
EQ/DJ EQ/registry checks and visual review of the updated app. Workspace checks and package
extraction/signatures are required. Physical computer testing remains deferred by the owner.
