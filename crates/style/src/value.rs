// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed native values. No renderer, pixels, or toolkit-specific colour types.
use cssparser::{Parser, ParserInput, Token};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Rgba([u8; 4]),
    Current,
}

impl Color {
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut input = ParserInput::new(source);
        let mut parser = Parser::new(&mut input);
        let color = match parser.next().map_err(|_| "Expected colour")?.clone() {
            Token::Hash(hex) | Token::IDHash(hex) => {
                let (r, g, b, a) =
                    cssparser::color::parse_hash_color(hex.as_bytes()).map_err(|_| "Invalid hex colour")?;
                Self::Rgba([r, g, b, cssparser::color::clamp_unit_f32(a)])
            }
            Token::Ident(name) if name.eq_ignore_ascii_case("currentcolor") => Self::Current,
            Token::Ident(name) if name.eq_ignore_ascii_case("transparent") => Self::Rgba([0; 4]),
            Token::Ident(name) => {
                let (r, g, b) =
                    cssparser::color::parse_named_color(&name.to_ascii_lowercase()).map_err(|_| "Unknown colour")?;
                Self::Rgba([r, g, b, 255])
            }
            Token::Function(name) if name.eq_ignore_ascii_case("rgb") || name.eq_ignore_ascii_case("rgba") => {
                let mut value = None;
                parser
                    .parse_nested_block(|parser| {
                        value = Some(rgb(parser));
                        Ok::<(), cssparser::ParseError<'_, ()>>(())
                    })
                    .map_err(|_| "Invalid rgb()")?;
                Self::Rgba(value.ok_or("Empty rgb()")??)
            }
            Token::Function(name) if name.eq_ignore_ascii_case("hsl") || name.eq_ignore_ascii_case("hsla") => {
                Self::Rgba(crate::sheet::nested(&mut parser, hsl)?)
            }
            Token::Function(name) if name.eq_ignore_ascii_case("color-mix") => {
                Self::Rgba(crate::sheet::nested(&mut parser, mix)?)
            }
            _ => return Err("Unsupported colour value".into()),
        };
        parser.expect_exhausted().map_err(|_| "Unexpected colour suffix")?;
        Ok(color)
    }
}

fn hsl(parser: &mut Parser<'_, '_>) -> Result<[u8; 4], String> {
    let hue = match parser.next().map_err(|_| "Missing hue")? {
        Token::Number { value, .. } => *value,
        Token::Dimension { value, unit, .. } => match unit.as_ref() {
            "deg" => *value,
            "rad" => value.to_degrees(),
            "turn" => value * 360.0,
            "grad" => value * 0.9,
            _ => return Err("Unknown hue unit".into()),
        },
        _ => return Err("Expected a hue".into()),
    };
    let comma = parser.try_parse(|p| p.expect_comma()).is_ok();
    let percent = |parser: &mut Parser<'_, '_>| -> Result<f32, String> {
        match parser.next().map_err(|_| "Missing hsl component")? {
            Token::Percentage { unit_value, .. } => Ok(*unit_value),
            Token::Number { value, .. } => Ok(*value / 100.0),
            _ => Err("Expected a percentage".into()),
        }
    };
    let saturation = percent(parser)?.clamp(0.0, 1.0);
    if comma {
        parser.expect_comma().map_err(|_| "Expected comma")?;
    }
    let lightness = percent(parser)?.clamp(0.0, 1.0);
    let alpha = if parser.is_exhausted() {
        1.0
    } else {
        if comma {
            parser.expect_comma().map_err(|_| "Expected alpha comma")?;
        } else {
            parser.expect_delim('/').map_err(|_| "Expected alpha slash")?;
        }
        match parser.next().map_err(|_| "Missing alpha")? {
            Token::Number { value, .. } => *value,
            Token::Percentage { unit_value, .. } => *unit_value,
            _ => return Err("Expected alpha".into()),
        }
    };
    parser.expect_exhausted().map_err(|_| "Unexpected hsl component")?;
    if ![hue, alpha].iter().all(|v| v.is_finite()) {
        return Err("Non-finite colour".into());
    }
    let hue = hue.rem_euclid(360.0) / 30.0;
    let a = saturation * lightness.min(1.0 - lightness);
    let channel = |n: f32| {
        let k = (n + hue) % 12.0;
        let value = lightness - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Ok([channel(0.0), channel(8.0), channel(4.0), (alpha.clamp(0.0, 1.0) * 255.0).round() as u8])
}

/// Colour spaces `color-mix()` interpolates in, premultiplied by alpha as CSS Color 5 says.
#[derive(Clone, Copy)]
enum Space {
    Srgb,
    SrgbLinear,
    Oklab,
}

fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 { channel / 12.92 } else { ((channel + 0.055) / 1.055).powf(2.4) }
}

