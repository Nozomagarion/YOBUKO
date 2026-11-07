// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO's look, shared by its three programs. The studio writes the colours of the theme in
//! use to `<data>/YOBUKO/look.json` whenever they change; Mixxx gets a skin recoloured with them
//! (`mixxx_skin`) and rbxport reads the file itself (`third_party/rbxport-app/src-tauri/src/yobuko.rs`),
//! so the three windows read as one app whichever theme the musician picks.
//!
//! Keep the field names in step with rbxport's `yobuko.rs` and `src/styles/yobuko.css`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The version of `look.json` this code writes.
pub const FORMAT: u32 = 1;

/// The colours of the theme in use, each `#RRGGBB`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Look {
    pub format: u32,
    /// The theme's id in the studio's settings, with its variant (`yobuko@washi`).
    pub theme: String,
    /// Whether its window is dark (light text on a dark ground).
    pub dark: bool,
    /// The colour of the app mode showing now: blue Studio, green Multiplayer, pink DJ.
    pub mode: String,
    /// The window behind everything.
    pub ground: String,
    /// Panels and bars.
    pub surface: String,
    /// Fields and buttons.
    pub control: String,
    pub control_hover: String,
    /// A selected or pressed field.
    pub control_active: String,
    /// Hairlines between regions.
    pub line: String,
    pub line_strong: String,
    pub text: String,
    pub text_dim: String,
    pub hint: String,
    /// Text on a lit fill.
    pub inverse: String,
    pub accent: String,
    pub selection: String,
    pub recording: String,
    pub meter_normal: String,
    pub meter_hot: String,
}

impl Default for Look {
    /// Ink & Echo, YOBUKO's own theme.
    fn default() -> Self {
        Self {
            format: FORMAT,
            theme: "yobuko".into(),
            dark: true,
            mode: "#DB2777".into(),
            ground: "#1D1C1A".into(),
            surface: "#252421".into(),
            control: "#343330".into(),
            control_hover: "#3F3D3A".into(),
            control_active: "#4B4A46".into(),
            line: "#343330".into(),
            line_strong: "#484642".into(),
            text: "#ECE8E1".into(),
            text_dim: "#B5B2AC".into(),
            hint: "#85837E".into(),
            inverse: "#1D1C1A".into(),
            accent: "#A6A3FF".into(),
            selection: "#E3C86A".into(),
            recording: "#E5484D".into(),
            meter_normal: "#8BC48A".into(),
            meter_hot: "#E3C86A".into(),
        }
    }
}

/// Where the studio writes the look: `<data>/YOBUKO/look.json`. Keep in step with rbxport's
/// `yobuko.rs` (`look_path`).
pub fn path(data_dir: &Path) -> PathBuf {
    crate::mixxx::yobuko_dir(data_dir).join("look.json")
}

/// The look in `path`, or `None` when there is none yet or it does not read.
pub fn read(path: &Path) -> Option<Look> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Writes `look` to `path` (whole, through a temporary file) unless it holds it already.
/// `true` when the file changed.
pub fn write(path: &Path, look: &Look) -> std::io::Result<bool> {
    let text = serde_json::to_string_pretty(look).map_err(std::io::Error::other)? + "\n";
    if std::fs::read_to_string(path).is_ok_and(|before| before == text) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("json.tmp");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)?;
    Ok(true)
}

/// An sRGB colour with alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };

    /// `#RRGGBB` (or `#RGB`); `None` for anything else.
    pub fn hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        let value = |s: &str| u8::from_str_radix(s, 16).ok();
        match digits.len() {
            3 => {
                let d: Vec<u8> = digits.chars().map(|c| c.to_digit(16).map(|v| v as u8 * 17)).collect::<Option<_>>()?;
                Some(Self { r: d[0], g: d[1], b: d[2], a: 255 })
            }
            6 => Some(Self { r: value(&digits[0..2])?, g: value(&digits[2..4])?, b: value(&digits[4..6])?, a: 255 }),
            _ => None,
        }
    }

    /// `#RRGGBB`, without alpha.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// `self` mixed with `other`: `amount` 0 is `self`, 1 is `other`. Alpha stays `self`'s.
    #[must_use]
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let t = amount.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self { r: m(self.r, other.r), g: m(self.g, other.g), b: m(self.b, other.b), a: self.a }
    }

    /// HSL lightness, 0 to 1.
    pub fn lightness(self) -> f32 {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        (f32::from(max) + f32::from(min)) / 510.0
    }

    /// How far from grey, 0 to 1 (chroma).
    pub fn chroma(self) -> f32 {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        f32::from(max - min) / 255.0
    }

    /// Hue in degrees, 0 to 360 (0 for greys).
    pub fn hue(self) -> f32 {
        let (r, g, b) = (f32::from(self.r), f32::from(self.g), f32::from(self.b));
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let c = max - min;
        if c == 0.0 {
            return 0.0;
        }
        let h = if max == r {
            ((g - b) / c).rem_euclid(6.0)
        } else if max == g {
            (b - r) / c + 2.0
        } else {
            (r - g) / c + 4.0
        };
        h * 60.0
    }
}

/// The look's colours, read, with Ink & Echo's for any that do not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colours {
    pub dark: bool,
    pub mode: Rgba,
    pub ground: Rgba,
    pub surface: Rgba,
    pub control: Rgba,
    pub control_hover: Rgba,
    pub control_active: Rgba,
    pub line: Rgba,
    pub line_strong: Rgba,
    pub text: Rgba,
    pub text_dim: Rgba,
    pub hint: Rgba,
    pub inverse: Rgba,
    pub accent: Rgba,
    pub selection: Rgba,
    pub recording: Rgba,
    pub meter_normal: Rgba,
    pub meter_hot: Rgba,
}

impl Look {
    /// Its colours, read.
    pub fn colours(&self) -> Colours {
        let fallback = Self::default();
        let pick = |mine: &str, theirs: &str| Rgba::hex(mine).or_else(|| Rgba::hex(theirs)).unwrap_or(Rgba::BLACK);
        Colours {
            dark: self.dark,
            mode: pick(&self.mode, &fallback.mode),
            ground: pick(&self.ground, &fallback.ground),
            surface: pick(&self.surface, &fallback.surface),
            control: pick(&self.control, &fallback.control),
            control_hover: pick(&self.control_hover, &fallback.control_hover),
            control_active: pick(&self.control_active, &fallback.control_active),
            line: pick(&self.line, &fallback.line),
            line_strong: pick(&self.line_strong, &fallback.line_strong),
            text: pick(&self.text, &fallback.text),
            text_dim: pick(&self.text_dim, &fallback.text_dim),
            hint: pick(&self.hint, &fallback.hint),
            inverse: pick(&self.inverse, &fallback.inverse),
            accent: pick(&self.accent, &fallback.accent),
            selection: pick(&self.selection, &fallback.selection),
            recording: pick(&self.recording, &fallback.recording),
            meter_normal: pick(&self.meter_normal, &fallback.meter_normal),
            meter_hot: pick(&self.meter_hot, &fallback.meter_hot),
        }
    }
}

#[cfg(test)]
#[path = "look_tests.rs"]
mod tests;
