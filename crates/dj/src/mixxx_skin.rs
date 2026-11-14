// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mixxx dressed as YOBUKO. Mixxx draws its window from a skin (XML layouts, Qt style sheets
//! and SVG artwork); YOBUKO copies LateNight, the skin every Mixxx ships, into the profile's
//! own `skins` folder as "YOBUKO" with every colour moved onto the studio's theme (`look`),
//! and selects it. A copy is remade only when the look or the Mixxx it came from changes.
//!
//! LateNight is GPL-2.0-or-later (© the Mixxx team); the recoloured copy stays under it and
//! is made on the DJ's computer from the Mixxx they run, so it always fits that Mixxx.

use std::path::{Path, PathBuf};

use crate::look::{Colours, Look, Rgba};
use crate::mixxx::Settings;

/// The skin's name in Mixxx, its folder in `<profile>/skins`.
pub const SKIN: &str = "YOBUKO";
/// The skin it is made from, shipped with every Mixxx from 2.3 on.
pub const SOURCE: &str = "LateNight";
/// LateNight's dark colour scheme, the one recoloured to the look.
pub const SCHEME: &str = "PaleMoon";
/// The note in the made skin saying what it was made from, so it is remade only when needed.
const STAMP: &str = "yobuko-look.json";
/// The rounded, colourful YOBUKO shapes laid over LateNight's own style sheets.
const SOFT: &str = include_str!("mixxx_skin_soft.qss");
/// Where the soft shapes start in a style sheet, so they are never laid twice.
const SOFT_MARK: &str = "/* ===== YOBUKO soft shapes";
/// The style sheets the soft shapes are appended to: LateNight's and its dark scheme's.
const STYLE_SHEETS: [&str; 2] = ["style.qss", "style_palemoon.qss"];
/// The deck colours of YOBUKO's own DJ page: A pink, B green, C violet, D amber.
pub const DECKS: [&str; 4] = ["#DB2777", "#10B981", "#8B5CF6", "#F59E0B"];
/// The hot cue pads' colours, 1 to 8, as on YOBUKO's own DJ page.
pub const CUES: [&str; 8] = ["#EF4444", "#F59E0B", "#EAB308", "#22C55E", "#06B6D4", "#3B82F6", "#8B5CF6", "#EC4899"];

/// Mixxx's `skins` folders for the Mixxx at `mixxx` (its app on macOS, its program
/// elsewhere), most likely first.
pub fn skins_dirs(mixxx: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if mixxx.extension().is_some_and(|e| e == "app") {
        out.push(mixxx.join("Contents").join("Resources").join("skins"));
    }
    if let Some(dir) = mixxx.parent() {
        out.push(dir.join("skins"));
        out.push(dir.join("res").join("skins"));
        if let Some(prefix) = dir.parent() {
            out.push(prefix.join("share").join("mixxx").join("skins"));
        }
    }
    out.push(PathBuf::from("/usr/share/mixxx/skins"));
    out.push(PathBuf::from("/usr/local/share/mixxx/skins"));
    out
}

/// The LateNight to start from, for the Mixxx at `mixxx`.
pub fn source(mixxx: &Path) -> Option<PathBuf> {
    skins_dirs(mixxx).into_iter().map(|dir| dir.join(SOURCE)).find(|dir| dir.join("skin.xml").is_file())
}

