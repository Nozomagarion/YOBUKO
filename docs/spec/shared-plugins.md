---
description: Projects with plug-ins and built-in effects in shared sessions; settings travel with the copy, each computer runs its own plug-ins
read_when: plug-ins in a shared session, VST3 or CLAP in multiplayer, missing plug-ins on a guest, built-in effects in a shared session, device states in the portable copy
type: spec
status: draft
last_verified: 2026-10-30
covers:
  - crates/multiplayer/src/midi.rs
  - crates/multiplayer/src/transfer.rs
  - crates/session/src/shared_devices.rs
  - crates/ui/src/app/multiplayer_devices.rs
  - crates/ui/src/app/multiplayer_device_chain.rs
---

# Plug-ins in shared sessions

Self-authored fork behaviour, protocol 17. Until protocol 16 a shared project could hold only
FM Synth and Utility without state; any other device refused the whole share. Real projects use
effects and plug-ins, so the portable copy (the one **Share project** and online sessions send)
now carries them. The narrow MIDI-only copy keeps its old subset.

## What travels

- **Every device kind**: built-in instruments and effects, device graphs (`.sd`), device groups
  with their branches and macros, CLAP, VST3 and VST2 plug-ins, and placeholders for devices of
  other DAWs.
- **Their settings**: a device's state (a plug-in's saved settings, a built-in's or a graph's
  data) and its parameters and automation, unchanged. A state is at most 4 MiB, all states of a
  copy at most 32 MiB, a copy at most 64 MiB and 256 devices counting those inside groups.
- **Not** device extensions (data imported from other DAWs) or branch extensions, which stay on
  the host like the project's other private metadata.

Devices that point at files on the host's disk are refused, naming the device and its channel,
because the file would not travel: a Sampler with a sample, a NAM Amp Modeler or Cab Loader with
a model or impulse response, a device graph with a sample. The same devices without a file
travel. Sharing those files through the sound transfer is later work.

A plug-in never travels: each computer runs the plug-ins installed on it, in Suhina's separate
plug-in host process, with the settings from the copy. A guest only loads settings from the host
whose invitation it chose to accept.

## Missing plug-ins

When the guest's shared tab opens, Suhina runs the existing `plugin.check` on it. Devices for
which no installed plug-in is found are listed under **Plug-ins missing on this computer**, as
"name (format) on channel". They stay silent there and keep their settings, so nothing is lost
and edits on the rest of the project still apply. Installing the same plug-in and joining again
makes them play. Leaving the session clears the list.

## Edits

- `device.setSharedParam` now sets a parameter of any built-in device, whatever its state holds;
  the state itself is never changed. A plug-in's parameters are not edited remotely, and neither
  are graphs or groups.
- `device.addShared` still adds only FM Synth and Utility. A chain may now hold any device.
  FM Synth is refused when the chain holds anything but built-in effects and MIDI effects, since
  a plug-in, graph or group might already be the instrument.
- `device.removeShared` removes only FM Synth and Utility without state. The **Remove selected
  device** button is disabled for other devices.
- The desktop **Devices** page lists every built-in device of the shared tab.

## Transfer

The copy keeps its 32 KiB chunks. Its deadline is 30 s plus one second per 128 KiB, so a 64 MiB
copy over a slow relayed link has about nine minutes. The receiver bounds each base64 state
before the native reader runs.

## Tests

`midi_tests.rs` round-trips a VST3 with a 300 KB state, a Reverb with state and a group holding
a CLAP and a VST2, checks extensions are stripped, and refuses host files, oversized states and
an oversized total, plus a forged oversized state in a received copy. `tests/plugins.rs` shares a
project with a 3 MB plug-in state online and on the LAN, edits a Reverb knob from the guest and
refuses a plug-in parameter edit. Session tests cover parameters of every built-in, chains
holding plug-ins and the FM Synth instrument guard. A desktop test shows a guest its missing
plug-in.

Remote editing of plug-in settings (sending a new state when someone turns a plug-in knob) and
freezing a track to audio for a guest without the plug-in are later work.
