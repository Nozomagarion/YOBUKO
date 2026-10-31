# rbxport USB exporter (vendored)

These crates come from Chris Le's [rbxport](https://github.com/chrisle/rbxport),
`crates/<name>/src` at revision `b739d369665b492534dc2eae0733322c040a26d5`
(rbxport 1.9.0, the same revision as `third_party/rbxport-analysis`), under
**GPL-2.0-or-later** (see `LICENSE`), used under GPLv3 with Suhina.

| Crate | What it does |
|---|---|
| `rbl-export` | Orchestrates a USB export: audio layout under `Contents/`, analysis under `PIONEER/USBANLZ/`, databases, sync of an earlier export, verification. |
| `rbl-pdb` | Writes `PIONEER/rekordbox/export.pdb` (DeviceSQL), what every CDJ/XDJ reads. |
| `rbl-onelibrary` | Writes `exportLibrary.db` (Device Library Plus, SQLCipher), what newer players read. |
| `rbl-anlz` | Writes `ANLZ0000.DAT/.EXT/.2EX`: beat grid, waveforms, cues. |
| `rbl-audio` | Probes and converts audio a player cannot read (to 16-bit WAV here). |
| `rbl-core` | Shared types and durable file writes. |

YOBUKO calls it from `crates/dj/src/usb.rs` (command `dj.usb.export`, button
*Export to USB stick (Rekordbox)…* in the DJ workspace).

## Changes from upstream

- Each `Cargo.toml` inlines the values rbxport's workspace gave it (version, edition,
  licence, dependency versions) and drops `[dev-dependencies]`; rbxport's tests and
  fixtures are not copied.
- `rbl-audio`: MP3 is encoded with Suhina's vendored LAME 3.100 (`crates/mp3`, ADR 0029)
  instead of the `mp3lame-encoder` crate, so one LAME is linked into the app, not two.
  Samples are clamped to full scale before encoding, as that encoder requires.
- `rbl-audio`: symphonia's `ogg` and `vorbis` features are on, so Ogg tracks in the DJ
  library can be converted for the player.

No other source line is changed. To update: copy `crates/<name>/src` from a newer rbxport
revision, keep the two `rbl-audio` changes above, and run `cargo test -p suhina-dj`.
