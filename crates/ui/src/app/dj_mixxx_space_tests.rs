// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // test helpers
use super::*;

#[test]
fn mixxx_is_wanted_until_it_cannot_show_or_the_dj_picks_yobukos_decks() {
    let mut space = MixxxSpace::default();
    assert!(space.wanted(), "Mixxx first");
    space.host.status = MixxxStatus::Stopped("it closed".into());
    assert!(space.wanted(), "a stopped Mixxx offers to start again");
    space.host.status = MixxxStatus::Unsupported("macOS".into());
    assert!(!space.wanted());
    space.host.status = MixxxStatus::Missing;
    assert!(!space.wanted());
    space.host.status = MixxxStatus::Ready;
    space.own_decks = true;
    assert!(!space.wanted());
    let none_found = MixxxSpace { found: Some(false), ..MixxxSpace::default() };
    assert!(!none_found.wanted(), "no Mixxx on this computer: YOBUKO's decks");
}

#[test]
fn mixxx_starts_with_yobukos_profile_and_never_full_screen() {
    let profile = std::path::Path::new("/data/YOBUKO/mixxx");
    let args = super::mixxx::arguments(Some(profile));
    assert_eq!(args, [OsString::from("--settings-path"), profile.as_os_str().to_owned()]);
    assert!(super::mixxx::arguments(None).is_empty());
}