/// Makes `<profile>/skins/YOBUKO` from the LateNight at `source`, in `look`'s colours. Nothing
/// is written when the skin there was made from the same LateNight and look. `true` when it
/// was (re)made.
pub fn install(source: &Path, profile: &Path, look: &Look) -> std::io::Result<bool> {
    let target = profile.join("skins").join(SKIN);
    let stamp = format!(
        "soft shapes {:016x}\n{}\n{}\n{}\n",
        fingerprint(SOFT),
        source.display(),
        std::fs::metadata(source.join("skin.xml"))
            .and_then(|m| m.modified())
            .map(|t| format!("{t:?}"))
            .unwrap_or_default(),
        serde_json::to_string(look).unwrap_or_default()
    );
    if std::fs::read_to_string(target.join(STAMP)).is_ok_and(|before| before == stamp) {
        return Ok(false);
    }
    let colours = look.colours();
    let partial = profile.join("skins").join(format!("{SKIN}.partial"));
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    let base = format!("{}/", target.to_string_lossy().replace('\\', "/"));
    copy_recoloured(source, &partial, &base, &colours)?;
    for (file, svg) in [("mixxx_logo_small.svg", logo_small(&colours)), ("mixxx_logo.svg", logo(&colours))] {
        let path = partial.join("palemoon").join("style").join(file);
        if path.exists() {
            std::fs::write(path, svg)?;
        }
    }
    soften(&partial, &colours)?;
    std::fs::write(partial.join(STAMP), stamp)?;
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::rename(&partial, &target)?;
    Ok(true)
}

