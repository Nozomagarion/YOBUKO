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
        "{}\n{}\n{}\n",
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
