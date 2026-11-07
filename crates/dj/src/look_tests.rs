// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn the_look_is_written_once_and_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = path(dir.path());
    assert_eq!(read(&path), None);
    let look = Look { theme: "yobuko@washi".into(), dark: false, ..Look::default() };
    assert!(write(&path, &look).unwrap());
    assert!(!write(&path, &look).unwrap(), "the same look is not written again");
    assert_eq!(read(&path), Some(look));
}

#[test]
fn a_look_missing_colours_takes_ink_and_echos() {
    let look: Look = serde_json::from_str(r##"{"format":1,"accent":"#FF0000","ground":"nope"}"##).unwrap();
    let colours = look.colours();
    assert_eq!(colours.accent, Rgba { r: 255, g: 0, b: 0, a: 255 });
    assert_eq!(colours.ground, Rgba::hex("#1D1C1A").unwrap());
    assert_eq!(Rgba::hex("#abc").unwrap().to_hex(), "#AABBCC");
}
