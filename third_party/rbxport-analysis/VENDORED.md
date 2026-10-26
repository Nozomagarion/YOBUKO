# rbxport analysis

Source: https://github.com/chrisle/rbxport/tree/b739d369665b492534dc2eae0733322c040a26d5/crates/rbl-analysis

Copyright (c) 2026 Chris Le. GPL-2.0-or-later; see LICENSE and COPYING.
The src/ tree is copied byte for byte. Only Cargo.toml is adapted to build without
Tauri, the original workspace or private-library development dependencies.
Suhina uses this for offline analysis, never in its audio callback. USB exports,
Rekordbox database access and the rbxport interface are not integrated yet.
Refresh through scripts/fork_sources.py and validate the integration before shipping.
