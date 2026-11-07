# rbxport app (vendored)

The whole [rbxport](https://github.com/chrisle/rbxport) desktop app by Chris Le — a
Rekordbox-style DJ library manager with track analysis, decks, USB export for CDJ/XDJ
players and PRO DJ LINK — at revision `b739d369665b492534dc2eae0733322c040a26d5`
(rbxport 1.9.0), under **GPL-2.0-or-later** (see `LICENSE` and
`crates/rbl-deck/vendor/rubberband/COPYING`), used under GPLv3 with YOBUKO.

It is the **USB stick** side of YOBUKO's DJ mode (which mixes live in Mixxx):
`scripts/rbxport_build.sh` builds it and ships it inside the YOBUKO packages, and the DJ
page's "Prepare a USB stick (rbxport)" opens it (`crates/ui/src/app/dj_rbxport.rs`).

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
- `src-tauri/src/update.rs`: the update check always answers "up to date", so this copy is
  never replaced by upstream's build; YOBUKO's releases update it.
- `src-tauri/tauri.conf.json`: identifier `com.yobuko.rbxport` (its settings stay apart
  from a separately installed rbxport) and no updater artifacts (they need upstream's
  signing key).

## Updating

Copy a newer rbxport over this folder (keeping the exclusions above), then put the
`YOBUKO patch` changes back; `git diff` against the "Vendor the rbxport app" commit shows
them.