fn gamma(channel: f32) -> f32 {
    if channel <= 0.003_130_8 { channel * 12.92 } else { 1.055 * channel.powf(1.0 / 2.4) - 0.055 }
}

impl Space {
    fn to(self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Srgb => [r, g, b],
            Self::SrgbLinear => [linear(r), linear(g), linear(b)],
            Self::Oklab => {
                let [r, g, b] = [linear(r), linear(g), linear(b)];
                let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
                let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
                let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
                [
                    0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
                    1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
                    0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
                ]
            }
        }
    }

    fn from(self, [x, y, z]: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Srgb => [x, y, z],
            Self::SrgbLinear => [gamma(x), gamma(y), gamma(z)],
            Self::Oklab => {
                let l = (x + 0.396_337_78 * y + 0.215_803_76 * z).powi(3);
                let m = (x - 0.105_561_346 * y - 0.063_854_17 * z).powi(3);
                let s = (x - 0.089_484_18 * y - 1.291_485_5 * z).powi(3);
                [
                    gamma(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
                    gamma(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
                    gamma(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
                ]
            }
        }
    }
}

/// `color-mix(in <space>, <colour> [<p>%], <colour> [<p>%])` for literal colours.
fn mix(parser: &mut Parser<'_, '_>) -> Result<[u8; 4], String> {
    parser.expect_ident_matching("in").map_err(|_| "color-mix needs an interpolation space")?;
    let space = match parser.expect_ident().map_err(|_| "Expected a colour space")?.to_ascii_lowercase().as_str() {
        "srgb" => Space::Srgb,
        "srgb-linear" => Space::SrgbLinear,
        "oklab" => Space::Oklab,
        _ => return Err("color-mix supports srgb, srgb-linear and oklab".into()),
    };
    let mut items = Vec::new();
    for _ in 0..2 {
        parser.expect_comma().map_err(|_| "Expected a comma in color-mix")?;
        let start = parser.position();
        let mut percent = None;
        let mut end = start;
        while !parser.is_exhausted() {
            let before = parser.state();
            match parser.next().map_err(|_| "Invalid color-mix item")?.clone() {
                Token::Comma => {
                    parser.reset(&before);
                    break;
                }
                Token::Percentage { unit_value, .. } if percent.is_none() => {
                    percent = Some(unit_value);
                    continue;
                }
                Token::Function(_) => crate::sheet::nested(parser, crate::sheet::consume)?,
                _ => {}
            }
            end = parser.position();
        }
        let text = parser.slice(start..end).trim();
        let Color::Rgba(color) = Color::parse(text)? else {
            return Err("currentColor in color-mix is not supported natively".into());
        };
        items.push((color, percent));
    }
    parser.expect_exhausted().map_err(|_| "color-mix takes two colours")?;
    let [(a, pa), (b, pb)] = <[_; 2]>::try_from(items).map_err(|_| "color-mix takes two colours")?;
    let (pa, pb) = match (pa, pb) {
        (None, None) => (0.5, 0.5),
        (Some(p), None) => (p, 1.0 - p),
        (None, Some(p)) => (1.0 - p, p),
        (Some(a), Some(b)) => (a, b),
    };
    if !(pa.is_finite() && pb.is_finite()) || pa < 0.0 || pb < 0.0 || pa + pb <= 0.0 {
        return Err("Invalid color-mix percentages".into());
    }
    let total = pa + pb;
    let scale = total.min(1.0);
    let weight = pb / total;
    let unit = |c: [u8; 4]| [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0];
    let alpha = |c: [u8; 4]| f32::from(c[3]) / 255.0;
    let (ca, cb) = (space.to(unit(a)), space.to(unit(b)));
    let mixed_alpha = alpha(a) * (1.0 - weight) + alpha(b) * weight;
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let premultiplied = ca[i] * alpha(a) * (1.0 - weight) + cb[i] * alpha(b) * weight;
        out[i] = if mixed_alpha > 0.0 { premultiplied / mixed_alpha } else { 0.0 };
    }
    let rgb = space.from(out);
    let byte = |v: f32| if v.is_finite() { (v.clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 };
    Ok([byte(rgb[0]), byte(rgb[1]), byte(rgb[2]), byte(mixed_alpha * scale)])
}

fn rgb(parser: &mut Parser<'_, '_>) -> Result<[u8; 4], String> {
    let component = |parser: &mut Parser<'_, '_>, alpha: bool| -> Result<u8, String> {
        let value = match parser.next().map_err(|_| "Missing rgb component")? {
            Token::Number { value, .. } => *value * if alpha { 255.0 } else { 1.0 },
            Token::Percentage { unit_value, .. } => *unit_value * 255.0,
            _ => return Err("Expected a numeric rgb component".into()),
        };
        if !value.is_finite() {
            return Err("Non-finite colour".into());
        }
        Ok(cssparser::color::clamp_floor_256_f32(value))
    };
    let red = component(parser, false)?;
    let comma = parser.try_parse(|p| p.expect_comma()).is_ok();
    let green = component(parser, false)?;
    if comma {
        parser.expect_comma().map_err(|_| "Expected comma")?;
    }
    let blue = component(parser, false)?;
    let alpha = if parser.is_exhausted() {
        255
    } else {
        if comma {
            parser.expect_comma().map_err(|_| "Expected alpha comma")?;
        } else {
            parser.expect_delim('/').map_err(|_| "Expected alpha slash")?;
        }
        component(parser, true)?
    };
    parser.expect_exhausted().map_err(|_| "Unexpected rgb component")?;
    Ok([red, green, blue, alpha])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    Px(f32),
    Em(f32),
    Percent(f32),
    Fr(f32),
    Auto,
}

impl Length {
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut input = ParserInput::new(source);
        let mut parser = Parser::new(&mut input);
        let length = match parser.next().map_err(|_| "Expected length")? {
            Token::Number { value, .. } if *value == 0.0 => Self::Px(0.0),
            Token::Dimension { value, unit, .. } if value.is_finite() && value.abs() <= 16384.0 => {
                match unit.as_ref() {
                    "px" => Self::Px(*value),
                    "em" => Self::Em(*value),
                    "fr" if *value >= 0.0 => Self::Fr(*value),
                    _ => return Err("Unsupported length unit".into()),
                }
            }
            Token::Percentage { unit_value, .. } if unit_value.is_finite() && unit_value.abs() <= 100.0 => {
                Self::Percent(*unit_value)
            }
            Token::Ident(value) if *value == "auto" => Self::Auto,
            _ => return Err("Invalid or excessive length".into()),
        };
        parser.expect_exhausted().map_err(|_| "Unexpected length suffix")?;
        Ok(length)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
    Grid,
    Stack,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Color(Color),
    Length(Length),
    Number(f32),
    Integer(i32),
    Bool(bool),
    Layout(Layout),
    Tracks(Vec<Length>),
    Areas(crate::layout::Areas),
    Name(String),
    Shadows(Vec<crate::paint::Shadow>),
    Gradients(Vec<crate::gradient::Linear>),
    /// One keyword of a property's fixed list; empty when the property was never written.
    Keyword(&'static str),
    TrackList(Vec<crate::layout::Track>),
    Placement([crate::layout::GridLine; 2]),
    Transitions(Vec<crate::motion::Transition>),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_values_preserve_alpha_and_reject_non_finite_sizes() {
        assert_eq!(Color::parse("#1230"), Ok(Color::Rgba([17, 34, 51, 0])));
        assert_eq!(Color::parse("rgb(100% 0% 50% / 50%)"), Ok(Color::Rgba([255, 0, 128, 128])));
        assert_eq!(Color::parse("rgba(255, 0, 128, .5)"), Ok(Color::Rgba([255, 0, 128, 128])));
        for bad in ["NaNpx", "1e99px", "-1fr", "calc(1px / 0)", "1px garbage"] {
            assert!(Length::parse(bad).is_err(), "{bad}");
        }
        assert!(Color::parse("rgb(1, 2 3)").is_err());
    }

    #[test]
    fn hsl_and_color_mix_resolve_to_srgb() {
        assert_eq!(Color::parse("hsl(120 100% 50%)"), Ok(Color::Rgba([0, 255, 0, 255])));
        assert_eq!(Color::parse("hsla(0, 100%, 50%, .5)"), Ok(Color::Rgba([255, 0, 0, 128])));
        assert_eq!(Color::parse("color-mix(in srgb, red, blue)"), Ok(Color::Rgba([128, 0, 128, 255])));
        assert_eq!(Color::parse("color-mix(in srgb, red 25%, blue)"), Ok(Color::Rgba([64, 0, 191, 255])));
        // Percentages under 100% scale alpha.
        assert_eq!(Color::parse("color-mix(in srgb, red 20%, blue 30%)"), Ok(Color::Rgba([102, 0, 153, 128])));
        // Oklab keeps white white and mixes in a perceptual space.
        assert_eq!(Color::parse("color-mix(in oklab, white, white)"), Ok(Color::Rgba([255, 255, 255, 255])));
        let Ok(Color::Rgba(mixed)) = Color::parse("color-mix(in oklab, #163440, #7CF0C4 14%)") else {
            panic!("oklab mix")
        };
        assert!(mixed[1] > 0x34 && mixed[1] < 0xf0, "{mixed:?}");
        for bad in [
            "color-mix(in hsl, red, blue)",
            "color-mix(red, blue)",
            "color-mix(in srgb, currentColor, blue)",
            "color-mix(in srgb, red)",
            "color-mix(in srgb, red -5%, blue)",
            "hsl(nan 1% 1%)",
        ] {
            assert!(Color::parse(bad).is_err(), "{bad}");
        }
    }
}
