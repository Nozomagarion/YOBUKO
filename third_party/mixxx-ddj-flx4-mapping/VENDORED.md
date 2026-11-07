# Pioneer DDJ-FLX4 mapping for Mixxx (vendored)

- Source: https://github.com/ElHanko/mixxx-ddj-flx4-mapping
- Commit: 231e902216498d69c9de59ed69f7e8196cd02dd1 (2026-10-14)
- Licence: MIT (see `LICENSE`), by ElHanko, building on Robert904's DDJ-FLX4 mapping that ships with Mixxx.
- Copied unchanged: `controllers/` and `effects/chains/`.

YOBUKO installs these files into the Mixxx profile it starts Mixxx with
(`crates/dj/src/mixxx.rs`, `install_community_mapping`) when that Mixxx is 2.6 or newer,
and selects the **Extended** preset (stems, Pad FX, Key Shift), which it writes from the
Basic one the way the project's README describes. With Mixxx 2.5, YOBUKO selects the
DDJ-FLX4 mapping that comes with Mixxx instead.

To update: copy the two folders again from a newer commit and change the commit above.
