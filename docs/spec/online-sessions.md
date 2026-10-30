---
description: Internet sessions joined with an invitation code, peer to peer, without accounts or a server of ours
read_when: online multiplayer, invitation code, joining from another city, relay, hole punching, iroh transport
type: spec
status: draft
last_verified: 2026-10-29
covers:
  - crates/multiplayer/src/online.rs
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/invitation.rs
  - crates/ui/src/app/multiplayer.rs
---

# Online sessions

Self-authored extension of the host-ordered session adapter
([several guests](lan-multiple-guests.md)). The collaboration protocol, its limits and its
admission rules are unchanged; only the transport between computers changes. Protocol 15
(14 before [heavy sessions](heavy-sessions.md), 13 before [linked playback](linked-playback.md)
added the shared transport).

## What a person does

1. The host picks **Over the Internet** (the default) and creates a session or shares a project.
2. The host copies the invitation code (`suhina-net://…`) and sends it through any chat.
3. The guest pastes the code and joins. The two computers may be in different cities, behind
   ordinary home routers, with nothing to configure and no account anywhere.

**On this local network** keeps the existing private-address TLS transport unchanged.

## Transport

Each session side opens an [iroh](https://github.com/n0-computer/iroh) endpoint (MIT OR
Apache-2.0) with a fresh secret key, created for the session and never stored. Connections are
QUIC with TLS 1.3 under the `ring` provider, application protocol `suhina/multiplayer/15`.

- The host waits up to ten seconds for a home relay, then puts its endpoint id (public key),
  relay URL and direct addresses in the invitation. Without a relay it still invites with its
  direct addresses.
- iroh first tries a direct path through both routers (hole punching). When routers block it,
  traffic goes through a public relay run by the iroh project. Relays forward end-to-end
  encrypted packets and cannot read the session.
- The guest dials the endpoint id from the invitation. The QUIC handshake proves the host holds
  that key, which replaces the LAN invitation's pinned certificate. A forged address under
  another key fails the handshake.
- One bidirectional stream per guest carries the same length-prefixed frames as the LAN
  transport, with the same exchange deadlines (3 s per exchange, 30 s for the initial
  document, 60 s per sound chunk).

## Invitation code

`suhina-net://` followed by unpadded URL-safe base64 of JSON `{protocol, endpoint, secret}`:
the protocol number, the iroh endpoint address and the 64-hex single-use secret. At most
8192 characters and 32 addresses; unknown fields are refused. The secret keeps the LAN rules:
valid five minutes, admits one guest, one outstanding invitation, rotated with **New
invitation** while fewer than three guests are connected. The endpoint stays the same for the
whole session, so a renewed code differs only in its secret.

## Limits

- At most four handshakes at once (three guests plus one being admitted); further incoming
  connections are refused before any data is read.
- Sound budgets, frame sizes and journal length are those of the LAN transport.
- No reconnection after a cut, no host transfer, no live audio between computers: each
  computer plays the project on its own, as on the LAN; [linked playback](linked-playback.md)
  only starts them together.
- `online-loopback`, accepted wherever an address is, binds 127.0.0.1 without relays or
  address lookup. It serves tests and two instances on one computer.

## Tests

`crates/multiplayer/src/online_tests.rs` covers code round trips and tampering, deadline
timeouts that lose no data, and a forged host key. `crates/multiplayer/tests/online.rs` runs
whole sessions over loopback: edits in both directions, a stale edit refused, the host leaving,
three guests with fresh codes, and a spent code refused.
