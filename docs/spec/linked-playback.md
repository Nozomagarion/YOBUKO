---
description: Linked playback in multiplayer sessions, one host-ordered transport every computer starts at the same moment
read_when: link playback, play together, shared transport, clock offset, start everyone at once, protocol 14
type: spec
status: draft
last_verified: 2026-10-29
covers:
  - crates/multiplayer/src/linked.rs
  - crates/multiplayer/src/transport.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
---

# Linked playback

Self-authored extension of the host-ordered session adapter ([online sessions](online-sessions.md),
[several guests](lan-multiple-guests.md)). No audio travels between computers: each one still
renders the project on its own player. Linking makes them start, stop and move at the same moment.
Protocol 14.

## What a person does

Any participant ticks **Link playback** in the multiplayer panel. While it is ticked, the panel's
buttons become **Play together**, **Stop together** and **Everyone from beat 16**, and act on
every computer in the session. Unticking it (anyone can) brings back **Play locally**, **Stop
locally** and **Listen from beat 16**, which behave exactly as before. The Studio's own transport
bar stays local.

## Shared transport

The host keeps `{seq, playing, beat, start_ms}` beside its canonical project. `start_ms` is on the
host's monotonic clock, in milliseconds since the session started. Every change raises `seq`, which
keeps rising across unlinking and relinking. Requests are ordered like edits: the host's own come
through its queue, a guest's through `ClientMessage::Transport`.

- **link** on stops everyone at the shared beat (initially 0), so all start from the same place;
  link off clears the shared transport from every state. Linking twice changes nothing.
- **play** at a beat schedules the start 300 ms ahead (`start_ms = host_now + 300`), so the state
  reaches every guest, polling every 100 ms, before it is due.
- **stop** applies at once and keeps the beat play started from; each computer's own stop then
  returns its playhead there, so a later play starts from the same place everywhere.
- **seek** sets the beat; while playing it reschedules the start 300 ms ahead.

Play, stop and seek are refused while unlinked. Beats must be finite and within 0 … 120000.
Nothing new is rate limited: the requests ride on the existing exchange cadence.

## Wire

`ServerMessage::State` gains `host_ms`, the host clock when the state was made, and `transport`,
the shared transport or null while unlinked. `ClientMessage::Transport {protocol, seen_revision,
request}` carries `{"action":"link","linked":bool}`, `{"action":"play","beat":n}`,
`{"action":"stop"}` or `{"action":"seek","beat":n}`; the reply's `accepted` and `error` report a
refusal. Unknown fields stay refused everywhere. Protocol 14 and ALPN `suhina/multiplayer/14`
refuse earlier applications.

## Clock estimate

Each guest times every poll or request exchange on its own monotonic clock. With `sent` and
`received` around the host's `host_ms`, one sample gives `offset = host_ms − (sent + rtt/2)`.
The guest keeps the last 64 samples (about six seconds) and uses the offset of the one with the
shortest round trip, which bounds the error by half that round trip. Exchanges slower than 2 s
and sound uploads are not sampled. The host's own offset is zero.

## Commands and state

- `multiplayer.linkPlayback {linked: bool}`
- `multiplayer.transport {action: "play" | "stop" | "seek", beats: number}` (stop ignores beats)

Both use the other multiplayer commands' local policy (agents denied) and exact keys.
`multiplayer.state` adds `linked`, `transport` and `transport_starts_in_ms`: the local delay to
the scheduled start, negative once it has passed, null unless playing. The view needs no clock
arithmetic of its own.

## Following on each computer

When a new `seq` arrives the app applies it to the shared tab's own player, once that tab is in
front. A stop runs `transport.stop` then `transport.setPosition` at the shared beat. A start waits
for its moment (frames come about every 100 ms; a closer start asks for a frame then), then runs
`transport.setPosition` at `beat + elapsed × tempo / 60` and `transport.play`. Computing the beat
from the time already elapsed makes a late frame, or a guest who joins mid-play, land where the
others are.

## Limits

- Alignment is within the clock estimate (half the best recent round trip) plus each computer's
  audio output latency, which is not compensated.
- The catch-up beat uses the project tempo; tempo changes in the arrangement's tempo map are not
  followed by the catch-up.
- Launched clips and scenes stay local. The Studio transport bar does not link.

## Tests

`crates/multiplayer/src/linked_tests.rs` covers the clock estimate, the schedule and catch-up
arithmetic, the host's ordering and strict parsing. `crates/multiplayer/tests/online.rs` links
playback over `online-loopback`, has the guest start it and checks both report the same transport
and start times within 30 ms, then stops and unlinks. `crates/ui/src/app/multiplayer_linked_tests.rs`
ticks the visible toggle and checks that Play, the move and Stop go through the shared transport and
are applied on host and guest.
