# CPAL 0.18.2

Source: crates.io cpal 0.18.2, upstream commit
e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7 in https://github.com/RustAudio/cpal.
License: Apache-2.0, preserved in LICENSE.

Local change: ASIO input and output reject only a repeated pair of buffer index
and driver timestamp, rather than every repeated index. Single-buffer drivers
otherwise deliver one callback and then stall. The helper's unit tests cover
single-buffer cycles, duplicate notifications, alternating buffers and clock reset.

Background: https://github.com/RustAudio/cpal/pull/1077 documented single-buffer
drivers; the later index-only duplicate guard reintroduced this incompatibility.
Silent hardware probe with the installed driver: one callback in three seconds
before the fix, 63 after it. See docs/spec/asio-host.md in the parent repository.

No driver-name checks or changes to the buffer addresses. Other backends are unchanged.

The ASIO Stream also exposes `control_panel()` on its retained driver so the
application's stream owner can open the active instance's settings without loading
another driver or launching an external executable.
