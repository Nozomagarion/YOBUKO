---
description: A fork-only, offline experiment in host-ordered edits with independent local players, before any network exposure
read_when: multiplayer fork, local collaboration experiment, ordered edits, independent playback
type: decision
status: proposed
built: partly
last_verified: 2026-10-25
covers:
  - crates/collaboration
---

# 0030: An offline collaboration experiment in the multiplayer fork

## Context

This fork explores the user's requested coediting and independent playback. It does not claim an
upstream issue, approve an upstream architecture, or implement the network session proposed by
[ADR 0023](0023-collaboration-and-project-versions.md). Object identity is already available from
[ADR 0025](0025-object-identity.md). The project folder and manifest remain the prerequisite in
[issue #606](https://github.com/suhina-daw/suhina-daw/issues/606) for sharing media.

The first question can be exercised without a listening socket, authentication, a changed project
format, or a changed audio callback: can two local sessions apply the same narrowly permitted
edits in host order while their existing players keep independent positions?

## Decision

1. **Isolate the experiment.** A layer-3 `collaboration` crate depends on `session` and the model;
   applications may use it only through the separate restricted LAN adapter. The core has no
   sockets, wire parser or invitation code.
   Its trusted in-process callers are not authenticated network peers.
2. **Reuse commands.** Supported edits call `Session::execute` on the host and each replica.
   Begin with track names, volume and pan only. No allocation of new object identities,
   file effects, arbitrary commands, batches, continuous gestures, undo or project replacement.
3. **Order edits on one host.** The host and guests submit through the same revision and per-peer
   operation checks. A stale base revision is refused. One cached acknowledgement per peer makes
   an immediate retry idempotent; older operations require an explicit fresh snapshot. This is
   deliberately bounded, not a durable journal or an offline merge algorithm.
4. **Keep playback local.** The existing play, stop and position commands act only on that
   instance. Replica edits update its existing session and player. No replacement of the session
   or remote transport state is needed for this subset. Test actual renderers, not just numbers
   in a presence struct.
5. **Separate the remote boundary.** The [LAN starter spec](../spec/lan-starter-session.md)
   defines authenticated TLS, expiring single-use invitations and typed four-track controls.
   The [threat model](../security/session-sharing.md) defines its limits. Full Project snapshots
   remain trusted-only. Before media exchange, reuse #606 instead of inventing another manifest.
   Before wider edits, address ID allocation and side effects.
   *Amended 2026-10-26, see Amendment: restricted LAN adapter.*

What is built: [session sharing specification](../spec/session-sharing.md) and
[the fork guide](../multiplayer-fork.md).

## Consequences

The experiment can test command convergence, permission refusals, duplicates, stale requests and
independent renderers entirely offline. The separate starter adapter adds desktop LAN control
sessions, but no Internet connection, media exchange, recording publication or linked transport.
These remain explicit later slices. No user data or project schema migration is needed.

The command subset is intentionally exact, including its accepted parameter keys. New commands
must be audited rather than admitted by a namespace prefix. The crate must not be wired to a
socket as though this prototype provided authentication or safe untrusted document loading.

## Alternatives considered

- A network listener immediately: deferred until the security and transport decisions are ready.
- A new command engine or a CRDT: unnecessary for this test and much harder to integrate upstream.
- Replacing each replica's session after every edit: would discard its live player and local state.

## Amendment: restricted LAN adapter (2026-10-26)

Fork-owner continuation of feature/multiplayer amends decisions 1 and 5. A separate layer-3
`multiplayer` crate uses the trusted host without exposing its project clone API. A generated MIDI starter or an explicitly validated, bounded MIDI-only copy can participate,
with three live track controls and single-note add/update/delete. The [MIDI-copy contract](../spec/lan-midi-copy.md) adds the native
format/subset, canonical encoding, hash/chunk/deadline bounds and original-project preservation. The adapter
owns authenticated TLS, bearer invitation lifetime, frame/deadline/queue bounds and guest identity.
The desktop owns a locked dedicated project and applies host-confirmed controls through commands.
The [note-editing contract](../spec/lan-note-editing.md) adds atomic absolute note updates,
revision-bound indices, detached quota preflight and a 32-operation delivery journal. Native
copy-on-write content IDs are allocated deterministically by the same host-confirmed command
on each replica; the [clip contract](../spec/lan-clip-editing.md) extends the same approach to six bounded
arrangement operations under protocol 4. Stack-only placement protects neighbours; new tracks
and arbitrary project mutations remain closed. Admission
snapshots current music with its revision. Existing projects and private playback are retained. Arbitrary project/media exchange remains
closed; this is a fork prototype, not an accepted upstream architecture or external security review.

## Sources

- [ADR 0002](0002-commands.md), commands shared by all front ends.
- [ADR 0005](0005-edits-while-playing.md), edits reaching the existing player.
- [ADR 0023](0023-collaboration-and-project-versions.md), the collaboration proposal.
- [Issue #568](https://github.com/suhina-daw/suhina-daw/issues/568), object identity, completed.
- [Issue #606](https://github.com/suhina-daw/suhina-daw/issues/606), project folder and assets.
