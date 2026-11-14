// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn washi() -> Colours {
    Look {
        dark: false,
        ground: "#F4EFE6".into(),
        surface: "#E9E2D5".into(),
        control: "#DCD4C6".into(),
        control_active: "#CFC6B6".into(),
        text: "#2A2723".into(),
        text_dim: "#5A554D".into(),
        hint: "#8A847A".into(),
        accent: "#3F7FBF".into(),
        ..Look::default()
    }
    .colours()
}

#[test]
fn greys_follow_the_looks_ramp_and_colours_become_its_accent() {
    let ink = Look::default().colours();
    assert!(map(Rgba::hex("#000").unwrap(), &ink).lightness() < ink.ground.lightness());
    assert_eq!(map(Rgba::hex("#fff").unwrap(), &ink), ink.text);
    assert_eq!(map(Rgba::hex("#257b82").unwrap(), &ink), ink.accent.mix(ink.ground, 0.25));
    assert_eq!(map(Rgba::hex("#ff2020").unwrap(), &ink), ink.recording);
    // A light look turns the dark skin light: its darkest grey is lighter than its text.
    let light = washi();
    assert!(
        map(Rgba::hex("#0c0c0c").unwrap(), &light).lightness() > map(Rgba::hex("#ccc").unwrap(), &light).lightness()
    );
}

