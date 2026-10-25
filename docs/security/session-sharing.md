---
description: Security boundary of the offline multiplayer experiment and requirements before any remote session is exposed
read_when: multiplayer threat model, invitation security, peer authorization, session network boundary
type: system
status: draft
last_verified: 2026-10-25
covers:
  - crates/collaboration
---

# Session sharing threat model

## Current boundary

The experiment is an in-process API for trusted local Rust callers. It has no listener, remote
messages, token generator, files, relay or application entry point. Peer handles establish only
local bookkeeping, not remote identities. A caller already owns the host and project. Tests
exercise refusal rules; they do not establish network safety.

The host retains its canonical project and accepts only a tiny, audited subset of commands.
Membership, role, IDs, parameter keys, numeric ranges and sequence/revision checks precede
execution. Peers and acknowledgement caches are bounded. A removed peer cannot retry through
the cache. The owner's permissions cannot be downgraded by the guest API. The low-level audio
path is unchanged. All API calls run on a control thread, never a render or device callback.

## Before remote inputs

A separate reviewed design and implementation must provide all of the following:

- Authenticated encrypted channels and host identity bound to an invitation, using reviewed
  cryptography; an unpredictable expiring/revocable invitation, no secrets in diagnostics.
- A capability map for every exposed command and read. Guests cannot change policy, local
  settings, plugins, library folders or microphones. Batch/nested execution must preserve policy.
- A bounded, version-negotiated wire schema; limits before parsing/allocating; rates, timeouts,
  resource quotas and saturation handling. The trusted project clone API is not a wire decoder.
- Replay and reconnect rules tied to authenticated session identity, not guessable numeric handles.
  Persistent or expiring credentials and a bounded journal need an explicit lifetime contract.
- A project/asset boundary with path and symlink confinement, size/hash checks, atomic finalization,
  disk quotas and recovery, aligned with the project folder/manifest work (#606).
- Tests of forged/replayed invitations, incompatible versions, malformed/flooded requests,
  revoked peers, hostile paths and media, interruption and partial recovery.
- An explicit LAN/direct/NAT/relay policy. No account requirement or implicit public infrastructure;
  a discovery result does not authorize admission. No network work on an audio thread.

No application should expose this prototype over a socket until these requirements and the
[session sharing spec](../spec/session-sharing.md) are implemented and reviewed. Recording media,
plugins and arbitrary project snapshots can contain private paths or untrusted state; they are
not permitted remote inputs of this experiment.
