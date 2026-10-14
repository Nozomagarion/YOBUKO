---
description: How played input reaches the engine (MIDI controllers first, audio inputs later), what arming a track is, and what a key press costs in latency today and how that is made smaller
read_when: MIDI input, MIDI controller, keyboard, arm, armed track, track.setArm, midi.inputs, live notes, sustain pedal, input latency, monitoring, recording, low latency, midir
type: decision
status: current
last_verified: 2026-10-14
covers:
  - crates/engine/src/live.rs
  - crates/engine/src/live_tests.rs
  - crates/session/src/midi.rs
  - crates/midi/src/lib.rs
  - crates/output/src/lib.rs
---

# 0017: Inputs, arming and live latency

- **Status:** accepted (MIDI input, and the shallower queue while armed, #265); audio input, monitoring and recording are planned in [#263](https://github.com/suhina-daw/suhina-daw/issues/263)
- **Date:** 2026-10-14

## Context

Until now Suhina had no inputs: it could play a project, but nobody could play an instrument
into it, monitor a guitar through its effects, or record. That is the first tier's third item in
[ADR 0016](0016-parity-target.md), and the first place the engine answers to a device rather than
to its own clock.

Played input is where latency is felt. A clip launch waits for its quantisation anyway; a key
pressed on a keyboard must sound within a few milliseconds, or the instrument feels late. The
output was built for playback: the render thread runs `Renderer::process` ahead of the device
into a lock-free ring and keeps two blocks queued (`docs/architecture.md` › Threads), so a frame
rendered now is heard about *two blocks plus the device buffer* later. Plug-ins add a round trip
per block, within a deadline ([ADR 0006](0006-plug-in-hosting.md)).

AGENTS.md rule 3 rules out allocating, locking or waiting on the audio thread, rule 6 asks that
arming and choosing inputs be commands, and rule 4 that offline renders keep testing the same
engine code.

## Decision

1. **MIDI first, through `midir`.** A new L3 crate, `crates/midi` (`suhina-midi`), lists the
   system's MIDI input ports and opens them with `midir` (MIT; the ALSA sequencer on Linux, WinMM
   on Windows, CoreMIDI on macOS). It is the only code that opens a MIDI device; the session sees
   it as a `MidiSystem` (`crates/session/src/midi.rs`), as it sees the audio output as an
   `AudioSystem`. It reads note on, note off (a note on of velocity 0 too), the sustain pedal
   (controller 64) and all-notes-off (controllers 120 and 123), on every channel; everything
   else is ignored, short or malformed messages included.
2. **Live notes reach the engine over their own queue.** The renderer has a live-note queue
   (`rtrb`, 1024 events) beside the controller's command queue (`crates/engine/src/live.rs`).
   Its sending end goes to the MIDI driver's callback thread, which may lock and allocate: it is
   not the audio thread. The session's `LiveInput` holds the sending end of the engine that plays
   now; `Projects::poll_plugins` (every frame in the window, around every command over MCP) hands
   it the queue of a new player (another tab, a new output). The render thread takes the queue's
   events at the start of each block, after the controller's commands, and plays them at the
   block's first frame through `Chain::note_on` and `Chain::release_key`, the way the preview
   voice plays its notes ([ADR 0011](0011-preview-and-added-audio.md)). Nothing on the render
   thread allocates, locks or frees.
3. **Arming is playing state, not an edit.** An armed track's instrument plays the live notes,
   whether the transport runs or not. Which tracks are armed is kept by the `Session` beside the
   project, not in it: `track.setArm` is a transport command, is not undone and is not saved
   (a project opens with nothing armed). It reaches the engine as `LaunchCommand::SetArmed`,
   without a new graph, and the renderer keeps the armed tracks (up to 64) across graph swaps; a
   new player is told the armed tracks when it is attached. Arming is exclusive when asked
   (`exclusive: true`, what a plain click in the window does; Ctrl adds a track), like solo.
   Disarming a track lets go of the keys it holds. A track armed while a key is down hears the
   next key, not that one.
4. **Every port is on until turned off.** `midi.inputs` lists the ports, `midi.setInput` turns
   one on or off; the window keeps the ports turned off in its settings. The ports are looked at
   again every two seconds (every thirty when they cannot be listed at all), so a controller
   plugged in is heard without a restart, and one taken out is closed. Closing a port lets go of
   its keys.
5. **What a key press costs**, from the driver's callback to the speaker: up to one block until
   the render thread starts its next one, what is queued ahead in the ring, and the device's
   buffer and its own delay. A player sets the buffer small (64 to 256 frames, with ASIO or
   WASAPI exclusive mode on Windows); the default 1024 frames is too slow to play.
6. **While a track is armed, the output keeps one block queued instead of two** (amendment,
   [#265](https://github.com/suhina-daw/suhina-daw/issues/265)). The render thread reads
   `Renderer::live_armed` before each block and aims for one block or one device buffer,
   whichever is more, instead of two blocks or a device buffer and a block. It is automatic, not
   a setting: an armed track is the one sign that someone may be playing, and playback alone
   keeps the safer depth. Arming lets the device drain the queue to the new depth and disarming
   lets the thread fill it again, so neither is heard. The cost is time to render: the thread
   has one device buffer's time to fill the next block instead of two, so a heavy project or a
   slow plug-in shows as underruns (Settings › Audio, `audio.info`), and the answer is a larger
   buffer. At 48 kHz with 128-frame blocks a key now waits at most about 5.3 ms plus the device
   (8 ms before); at 256 frames, 10.7 ms (16 ms).
7. **The latency is reported.** `audio.info` gives `live` (a track is armed) and
   `live_latency_ms`, the longest a key takes to be heard: the measured engine-to-speaker latency
   and one block. Settings › Audio shows it as *Key to sound*. The engine's own part is tested
   offline: a live note sounds from the first frames of the next block, at any block size; the
   output's queue depth is tested on the real render thread.

## Consequences

- A MIDI keyboard plays any instrument, built in or a plug-in, on armed tracks, with the sustain
  pedal; the latency follows the buffer size chosen.
- Live notes land on the block's first frame: they jitter by up to one block. Placing them at a
  constant delay from the driver's timestamps removes the jitter
  ([#269](https://github.com/suhina-daw/suhina-daw/issues/269)).
- A clip note and a live note on the same key of the same track share that key: the later one
  restarts it, and either's release ends it.
- Arming is lost when the project closes. Saving it with the project can be added later as a
  project field without changing the engine side.
- The last queued block is the one left between the engine and the device; rendering inside the
  device's callback would remove it, but would put plug-in round trips and graph swaps on the
  driver's thread, so it is not done. Lower latency from here comes from real-time priority
  ([#150](https://github.com/suhina-daw/suhina-daw/issues/150)) and fewer plug-in round trips
  ([#108](https://github.com/suhina-daw/suhina-daw/issues/108)), which make small buffers safe.

## Planned

The rest of [#263](https://github.com/suhina-daw/suhina-daw/issues/263) follows this ADR and
amends it as it lands: MIDI recording with count-in (#266), audio inputs and monitoring In / Auto /
Off (#267), and audio recording lined up for input and output latency (#268). Monitored audio
input takes the same shallower queue while its track is armed.