#[test]
fn every_way_of_writing_a_colour_is_recoloured_and_nothing_else() {
    let ink = Look::default().colours();
    let qss = "a { color: #fff; background: rgba(0, 0, 0, 50); border: 1px solid #FF2020; }\n\
               b { qproperty-x: #dd151515; image: url(skins:YOBUKO/a.svg); } /* #abcdefg url(#abc) */ é";
    let out = recolour(qss, &ink, false);
    let text = ink.text;
    assert!(out.contains(&format!("color: #{:02x}{:02x}{:02x};", text.r, text.g, text.b)), "{out}");
    assert!(out.contains("rgba(") && out.contains(", 50);"), "{out}");
    assert!(out.contains("#e5484d"), "{out}");
    assert!(out.contains("qproperty-x: #dd"), "Qt's alpha stays first: {out}");
    assert!(out.contains("#abcdefg url(#abc)"), "not colours: {out}");
    assert!(out.ends_with('é'));
    let svg = recolour(r##"<path fill="#ffffff80"/>"##, &ink, true);
    assert!(svg.ends_with(r#"80"/>"#), "SVG's alpha stays last: {svg}");
}

#[test]
fn links_point_at_the_copy_and_style_sheet_urls_are_quoted() {
    let base = "/Users/a b/Library/Application Support/YOBUKO/Mixxx/skins/YOBUKO/";
    assert_eq!(
        relink("image: url(skins:LateNight/x/y.svg) no-repeat; <Pixmap>skins:LateNight/k.svg</Pixmap>", base),
        format!("image: url(\"{base}x/y.svg\") no-repeat; <Pixmap>{base}k.svg</Pixmap>")
    );
}

#[test]
fn the_skin_is_made_once_and_selected_only_where_it_is_ours_to_choose() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("LateNight");
    std::fs::create_dir_all(source.join("palemoon")).unwrap();
    std::fs::write(source.join("skin.xml"), "<skin><title>LateNight</title><x src=\"skins:LateNight/a.qss\"/></skin>")
        .unwrap();
    std::fs::write(source.join("palemoon").join("a.svg"), r##"<svg fill="#000"/>"##).unwrap();
    std::fs::write(source.join("palemoon").join("a.png"), [1u8, 2, 3]).unwrap();
    let profile = dir.path().join("profile");
    let look = Look::default();
    assert!(install(&source, &profile, &look).unwrap());
    assert!(!install(&source, &profile, &look).unwrap(), "unchanged: not made again");
    let made = profile.join("skins").join(SKIN);
    let xml = std::fs::read_to_string(made.join("skin.xml")).unwrap();
    let base = made.to_string_lossy().replace('\\', "/");
    assert!(xml.contains("<title>YOBUKO</title>") && xml.contains(&format!("{base}/a.qss")), "{xml}");
    assert_eq!(std::fs::read(made.join("palemoon").join("a.png")).unwrap(), [1, 2, 3]);
    assert!(!std::fs::read_to_string(made.join("palemoon").join("a.svg")).unwrap().contains("#000\""));
    assert!(install(&source, &profile, &Look { accent: "#00FF00".into(), ..look }).unwrap(), "a new look remakes it");

    // Another Mixxx's profile keeps the skin its DJ chose; YOBUKO's own always takes YOBUKO.
    std::fs::write(profile.join(crate::mixxx::CONFIG), "[Config]\nResizableSkin Deere\n").unwrap();
    assert!(!select(&profile, false).unwrap());
    assert!(select(&profile, true).unwrap());
    let cfg = std::fs::read_to_string(profile.join(crate::mixxx::CONFIG)).unwrap();
    assert!(cfg.contains("ResizableSkin YOBUKO") && cfg.contains("Scheme PaleMoon"), "{cfg}");
    let fresh = dir.path().join("fresh");
    assert!(select(&fresh, false).unwrap(), "a Mixxx with no skin chosen yet takes YOBUKO");
}

#[test]
fn the_soft_shapes_fill_every_name_with_the_look_and_the_deck_colours() {
    for colours in [Look::default().colours(), washi()] {
        let shapes = soft_shapes(&colours);
        assert!(!shapes.contains('@'), "a name is left: {shapes}");
        assert!(shapes.contains(&colours.surface.to_hex()), "the look's cards");
        for colour in DECKS.iter().chain(CUES.iter()) {
            assert!(shapes.contains(colour), "{colour} is missing");
        }
        assert!(shapes.contains("border-radius: 10px"), "round cards");
    }
}

#[test]
fn the_soft_shapes_are_laid_once_over_the_sheet_kept_as_it_was() {
    let shapes = soft_shapes(&Look::default().colours());
    let sheet = "#Deck1 { color: red; }\n";
    let once = with_soft_shapes(sheet, &shapes);
    assert!(once.starts_with("#Deck1 { color: red; }"));
    assert!(!once.contains("SPDX"), "the file's own header stays out of Mixxx's sheet");
    assert_eq!(with_soft_shapes(&once, &shapes), once, "laid again: still once");
    assert_eq!(once.matches(SOFT_MARK).count(), 1);
}

#[test]
fn the_made_skin_has_big_transport_buttons_and_pads_and_a_white_play_icon() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("LateNight");
    for folder in ["decks", "controls", "palemoon/buttons"] {
        std::fs::create_dir_all(source.join(folder)).unwrap();
    }
    std::fs::write(source.join("skin.xml"), "<skin><title>LateNight</title></skin>").unwrap();
    std::fs::write(source.join("style.qss"), "WPushButton { color: #fff; }\n").unwrap();
    std::fs::write(source.join("style_palemoon.qss"), "WLabel { color: #ccc; }\n").unwrap();
    std::fs::write(
        source.join("decks").join("row_5_transportLoopJump.xml"),
        "<ObjectName>CueDeck</ObjectName>\n<Size>42f,26f</Size>\n<Size>68f,26f</Size>\n",
    )
    .unwrap();
    std::fs::write(source.join("controls").join("button_hotcue.xml"), "<MinimumSize>26,26</MinimumSize>").unwrap();
    std::fs::write(source.join("palemoon/buttons/btn__play_deck.svg"), r##"<svg fill="#908e89"/>"##).unwrap();
    let profile = dir.path().join("profile");
    assert!(install(&source, &profile, &Look::default()).unwrap());
    let made = profile.join("skins").join(SKIN);
    let read = |path: &str| std::fs::read_to_string(made.join(path)).unwrap();
    for sheet in STYLE_SHEETS {
        let text = read(sheet);
        assert_eq!(text.matches(SOFT_MARK).count(), 1, "{sheet}");
        assert!(text.contains(DECKS[0]), "{sheet}");
    }
    let transport = read("decks/row_5_transportLoopJump.xml");
    assert!(transport.contains("<Size>48f,34f</Size>") && transport.contains("<Size>76f,34f</Size>"), "{transport}");
    assert!(transport.contains("<ObjectName>CueDeck</ObjectName>"), "every control stays");
    assert_eq!(read("controls/button_hotcue.xml"), "<MinimumSize>30,30</MinimumSize>");
    assert!(read("palemoon/buttons/btn__play_deck.svg").contains("fill=\"#ffffff\""));
}
