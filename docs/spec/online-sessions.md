---
description: Internet sessions joined with an invitation code, peer to peer, without accounts or a server of ours
read_when: online multiplayer, invitation code, short code, joining from another city, relay, hole punching, iroh transport
type: spec
status: draft
last_verified: 2026-10-30
covers:
  - crates/multiplayer/src/online.rs
  - crates/multiplayer/src/short_code.rs
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/invitation.rs
  - crates/ui/src/app/multiplayer.rs
---

# Online sessions

Self-authored extension of the host-ordered session adapter
([several guests](lan-multiple-guests.md)). The collaboration protocol, its limits and its
admission rules are unchanged; only the transport between computers changes. Protocol 16
(14 before [heavy sessions](heavy-sessions.md) and [session resume](session-resume.md), each
developed as 15 on its own, 13 before [linked playback](linked-playback.md) added the shared
transport).

## What a person does

1. The host picks **Over the Internet** (the default) and creates a session or shares a project.
2. The host copies the invitation code (`suhina-net://…`) and sends it through any chat.
3. The guest pastes the code and joins. The two computers may be in different cities, behind
   ordinary home routers, with nothing to configure and no account anywhere.

**On this local network** keeps the existing private-address TLS transport unchanged.

## Transport

Each session side opens an [iroh](https://github.com/n0-computer/iroh) endpoint (MIT OR
Apache-2.0) with a fresh secret key, created for the session and never stored. Connections are
QUIC with TLS 1.3 under the `ring` provider, application protocol `suhina/multiplayer/16`.

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

## Short codes

The long code is hard to read out, so the host also gets a short one such as
`7-piano-tigre-lune`: a number from 1 to 999 and three words from a list of 2048
(`crates/multiplayer/src/short_code_words.txt`). The list is self-authored for Suhina (common
French words, written from scratch, no list copied), stored without accents, 3 to 9 letters,
sorted, without duplicates, numbers or obvious homophones of another entry. It never changes
order: a code is the positions of its words. Still no account and no server of ours.

- **Derivation.** Both sides stretch the code's canonical text with Argon2id (32 MiB, three
  passes, fixed salt `suhina/online/short-code/v1`), then split the result with BLAKE3
  `derive_key` under two distinct contexts: an iroh secret key for a *lobby* endpoint and a
  32-byte *lobby proof*.
- **Host.** Besides its session endpoint, the host binds a small lobby endpoint under the
  derived key, application protocol `suhina/lobby/1`. On the Internet the lobby publishes its
  address through iroh's ordinary public address lookup (pkarr records served over DNS by the
  iroh project), like any iroh endpoint. The code reaches the view once the lobby has a home
  relay; the long invitation is shown at once and stays available.
- **Guest.** The guest derives the same key, dials the lobby by its endpoint id alone (the
  address lookup finds it; the guest retries for up to 20 s while a fresh record propagates)
  and sends the proof over that QUIC connection. The handshake proves the lobby holds the key
  derived from the code; the lobby compares the proof in constant time. For the first correct
  proof it answers with the full invitation (u16 big-endian length, then the `suhina-net://`
  text, at most 8192 bytes); for any other it answers a zero length. The guest then joins the
  session endpoint exactly as with the long code, single-use secret included.
- **Life.** A lobby serves one code. It admits one guest, then refuses everyone, and closes
  after five minutes, when the host leaves, or when **New invitation** replaces it with a new
  code and a new lobby. A wrong proof does not spend the code.
- **Join field.** Text starting with a digit is a short code, `suhina-net://` a long online
  code, `suhina-lan://` a LAN code. Case, accents and the separators between parts (spaces,
  hyphens, dots, commas, underscores) do not matter. A malformed code (wrong number, unknown
  word, wrong count) is refused before any network use, naming the unknown word.
- **State.** `multiplayer.state` exposes `invitation` (long) and `short_code` (null until the
  lobby is reachable, on the LAN, and after leaving). The view shows the short code with
  **Copy code**, and **Copy full invitation** as a fallback, for example when a network blocks
  the public address lookup.
- **Loopback.** `online-loopback` lobbies take number 0, which Internet codes never use, and
  are found through an address directory kept in this process instead of the public lookup.
  Their codes therefore work only between sessions of one process (the tests); two instances on
  one computer use the full invitation.

### Why about 43 bits are enough

An Internet code carries log2(999 × 2048³) ≈ 43 bits. The only way to use a code is to ask its
lobby online: an attacker guesses a code, derives its lobby id, and must find a live lobby
under it through the public lookup, then race the guest to it, all within five minutes and
before the single use is spent. Even at a thousand lookups a second, an attacker covers
300 000 codes per five-minute window, a chance of about 3 × 10⁻⁸ for one outstanding code
(still about 3 × 10⁻⁵ if a thousand codes were outstanding worldwide at once). The derived
lobby id is public in the address records, so the code must not be recoverable from it
offline: Argon2id makes every guess cost 32 MiB and about 70 ms of a recent processor core, which puts the
2⁴³ space far beyond any five-minute window. A guess of the proof itself is a guess of 256 bits.
The person who reads the code out still chooses who hears it: anyone who hears it within the
five minutes can take the seat, exactly as with the long code.

## Limits

- At most four handshakes at once (three guests plus one being admitted); further incoming
  connections are refused before any data is read.
- Sound budgets, frame sizes and journal length are those of the LAN transport.
- A guest whose link drops comes back by itself within a minute ([session resume](session-resume.md));
  a refused guest is told why. No host transfer, no live audio between computers: each
  computer plays the project on its own, as on the LAN; [linked playback](linked-playback.md)
  only starts them together.
- `online-loopback`, accepted wherever an address is, binds 127.0.0.1 without relays or
  address lookup. It serves tests and two instances on one computer.

## Tests

`crates/multiplayer/src/online_tests.rs` covers code round trips and tampering, deadline
timeouts that lose no data, a forged host key, and the lobby: one hand-over, a forged proof
refused without spending the code, unknown and withdrawn codes. `short_code_tests.rs` checks
the word list (2048 distinct plain words, sorted), round trips whatever the case, spacing or
accents, malformed codes, and the derivation. `crates/multiplayer/tests/online.rs` runs whole
sessions over loopback: edits in both directions, a stale edit refused, the host leaving,
three guests with fresh codes, a spent code refused, and short codes: a join by short code
alone, single use, a wrong code, renewal revoking the old code, malformed codes refused before
any network use. All run offline. Degraded links, cuts and resumes are tested with the fault
injector described in [session resume](session-resume.md).
