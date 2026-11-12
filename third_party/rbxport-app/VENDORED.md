# rbxport app (vendored)

The whole [rbxport](https://github.com/chrisle/rbxport) desktop app by Chris Le — a
Rekordbox-style DJ library manager with track analysis, decks, USB export for CDJ/XDJ
players and PRO DJ LINK — at revision `d0ff75b378ec85be891d9f3067ada1e82bad4cae`
(rbxport 1.10.0), under **GPL-2.0-or-later** (see `LICENSE` and
`crates/rbl-deck/vendor/rubberband/COPYING`), used under GPLv3 with YOBUKO.

It is the **USB stick** side of YOBUKO's DJ mode: `scripts/rbxport_build.sh` builds it and
ships it inside the YOBUKO packages, and YOBUKO's USB space runs it hidden and shows its
interface **inside YOBUKO's own window** (`apps/suhina/src/rbxport_panel.rs` and
`crates/ui/src/app/usb_space.rs` in YOBUKO); it never shows a window of its own there.

It is its own Cargo and pnpm workspace, excluded from YOBUKO's (`Cargo.toml` › `exclude`).
Left out of the copy: `.git`, `.github` and `docs/assets` (brand artwork).

## Changes from upstream (each marked `YOBUKO patch` in the code)

- `src-tauri/src/yobuko.rs` (new): reads the inbox folder
  `<data dir>/YOBUKO/rbxport-inbox`, where YOBUKO drops `.paths` requests for tracks (ones
  it rendered, and Mixxx's) and `.playlist` requests for Mixxx's playlists and crates
  (filed in a root folder "Mixxx", a same-named playlist replaced); and publishes the
  library (tracks, tags, hot cues, playlists) as `<data dir>/YOBUKO/library/rbxport.json`
  for YOBUKO to bring into Mixxx (`crates/dj/src/mixxx.rs` in YOBUKO). Started from
  `lib.rs`' `setup`; `commands::import_files_then` is `pub(crate)` for it.
- YOBUKO's look: `yobuko.rs` also hands the window the colours of the theme YOBUKO's studio
  uses (`<data dir>/YOBUKO/look.json`, command `yobuko_look`, registered in `lib.rs`);
  `src/lib/yobukoLook.ts` (new, started from `src/main.tsx` before the first paint) moves the
  interface's grey tokens onto them and its blue highlights onto the theme's accent, and
  `src/styles/base.css` draws DJ mode's pink line along the top of the window.
- Drawn inside YOBUKO's window: `src-tauri/src/yobuko_embed.rs` (new). Started by YOBUKO
  with `YOBUKO_EMBED=<port>:<token>`, every window (main, Preferences, Sync Manager, Report
  bug, Log) is still built, hidden and out of the taskbar (an accessory app on macOS), and
  loads `public/yobuko-bridge.html` + `.js` (new) instead of the interface; YOBUKO shows the
  interface in a webview of its own window and its commands, events, callbacks, the
  interface's files, the `rbl:` pictures and sounds and the menu bar travel over a local TCP
  connection. `lib.rs` (main window page, setup, four commands, window events),
  `startup.rs` (never reveals a hidden window), `preferences.rs`, `sync_window.rs`,
  `report.rs`, `log_window.rs` (page, show, focus and eval through `yobuko_embed`) and
  `yobuko.rs` (no bringing forward) carry one-line hooks. Without the variable nothing
  changes.
- YOBUKO's soft shapes: `src/styles/yobuko-soft.css` (new, imported in `src/main.tsx`)
  rounds buttons, fields, menus and dialogs like YOBUKO's DJ space once YOBUKO's look is on.
- `src-tauri/src/update.rs`: the update check always answers "up to date", so this copy is
  never replaced by upstream's build; YOBUKO's releases update it.
- `src-tauri/tauri.conf.json`: identifier `com.yobuko.rbxport` (its settings stay apart
  from a separately installed rbxport) and no updater artifacts (they need upstream's
  signing key).

## Updating

Copy a newer rbxport over this folder (keeping the exclusions above), then put the
`YOBUKO patch` changes back; `git diff` against the "Vendor the rbxport app" commit shows
them.
