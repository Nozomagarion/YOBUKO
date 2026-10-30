---
description: Multiplayer guests that come back by themselves after a dropped link, and the reasons a host gives when it refuses a guest
read_when: reconnection, resume token, connection lost, grace period, admission refused, version mismatch message, host ended the session, fault injection tests, protocol 16
type: spec
status: draft
last_verified: 2026-10-30
covers:
  - crates/multiplayer/src/resume.rs
  - crates/multiplayer/src/faults.rs
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
---

# Session resume and refusal reasons

Self-authored extension of the host-ordered session adapter ([online sessions](online-sessions.md),
[several guests](lan-multiple-guests.md), [linked playback](linked-playback.md)). It applies to
the Internet and the LAN transports alike. Protocol 16 and ALPN `suhina/multiplayer/16`,
shared with [heavy sessions](heavy-sessions.md); this change was developed as protocol 15 alongside it.

## What a person sees

- When the network drops under an admitted guest, its status reads **Reconnecting…** and Suhina
  tries again for about a minute, without a new code. Edits made meanwhile wait and are sent once
  the link is back. The host sees **N guest(s) reconnecting** and keeps their places.
- When the host leaves on purpose, the guest reads *The host ended the session. Your copy of the
  project is kept.* and does not retry.
- When the minute passes without success, the guest reads *Lost the connection to the host and
  could not reconnect…* and keeps its copy, as after any disconnect.
- A refused join says why and what to do, for example *Your partner runs a different Suhina build
  (host abc1234, this computer def5678). Install the same version on both computers.*

## Refusals

The host answers a hello it refuses with `Denied {refusal}`; `refusal.code` is one of:

| Code | When | Shown to the guest |
|---|---|---|
| `protocol` (`host`, `guest`) | the protocol numbers differ | different Suhina version, both numbers |
| `build` (`host`, `guest`) | the engine revision or the portable-copy format differs | different build, both short identifiers |
| `unsupported` | the hello cannot be read | install the same version |
| `invalid` | the code is not the session's current code (mistyped, replaced by a newer one) | ask for a new code |
| `expired` | the right code, after its five minutes | ask for a new code |
| `used` | the right code, already redeemed | each code works once |
| `full` | the right code, while three guests hold a place | wait for a guest to leave |
| `resume` | a resume token unknown, replaced or past its grace period | ask for a new code |

The host reads a hello first as plain JSON, so it can name the protocol of an older or newer
application before parsing it exactly. Versions are checked before the secret and refuse without
spending the code. Secrets and tokens are compared as SHA-256 digests in constant time; a wrong
code is always `invalid`, whatever its age or the session's size, so only the holder of the right
code learns that it expired, was used or that the session is full. A guest reads a refusal
leniently: an unknown reason, or a protocol 14 host's bare `Denied`, becomes `unsupported`.
Identifiers taken from the other side are cut to at most seven letters or digits before display.
An invitation whose `protocol` differs is refused before connecting with both numbers named.

## Resume

- **Token.** After admission the host sends `Admitted {protocol, token}`: 32 random bytes in hex,
  stored only as its SHA-256 digest, bound to the guest's PeerId. Each resume replaces it.
- **Membership.** The host keeps every admitted guest as a member: connected, or away until a
  deadline. Members count toward the three places; the state's `guests` counts connected ones
  only. A link that fails (an I/O error, a timeout or the end of the stream) makes the member
  away for **two minutes**; a guest that breaks the protocol or sends `Leave` is removed at once.
  At the deadline the PeerId is removed from the collaboration host and the token stops working.
- **Hello.** The guest sends `Resume {protocol, format, engine, token, seen_revision}` where
  `seen_revision` is the last revision it applied completely, or `null` after an update that
  failed halfway or while sounds of a [heavy copy](heavy-sessions.md) are still arriving in the
  background (their requests refer to the first copy's manifest; the sound cache keeps what
  already arrived, so the fresh copy only fetches the rest). The host compares the token with every member's digest without stopping early.
  It answers `Resumed {protocol, token, copy}`. With `copy: false` (the journal of 32 operations
  covers `seen_revision`) a state with the operations since then follows, with the sounds the
  member was known to hold; with `copy: true` the whole initial copy follows, as at admission, and
  the desktop replaces its shared tab with the new copy.
- **Same participant.** The PeerId, its sequence and retry cache stay. A connection that resumes
  while the host still serves the older one supersedes it: each connection carries a generation
  and the older handler stops at its next exchange without touching the member.
- **The unanswered request.** The guest keeps the request it was waiting on and sends it again
  first, with the same sequence. The collaboration host returns its cached confirmation for an
  exact retry, so an edit applied before the drop is applied once. The guest counts an accepted
  sequence and shows a refusal as soon as the answer arrives, before the update is applied, so a
  link lost during the update cannot repeat or hide either. A take sent again is accepted when it
  is already in the guest's copy.
- **Retries.** Waits of 0.25, 0.5, 1, 2 and then 4 seconds, for 60 seconds. A refusal stops at
  once and shows its sentence.
- **Goodbyes.** A host that stops sends `Ended`, then waits for the guest's next request so the
  link stays open until it is read; a guest that stops sends `Leave`. The desktop waits up to two
  seconds before shutting the remaining LAN sockets, and an online stream waits up to half a
  second for its last bytes to be acknowledged before the connection closes.

## Limits

The resume window is the host session itself: a host that quits or crashes ends every place. One
request is in flight per guest, so at most one edit, take or playback change is resent. A resent
playback change applies again. Clock estimates for linked playback carry over a resume.

## Tests

`crates/multiplayer/src/faults.rs` is a test-only `Link` wrapper: latency each time a link turns
between reading and writing, a bandwidth limit, a cut after a byte count, and a cut on demand that
interrupts a blocked read. A `Tuning` on the service (test timings and faults) wraps every link of
that side, online streams and LAN TLS streams alike. `faults_tests.rs` runs whole sessions over
loopback and LAN: edits both ways, a guest's take and linked playback at 150 ms and 64 KiB/s; a
lasting cut ending with the clear error, the host counting the guest out at once, its place freed
after the grace period and its state intact; a cut after an edit applied but before its answer,
resumed with the edit applied once; a guest away for more than the journal, resumed with a fresh
copy; a background sound cut midway and completed after the resume from its partial copy; the
host ending the session told apart from a lost network. `transport_tests.rs` checks every
refusal code, that wrong codes leak nothing, and token rotation, forgery, supersession and expiry.
