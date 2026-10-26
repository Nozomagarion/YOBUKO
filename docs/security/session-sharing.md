---
description: Security boundary of trusted collaboration and the restricted certificate-bound LAN starter adapter
read_when: multiplayer threat model, invitation security, peer authorization, session network boundary
type: system
status: draft
last_verified: 2026-10-25
covers:
  - crates/collaboration
  - crates/multiplayer
  - crates/session/src/projects.rs
  - crates/ui/src/app/multiplayer.rs
---

# Session sharing threat model

## Trusted core and remote boundary

`collaboration` remains a trusted in-process API. Its full Project clones and opaque
acknowledgements are not wire formats. Numeric PeerIds are local bookkeeping, never credentials.
The separate `multiplayer` adapter exposes only the [LAN starter contract](../spec/lan-starter-session.md).
No arbitrary project, asset, device, plugin or microphone state crosses that boundary.

The host retains its canonical project and accepts only track rename, volume and pan on four
fixed tracks. Membership, role, exact parameter keys, IDs, numeric ranges and sequence/revision
checks precede execution. The adapter allocates the guest's PeerId after admission. A guest
cannot claim the owner's identity or change roles. Transport, history, files, commands/batches,
settings and library/plugin operations have no remote representation.

## Admission and confidentiality

Only an explicit Create LAN session action starts a listener, on one private/loopback/link-local
IPv4 address. There is no discovery, relay, DNS target, public binding or internet service.
Joining trusts the certificate embedded in the private invitation with normal rustls TLS 1.3
verification for suhina.local. A 256-bit OS-random secret is proven inside TLS, compared by
constant-time hash equality, expires after five minutes and is redeemed once. Stopping revokes
all admission. Disconnect cannot resume a used invitation. One host and one guest are supported.

The invitation is a bearer capability: a person with its complete contents can join during its
lifetime. Send it privately. Certificate mismatch, malformed data and failed admission do not
consume a legitimate invitation. Debug output is redacted; neither the invitation nor its secret
is written to application logs or project files. The user's explicit clipboard copy is intentional.
An attacker controlling that clipboard or either computer is outside this boundary.

## Resource and mutation limits

Length-prefixed typed JSON is capped at 8192 bytes before body allocation, with exact fields,
fixed track count and bounded names/ranges. Read/write/handshake exchanges have absolute three
second deadlines. The host has at most four pending connection handlers; the normal authenticated
guest polls every 100 ms and the server enforces at least 50 ms per request. UI queues are bounded.
TLS, canonical execution and I/O run on workers, never the audio thread. A local peer can still
occupy pending slots until timeout; availability against a sustained LAN flood is not guaranteed.

The desktop creates a dedicated deterministic MIDI project. A Projects mutation gate prevents
normal UI and attached-agent commands from changing its shared model/history outside confirmed
controls. Its host-owned API admits only the three control command IDs; it is not a registered
agent command. Other project tabs stay local. Local audition and explicit save/export remain
available. Disconnect retains and unlocks the last local project; a failed authoritative apply
leaves the session rather than silently continuing with divergent state.

## Evidence and remaining boundaries

Automated tests cover real TLS loopback exchange, wrong certificate/secret/version, one-use and
expiry rules, replayed edits, stale revisions, malformed/duplicate fields, an incomplete handshake
timeout, stopped connections, desktop application of controls, mutation refusal and independent
real audio renderers. Physical two-computer/firewall and cross-platform session acceptance remain
unverified. This prototype has not had an independent security audit.

Sharing arbitrary projects/media remains closed until portable asset manifests, path/symlink
confinement, size/hash/disk quotas, atomic finalization and interruption recovery are designed and
tested, aligned with project folder work (#606). Wider edits require identity allocation and
side-effect review. Recording publication, linked transport, presence, reconnect/resume, host
migration and internet routing need separate contracts. The restricted starter is not evidence
that these wider boundaries are safe or implemented.
