---
description: The host inputs passed when initializing an installed ASIO driver on Windows
read_when: ASIO, UAD, audio driver initialization, ASIOInit, sysRef, asio-sys
type: spec
status: current
last_verified: 2026-10-28
covers:
  - third_party/asio-sys/
  - third_party/cpal/
  - Cargo.toml
  - crates/output/
---

# ASIO host initialization

An ASIO-enabled Windows build lists installed drivers through cpal. Loading a
driver calls ASIOInit through asio-sys. The host must initialize the entire
ASIODriverInfo before that call: asioVersion is 2, driver metadata buffers start
empty, and sysRef is a valid desktop window handle. The handle is borrowed and
stays valid across audio-owner threads; it is not an audio buffer or an owned
window. The same contract applies when reloading a driver after a rate change.

The bundled SDK reads sysRef and passes it to the driver's init method. Leaving
that field uninitialized is invalid even when an individual driver ignores it.
The crates.io asio-sys 0.4.0 wrapper leaves it uninitialized at both call sites;
a local Apache-2.0 Cargo patch supplies the initialized structure and tests its
inputs without requiring hardware. Application crates still forbid unsafe code.

Before querying a channel's format, the wrapper queries ASIOGetChannels for the
current driver instance and validates the requested index. Enumeration does not
establish this precondition for a later reload: cpal releases the enumerated
driver and opens it again when creating a stream. Hardware reproduction with
Universal Audio Thunderbolt showed ASIOGetChannelInfo returning ASE_NotPresent
after that reload until ASIOGetChannels had been called. The hardware regression
test queries the output format immediately after each of three fresh loads.

Hardware verification must distinguish the selected device from the actual
output: audio.info must report host ASIO, the selected device, and fallback false.
Successful playback on a WASAPI fallback does not establish ASIO support. Tests
of project-specific failures use that project, its plug-ins and the same driver.

## Native control panel and buffer readouts

`audio.openControlPanel` requests the active ASIO stream's native panel through
the stream-owner queue. `ASIOControlPanel` runs on that owner thread, never in an
audio callback or by loading a second driver. Repeated requests are refused while
the panel is pending/open. Non-ASIO outputs and absent outputs return an error.
The dialog can remain modal on the owner until the user closes it; close the panel
before changing outputs or closing the project. Opening it does not reload plugins.

Audio > Now offers **ASIO Control Panel…** for the active ASIO output and reports
**Active buffers** as separate engine-block and observed callback frame counts.
The latter is not a claim about every internal hardware/WDM buffer. The driver
owns its internal settings. External offline panels may edit defaults rather than
the running application's settings; the native call addresses the current instance.

## Device catalog during playback

CPAL 0.18.2's ASIO device descriptors cache metadata without retaining a driver.
Suhina collects those descriptors before opening the first ASIO stream, and reuses
them for both settings and selection during this app run. ASIO enumeration loads
each driver in turn; while a different driver is already loaded, CPAL ends that
iterator. Enumerating during playback would therefore hide installed devices and
needlessly call drivers from the settings window. Newly installed drivers become
available after restarting Suhina. Opening the chosen descriptor still reports
an error if its hardware is unavailable.

## Switching from the settings form

Driver selection is pending until the user selects Device. One `audio.setOutput`
command then names both; no intermediate default driver opens. The active output
continues while the choice is pending. The rule applies to every host and device,
without matching any vendor name.

## Initial callback burst

The ASIO SDK 2.3 specification, section 5, describes callbacks that fill the first buffers
before hardware streaming starts; its example gives the first two callbacks the same time.
Suhina therefore primes enough silence for two known device periods before starting the
stream. This is a startup reserve only; the ordinary render queue target stays unchanged.

## Repeated buffer indices

A buffer index identifies the memory to process, not a unique processing cycle.
Some installed drivers use buffer 0 for every cycle. CPAL 0.18.2's index-only
repeat guard drops all but the first callback from these drivers. A silent
three-second reproduction with FL Studio ASIO delivered one callback before
including the driver timestamp in the comparison, and 63 afterwards.

Compare both index and timestamp to suppress duplicate notifications from the
same cycle while allowing a new timestamp for the same buffer. Apply the same
rule to input and output, without driver-name checks. This does not change which
ASIO buffer is read or written. Drivers with missing or repeated timestamps need
separate validation; the test does not establish their compatibility.

## Sources

- The vendored SDK, `third_party/asiosdk/common/asio.cpp`, ASIOInit.
- [asio-sys 0.4.0 source](https://github.com/RustAudio/cpal/tree/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/asio-sys).
- [PortAudio ASIO host](https://github.com/PortAudio/portaudio/blob/master/src/hostapi/asio/pa_asio.cpp), LoadAsioDriver and the desktop window used by its Windows host.
- [Ardour's PortAudio integration](https://github.com/Ardour/ardour/blob/master/libs/backends/portaudio/portaudio_io.cc), which opens its ASIO stream through PortAudio.

- [CPAL 0.18.2 ASIO device enumeration](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/asio/device.rs).

- [Steinberg ASIO SDK 2.3 specification, section 5](https://dspace.tul.cz/bitstream/handle/15240/49466/ASIO_SDK_2.3.pdf?sequence=1).
- [CPAL single-buffer compatibility report](https://github.com/RustAudio/cpal/pull/1077).
