// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::collections::BTreeSet;

#[test]
fn the_word_list_has_2048_distinct_plain_words() {
    let list = words().unwrap();
    assert_eq!(list.len(), WORD_COUNT);
    let distinct: BTreeSet<_> = list.iter().collect();
    assert_eq!(distinct.len(), WORD_COUNT, "no duplicates");
    for word in list {
        assert!((3..=9).contains(&word.len()), "{word}");
        assert!(word.bytes().all(|b| b.is_ascii_lowercase()), "{word}");
    }
    assert!(list.windows(2).all(|pair| pair[0] < pair[1]), "sorted, so lookups can bisect");
}

#[test]
fn codes_round_trip_through_text_whatever_the_case_spacing_or_accents() {
    for _ in 0..200 {
        let code = ShortCode::generate().unwrap();
        let text = code.text().unwrap();
        assert!(ShortCode::looks_like(&text));
        assert_eq!(ShortCode::parse(&text).unwrap(), code);
        assert_eq!(ShortCode::parse(&format!("  {}  ", text.to_uppercase())).unwrap(), code);
        assert_eq!(ShortCode::parse(&text.replace('-', " ")).unwrap(), code);
        assert_eq!(ShortCode::parse(&text.replace('-', " - ")).unwrap(), code);
        assert_eq!(ShortCode::parse(&text.replace('-', ".")).unwrap(), code);
    }
    let code = ShortCode::parse("7-piano-tigre-lune").unwrap();
    assert_eq!(code.text().unwrap(), "7-piano-tigre-lune");
    assert_eq!(ShortCode::parse("007 Piano TIGRE lune\n").unwrap(), code);
    assert_eq!(ShortCode::parse("7_piano,tigre,lune").unwrap(), code);
    let accented = ShortCode::parse("12-écureuil-château-crêpe").unwrap();
    assert_eq!(accented.text().unwrap(), "12-ecureuil-chateau-crepe");
    // A decomposed paste: e followed by a combining acute accent.
    assert_eq!(ShortCode::parse("12-e\u{301}cureuil-chateau-crepe").unwrap(), accented);
}

#[test]
fn malformed_codes_are_refused_with_a_reason() {
    for bad in [
        "",
        "piano-tigre-lune",
        "1000-piano-tigre-lune",
        "7-piano-tigre",
        "7-piano-tigre-lune-soleil",
        "7-piano-tigre-lunes",
        "7-piano-tigre-xyzzy",
        "7piano-tigre-lune",
        "7-piano-tigre-lune-8",
    ] {
        assert!(ShortCode::parse(bad).is_err(), "{bad:?} must be refused");
    }
    assert!(ShortCode::parse(&format!("7-piano-tigre-lune{}", " ".repeat(200))).is_err(), "too long");
    let error = ShortCode::parse("7-piano-tigre-xyzzy").unwrap_err();
    assert!(error.contains("xyzzy"), "{error}");
    assert!(!ShortCode::looks_like("suhina-net://abc"));
    assert!(!ShortCode::looks_like("suhina-lan://abc"));
    assert!(ShortCode::looks_like(" 7-piano"));
}

#[test]
fn number_zero_marks_a_code_for_this_computer_only() {
    let local = ShortCode::parse("0-piano-tigre-lune").unwrap();
    assert!(local.is_local());
    assert!(!ShortCode::parse("7-piano-tigre-lune").unwrap().is_local());
    let fresh = ShortCode::local().unwrap();
    assert!(fresh.is_local() && fresh.text().unwrap().starts_with("0-"));
    assert!((0..64).all(|_| !ShortCode::generate().unwrap().is_local()), "Internet codes start at 1");
}

#[test]
fn generated_codes_spread_over_the_whole_space() {
    let codes: Vec<_> = (0..64).map(|_| ShortCode::generate().unwrap()).collect();
    let distinct: BTreeSet<_> = codes.iter().map(|c| c.text().unwrap()).collect();
    assert_eq!(distinct.len(), codes.len());
    assert!(codes.iter().all(|c| (1..=MAX_NUMBER).contains(&c.number)));
    assert!(codes.iter().flat_map(|c| c.words).all(|w| usize::from(w) < WORD_COUNT));
}

#[test]
fn the_same_code_always_gives_the_same_keys_and_another_code_others() {
    let code = ShortCode::parse("7-piano-tigre-lune").unwrap();
    let (first, again) = (code.keys().unwrap(), code.keys().unwrap());
    assert_eq!(first.key.public(), again.key.public());
    assert_eq!(first.proof, again.proof);
    assert_ne!(first.key.to_bytes(), first.proof, "key and proof come from distinct contexts");
    let other = ShortCode::parse("8-piano-tigre-lune").unwrap().keys().unwrap();
    assert_ne!(first.key.public(), other.key.public());
    assert_ne!(first.proof, other.proof);
    assert!(!format!("{code:?} {first:?}").contains("piano"), "debug output stays redacted");
}
