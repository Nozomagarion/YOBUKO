---
description: Bounded LAN sessions with one host and three independently invited guests
read_when: multiple LAN guests, invitation rotation while connected, participant count, protocol 12
type: spec
status: draft
last_verified: 2026-10-28
covers:
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
---

# Several LAN guests

Self-authored extension of the existing authenticated host-ordered LAN adapter. Protocol 12
requires matching applications; native project format 3 and portable-copy format 2 stay.

One host admits at most three guests (four musicians including the host). The canonical host
tracks authenticated PeerIds, not a single connected flag. Each connection has its own native
collaboration sequence/retry cache, delivery cursor and verified asset inventory. The existing
four-handler limit leaves one handshake slot when three guests are admitted. Document, asset,
request/frame/deadline and journal limits remain unchanged.

There is one outstanding invitation at a time. New invitation / multiplayer.renewInvitation
works while guests are present if fewer than three are admitted. It rotates the secret with
a new five-minute, single-use lifetime, invalidating the previous outstanding invitation, while
retaining all authenticated connections, peer identities, host sequence, canonical project,
journal, assets, host tab and player. Create and privately copy a new invitation for each next
guest. Do not create the next invitation before the previous recipient has joined; otherwise
their unused invitation is replaced. Invitations are neither public lists nor reusable room links.

Renewal and hello share the canonical mutex. Successful bounded event publication precedes
secret replacement, preserving the previous token under UI backpressure. Admission checks
expiry, one-use secret and guest capacity before consuming the invitation or allocating a peer.
A full session refuses renewal and admission without disconnecting anyone or consuming a valid
pending secret. Closing/failure of one authenticated handler removes only that PeerId. Other
players/connections and the pending invitation are retained. The departing app unlocks and
keeps its last local copy. It can rejoin with a new invitation into a new tab, without merging.
Stopping the host ends every connection, retaining each participant's local copy.

Every server State includes the count of currently admitted guests (including transfers in
progress). Guests require 1..3; the host can show 0..3. Membership changes emit state even when
musical revision and controls are unchanged. The UI shows Guests: N / 3 on host and guests;
New invitation is disabled at capacity and available again after a departure. The legacy local
state field guest_connected remains derived as N > 0; guests_connected is the count and
max_guests the bound. No remote guest-management command or secret is exposed to other guests.

All editing still requires the exact shared command policy and current revision. Racing drafts
are refused visibly, never silently merged; a retry with refreshed revision preserves the
participant's next sequence. Each guest can publish a finished take through the same checked
upload path; every connection receives verified new audio before its clip. Simultaneous uploads
can be refused as stale after another edit; the source take stays local and can be resent.
Local play/stop/position and scene/clip launches remain private.

Validation uses real TLS with host plus three guests: unique single-use invitations, full/expired/
replaced-token refusal, independent sequences, competing edits, audio from multiple authors,
exact model/assets convergence, membership-only updates, one guest leaving/rejoining, host stop,
UI controls and independent renderers. Physical household-computer tests stay deferred.
Internet routing, automatic resume, participant naming/roles and host migration remain planned.
