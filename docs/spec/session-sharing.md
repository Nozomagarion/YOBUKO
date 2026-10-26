---
description: Contract of the fork's trusted in-process collaboration experiment and the remote capabilities still absent
read_when: multiplayer prototype, host revision, duplicate operations, independent players, collaboration status
type: spec
status: draft
last_verified: 2026-10-25
covers:
  - crates/collaboration
---

# Session sharing

## Status

This document describes the trusted in-process collaboration core. Two existing Suhina sessions
can exchange permitted edits through a host. The separate [LAN starter adapter](lan-starter-session.md)
now reuses its allowed edits behind authenticated TLS, without serializing Project snapshots.
The original core experiment is specified by [ADR 0030](../decisions/0030-multiplayer-local-spike.md).

## Project and peers

A host owns one canonical session without a player; its owner also listens through a replica.
A host owns one session and a monotonically increasing revision, initially zero. It allocates
peer handles locally. The owner is an editor; additional peers are readers or editors, up to
16 peers including the owner. Only the trusted host caller can change a role or remove a guest.
Handles are not authentication credentials. Removing the owner is refused. Handles are never
reused. A removed peer's later requests are refused before any cached result is returned.

A snapshot is the revision and a clone of the current project, for trusted in-process use.
A fresh replica starts from that snapshot. This is not a safe remote import, a file transfer
format or a resumption protocol. Applying a snapshot means explicitly constructing a new replica;
a live instance is never silently replaced to conceal an ordering error.

## Allowed shared edits

Only `track.rename`, `track.setVolume` and `track.setPan` are accepted. The command registry
must also identify each as an Edit. Parameters have the exact keys needed by that command:

- `track.rename`: `track` is a nonzero `u32` id and `name` is nonempty text of at most 256 bytes.
- `track.setVolume`: `tracks` is one to 64 nonzero `u32` ids; `db` is a finite number from -150
  to +6. The prototype does not accept the textual silence shortcut.
- `track.setPan`: the same `tracks`, and finite `pan` from -1 to +1.

Unknown keys, name-based addressing, batches, files, plugins, history and local transport are
refused as shared edits. No `continue` gesture is admitted. Normal session validation still
checks object existence and model invariants. Refused preconditions do not execute a command.
This subset creates no new IDs and performs no file I/O. An accepted no-op still advances the
protocol revision, but records no extra undo step in the underlying session.

## Ordering and retries

Each request carries a per-peer sequence starting at one and the base project revision. The
host checks membership and role, the command and its parameters, then the request sequence and
revision before executing. Its own edits follow the same path. Accepted edits advance the host
revision by one and the author's next sequence by one, with checked arithmetic.

Each peer retains only its last accepted request and acknowledgement. The identical request
repeated immediately receives that acknowledgement without executing again, including its
original base revision. Reusing that sequence with different data is refused. Requests older
than the cache, future sequence gaps and stale base revisions are refused without mutation.
The caller must explicitly inspect/refetch state and construct the appropriate new request.
Rejected requests do not consume a sequence number. There is no persistent replay protection.

Snapshots and confirmations carry a shared opaque in-process origin. A confirmation from another
local host is refused even if its revision and project happen to match. This is not a network
identity.

A replica accepts only the next revision and an allowed command; an identical repeat of its last
applied acknowledgement is ignored. A gap, older acknowledgement or different data under the same
revision is refused without changing its session. The host's acknowledgement is constructed by
the host type, not deserialized from arbitrary JSON in this experiment.

## Independent audition

Each host/replica can attach its own existing `Player`. Local audition admits only empty-parameter
`transport.play` / `transport.stop`, and `transport.setPosition` with a finite `beats` from 0 to
120000. The session registry still executes and validates these commands. A local audition never
increments the shared revision, changes another player's position or enters the shared edit path.
Tests render separate engines at different positions, propagate edits and stop one independently.
Actual sound-card latency, a network connection and cross-machine clocks are not tested by this.

If an underlying session edit/player notification fails, the replica refuses further confirmations
until explicitly reconstructed from a fresh trusted snapshot. This avoids silently retrying an edit
whose model change may have preceded a player queue failure. The existing player can still be
stopped locally; automatic seamless recovery is not implemented.

## Not yet

Authentication, invitation and wire bounds exist only in the separate restricted LAN adapter.
Persistent resume, presence, linked transport, project assets, recording publication, file
confinement, host migration and offline multi-master editing remain absent. The trusted core
alone does not make a listener safe. The [threat model](../security/session-sharing.md) distinguishes
the starter boundary from the project/media boundary that remains closed.