fn copy_recoloured(from: &Path, to: &Path, base: &str, colours: &Colours) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_recoloured(&path, &target, base, colours)?;
            continue;
        }
        let kind = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        if matches!(kind.as_str(), "xml" | "qss" | "svg") {
            let text = std::fs::read_to_string(&path)?;
            let text = relink(&text, base);
            let text = if kind == "xml" && entry.file_name() == "skin.xml" { retitle(&text) } else { text };
            std::fs::write(&target, recolour(&text, colours, kind == "svg"))?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// The red 呼子 seal and the YOBUKO name, in place of Mixxx's logo at the top right (74 × 24).
fn logo_small(c: &Colours) -> String {
    format!(
        r##"<svg width="74" height="24" version="1.1" xmlns="http://www.w3.org/2000/svg">
  <rect x="1" y="3" width="18" height="18" rx="1.5" fill="#C8372D"/>
  <g font-family="Hiragino Mincho ProN, Yu Mincho, Noto Serif CJK JP, serif" font-weight="700" font-size="7.5" fill="#F7E9DA" text-anchor="middle"><text x="10" y="11">呼</text><text x="10" y="19">子</text></g>
  <text x="24" y="16.5" font-family="Manrope, Helvetica Neue, Segoe UI, sans-serif" font-weight="700" font-size="10" letter-spacing="1.6" fill="{}">YOBUKO</text>
</svg>
"##,
        c.text.to_hex()
    )
}

/// The same, larger, crediting Mixxx (160 × 40).
fn logo(c: &Colours) -> String {
    format!(
        r##"<svg width="160" height="40" version="1.1" xmlns="http://www.w3.org/2000/svg">
  <rect x="2" y="5" width="30" height="30" rx="2" fill="#C8372D"/>
  <g font-family="Hiragino Mincho ProN, Yu Mincho, Noto Serif CJK JP, serif" font-weight="700" font-size="12" fill="#F7E9DA" text-anchor="middle"><text x="17" y="18">呼</text><text x="17" y="31">子</text></g>
  <text x="40" y="22" font-family="Manrope, Helvetica Neue, Segoe UI, sans-serif" font-weight="700" font-size="15" letter-spacing="2.6" fill="{}">YOBUKO</text>
  <text x="40" y="34" font-family="Manrope, Helvetica Neue, Segoe UI, sans-serif" font-size="8.5" fill="{}">DJ · with Mixxx</text>
</svg>
"##,
        c.text.to_hex(),
        c.hint.to_hex()
    )
}

/// LateNight's links to its own files (`skins:LateNight/…`) pointed at the copy's folder
/// `base`. Mixxx resolves `skins:` in its own install only, never in the profile, so the copy
/// names its folder in full; inside a style sheet's `url(…)` the path is quoted, as profile
/// folders can hold spaces ("Application Support").
pub fn relink(text: &str, base: &str) -> String {
    const LINK: &str = "skins:LateNight/";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(LINK) {
        let before = &rest[..at];
        let after = &rest[at + LINK.len()..];
        if before.ends_with("url(") {
            let end = after.find(')').unwrap_or(after.len());
            out.push_str(before);
            out.push('"');
            out.push_str(base);
            out.push_str(after[..end].trim_end_matches(['"', '\'']));
            out.push('"');
            rest = &after[end..];
        } else {
            out.push_str(before);
            out.push_str(base);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Lays the soft shapes over the skin made at `skin`: rounded cards and pills, big CUE and
/// PLAY buttons, a colour per hot cue pad, the deck colours (style sheets appended to, sizes
/// changed in two layouts, a white play icon). Only looks change: every control stays.
fn soften(skin: &Path, colours: &Colours) -> std::io::Result<()> {
    let shapes = soft_shapes(colours);
    for name in STYLE_SHEETS {
        let path = skin.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            std::fs::write(&path, with_soft_shapes(&text, &shapes))?;
        }
    }
    for (file, change) in [
        (["decks", "row_5_transportLoopJump.xml"], bigger_transport as fn(&str) -> String),
        (["controls", "button_hotcue.xml"], bigger_pads),
    ] {
        let path = skin.join(file[0]).join(file[1]);
        if let Ok(text) = std::fs::read_to_string(&path) {
            std::fs::write(&path, change(&text))?;
        }
    }
    let play = skin.join("palemoon").join("buttons").join("btn__play_deck.svg");
    if play.exists() {
        std::fs::write(play, PLAY_ICON)?;
    }
    Ok(())
}

/// A fingerprint of `text` (FNV-1a), so a changed style sheet remakes every skin made before it.
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// `sheet` with `shapes` at its end, in place of any shapes laid before.
pub fn with_soft_shapes(sheet: &str, shapes: &str) -> String {
    let own = sheet.find(SOFT_MARK).map_or(sheet, |at| &sheet[..at]);
    // The file's own header comment is kept out: only the marked part is laid.
    let shapes = shapes.find(SOFT_MARK).map_or(shapes, |at| &shapes[at..]);
    format!("{}\n{shapes}", own.trim_end())
}

/// The soft shapes' style sheet in `colours` and the deck and hot cue colours.
pub fn soft_shapes(colours: &Colours) -> String {
    let c = colours;
    let deck = |i: usize| Rgba::hex(DECKS[i]).unwrap_or(c.accent);
    let cue = |i: usize| Rgba::hex(CUES[i]).unwrap_or(c.accent);
    let mut names: Vec<(String, Rgba)> = vec![
        ("ground".into(), c.ground),
        ("surface_alt".into(), c.surface.mix(c.text, 0.03)),
        ("surface".into(), c.surface),
        ("control_strong".into(), c.control.mix(c.text, 0.06)),
        ("control".into(), c.control),
        ("line".into(), c.line),
        ("accent".into(), c.accent),
        ("text".into(), c.text),
        ("muted".into(), c.text_dim),
        ("ink".into(), Rgba { r: 0x16, g: 0x11, b: 0x0F, a: 255 }),
        ("white".into(), Rgba::WHITE),
    ];
    for i in 0..DECKS.len() {
        names.push((format!("deck{}soft", i + 1), deck(i).mix(c.surface, 0.55)));
        names.push((format!("deck{}", i + 1), deck(i)));
    }
    for i in 0..CUES.len() {
        names.push((format!("cue{}dim", i + 1), cue(i).mix(c.surface, 0.8)));
        names.push((format!("cue{}", i + 1), cue(i)));
    }
    // Longest first, so `@deck1soft` is never read as `@deck1` and `soft`.
    names.sort_by_key(|name| std::cmp::Reverse(name.0.len()));
    let mut out = SOFT.to_owned();
    for (name, colour) in names {
        out = out.replace(&format!("@{name}"), &colour.to_hex());
    }
    out
}

/// CUE and PLAY taller (34 px, from 26) and PLAY wider, so they read as big round buttons.
fn bigger_transport(xml: &str) -> String {
    xml.replace("<Size>42f,26f</Size>", "<Size>48f,34f</Size>")
        .replace("68f,26f", "76f,34f")
        .replace(
            "<SetVariable name=\"Size\">26f,26f</SetVariable>\n                    <SetVariable name=\"BtnSize\">square</SetVariable>\n                    <SetVariable name=\"ConfigKey\"><Variable name=\"Group\"/>,reverse</SetVariable>",
            "<SetVariable name=\"Size\">26f,34f</SetVariable>\n                    <SetVariable name=\"BtnSize\">square</SetVariable>\n                    <SetVariable name=\"ConfigKey\"><Variable name=\"Group\"/>,reverse</SetVariable>",
        )
}

/// Hot cue pads a little bigger (30 px, from 26).
fn bigger_pads(xml: &str) -> String {
    xml.replace("26,26", "30,30")
}

/// LateNight's play and pause icon in white, to sit on the deck's colour.
const PLAY_ICON: &str = r##"<svg width="100" height="48" version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 50 24">
  <g transform="translate(1.00049 1)" fill="#ffffff">
    <path d="m27 5h4v12h-4v-12"/>
    <path d="m34 5v12h4v-12h-4"/>
    <path d="m11 5 8.9995 6-8.9995 6v-12"/>
  </g>
</svg>
"##;

/// The skin's own title, shown in Mixxx's preferences.
fn retitle(xml: &str) -> String {
    match (xml.find("<title>"), xml.find("</title>")) {
        (Some(start), Some(end)) if start < end => {
            format!("{}<title>{SKIN}</title>{}", &xml[..start], &xml[end + "</title>".len()..])
        }
        _ => xml.to_owned(),
    }
}

/// Selects the YOBUKO skin in `profile`'s settings when that is the DJ's to have: always in
/// YOBUKO's own profile (`own`), and in another Mixxx's profile only while it has no skin
/// chosen, or this one, so a DJ's own choice is kept.
pub fn select(profile: &Path, own: bool) -> std::io::Result<bool> {
    let path = profile.join(crate::mixxx::CONFIG);
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    let mut settings = Settings::parse(&before);
    let chosen = settings.get("[Config]", "ResizableSkin").map(str::to_owned);
    if !own && chosen.as_deref().is_some_and(|skin| skin != SKIN) {
        return Ok(false);
    }
    settings.set("[Config]", "ResizableSkin", SKIN);
    settings.set("[Config]", "Scheme", SCHEME);
    let after = settings.render();
    if after == before {
        return Ok(false);
    }
    std::fs::create_dir_all(profile)?;
    let partial = path.with_extension("cfg.tmp");
    std::fs::write(&partial, after)?;
    std::fs::rename(&partial, &path)?;
    Ok(true)
}

/// Every colour in `text` (`#RGB`, `#RRGGBB`, `#AARRGGBB` in Qt, `#RRGGBBAA` in SVG,
/// `rgb(…)` and `rgba(…)`) moved onto the look.
pub fn recolour(text: &str, colours: &Colours, svg: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' && (i == 0 || !matches!(bytes[i - 1], b'(' | b'&')) {
            let digits = bytes[i + 1..].iter().take_while(|b| b.is_ascii_hexdigit()).count();
            let ends = bytes.get(i + 1 + digits).is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_' && *b != b'-');
            if ends && matches!(digits, 3 | 6 | 8) {
                let hex = &text[i + 1..i + 1 + digits];
                if let Some(colour) = parse_hash(hex, svg) {
                    out.push_str(&text[copied..i]);
                    out.push_str(&write_hash(map(colour, colours), digits, svg));
                    i += 1 + digits;
                    copied = i;
                    continue;
                }
            }
        }
        let rgb = bytes[i..].len() > 4 && bytes[i..i + 4].eq_ignore_ascii_case(b"rgb(");
        let rgba = bytes[i..].len() > 5 && bytes[i..i + 5].eq_ignore_ascii_case(b"rgba(");
        if (rgb || rgba) && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let open = i + if rgb { 3 } else { 4 };
            if let Some(close) = text[open..].find(')').map(|c| open + c)
                && let Some((colour, alpha)) = parse_rgb(&text[open + 1..close])
            {
                let mapped = map(colour, colours);
                out.push_str(&text[copied..i]);
                match alpha {
                    Some(a) => out.push_str(&format!("rgba({}, {}, {}, {a})", mapped.r, mapped.g, mapped.b)),
                    None => out.push_str(&format!("rgb({}, {}, {})", mapped.r, mapped.g, mapped.b)),
                }
                i = close + 1;
                copied = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[copied..]);
    out
}

fn parse_hash(hex: &str, svg: bool) -> Option<Rgba> {
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 => Rgba::hex(&format!("#{hex}")),
        6 => Rgba::hex(&format!("#{hex}")),
        8 if svg => {
            Some(Rgba { r: byte(&hex[0..2])?, g: byte(&hex[2..4])?, b: byte(&hex[4..6])?, a: byte(&hex[6..8])? })
        }
        8 => Some(Rgba { a: byte(&hex[0..2])?, r: byte(&hex[2..4])?, g: byte(&hex[4..6])?, b: byte(&hex[6..8])? }),
        _ => None,
    }
}

fn write_hash(c: Rgba, digits: usize, svg: bool) -> String {
    match digits {
        8 if svg => format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
        8 => format!("#{:02x}{:02x}{:02x}{:02x}", c.a, c.r, c.g, c.b),
        _ => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
    }
}

fn parse_rgb(inside: &str) -> Option<(Rgba, Option<String>)> {
    let parts: Vec<&str> = inside.split(',').map(str::trim).collect();
    let channel = |s: &str| s.parse::<f32>().ok().map(|v| v.clamp(0.0, 255.0).round() as u8);
    match parts.as_slice() {
        [r, g, b] => Some((Rgba { r: channel(r)?, g: channel(g)?, b: channel(b)?, a: 255 }, None)),
        [r, g, b, a] => Some((Rgba { r: channel(r)?, g: channel(g)?, b: channel(b)?, a: 255 }, Some((*a).to_owned()))),
        _ => None,
    }
}

/// One of LateNight's colours on the look. Greys follow their lightness up the look's own
/// ramp (ground, panels, fields, hints, text), so a light theme turns the skin light too;
/// reds stay alarms; every other colour becomes the accent, as dark or bright as it was.
pub fn map(colour: Rgba, look: &Colours) -> Rgba {
    let alpha = colour.a;
    let l = colour.lightness();
    let mapped = if colour.chroma() < 0.22 || (colour.chroma() < 0.4 && !(0.2..=0.85).contains(&l)) {
        let deep = look.ground.mix(if look.dark { Rgba::BLACK } else { Rgba::WHITE }, 0.45);
        let ramp = [
            (0.0, deep),
            (0.05, look.ground.mix(deep, 0.5)),
            (0.09, look.ground),
            (0.14, look.surface),
            (0.2, look.control),
            (0.3, look.control_active),
            (0.45, look.hint),
            (0.7, look.text_dim),
            (1.0, look.text),
        ];
        let mut out = look.text;
        for pair in ramp.windows(2) {
            let ((l0, c0), (l1, c1)) = (pair[0], pair[1]);
            if l <= l1 {
                out = c0.mix(c1, (l - l0) / (l1 - l0));
                break;
            }
        }
        out
    } else {
        let hue = colour.hue();
        let base = if !(15.0..=345.0).contains(&hue) && colour.r > 120 { look.recording } else { look.accent };
        if l < 0.3 {
            base.mix(look.ground, 0.55)
        } else if l < 0.45 {
            base.mix(look.ground, 0.25)
        } else {
            base
        }
    };
    Rgba { a: alpha, ..mapped }
}

#[cfg(test)]
#[path = "mixxx_skin_tests.rs"]
mod tests;
