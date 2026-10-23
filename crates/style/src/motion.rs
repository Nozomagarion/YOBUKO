// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CSS transitions: which properties change smoothly, for how long and with which easing.
//! The host interpolates; this module only parses and evaluates timing functions.
use crate::property::Property;

/// Most transitions on one element.
pub const MAX_TRANSITIONS: usize = 8;
/// Longest duration or delay, in seconds.
pub const MAX_SECONDS: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    /// `cubic-bezier(x1, y1, x2, y2)`; the keywords are fixed curves of it.
    CubicBezier([f32; 4]),
    /// `steps(n, jump-end)` or `steps(n, jump-start)`.
    Steps(u16, bool),
}

impl Easing {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "linear" => Self::Linear,
            "ease" => Self::CubicBezier([0.25, 0.1, 0.25, 1.0]),
            "ease-in" => Self::CubicBezier([0.42, 0.0, 1.0, 1.0]),
            "ease-out" => Self::CubicBezier([0.0, 0.0, 0.58, 1.0]),
            "ease-in-out" => Self::CubicBezier([0.42, 0.0, 0.58, 1.0]),
            "step-start" => Self::Steps(1, true),
            "step-end" => Self::Steps(1, false),
            _ => {
                if let Some(inner) = word.strip_prefix("cubic-bezier(").and_then(|w| w.strip_suffix(')')) {
                    let values: Vec<f32> = inner.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                    let [x1, y1, x2, y2] = <[f32; 4]>::try_from(values).ok()?;
                    if ![x1, y1, x2, y2].iter().all(|v| v.is_finite())
                        || !(0.0..=1.0).contains(&x1)
                        || !(0.0..=1.0).contains(&x2)
                        || y1.abs() > 16.0
                        || y2.abs() > 16.0
                    {
                        return None;
                    }
                    Self::CubicBezier([x1, y1, x2, y2])
                } else {
                    let inner = word.strip_prefix("steps(").and_then(|w| w.strip_suffix(')'))?;
                    let mut parts = inner.split(',').map(str::trim);
                    let count = parts.next()?.parse::<u16>().ok().filter(|n| (1..=1000).contains(n))?;
                    let start = match parts.next() {
                        None | Some("jump-end" | "end") => false,
                        Some("jump-start" | "start") => true,
                        _ => return None,
                    };
                    if parts.next().is_some() {
                        return None;
                    }
                    Self::Steps(count, start)
                }
            }
        })
    }

    /// The eased progress for linear progress `t` in `0..=1`.
    pub fn apply(self, t: f32) -> f32 {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 1.0 };
        match self {
            Self::Linear => t,
            Self::Steps(count, start) => {
                let count = f32::from(count);
                let step = (t * count).floor() + if start { 1.0 } else { 0.0 };
                (step / count).clamp(0.0, 1.0)
            }
            Self::CubicBezier([x1, y1, x2, y2]) => {
                // Solve x(s) = t by bisection: bounded work, monotonic for x1, x2 in 0..=1.
                let curve = |s: f32, a: f32, b: f32| {
                    let r = 1.0 - s;
                    3.0 * r * r * s * a + 3.0 * r * s * s * b + s * s * s
                };
                let (mut low, mut high) = (0.0f32, 1.0f32);
                for _ in 0..24 {
                    let middle = (low + high) * 0.5;
                    if curve(middle, x1, x2) < t {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                curve((low + high) * 0.5, y1, y2)
            }
        }
    }
}

/// One `transition` item. `property` is `None` for `all`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub property: Option<Property>,
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
}

/// Properties the native renderer interpolates. Others change at once.
pub fn animatable(property: Property) -> bool {
    matches!(
        property,
        Property::Color
            | Property::BackgroundColor
            | Property::BorderColor
            | Property::Opacity
            | Property::Radius
            | Property::FontSize
    )
}

fn seconds(word: &str) -> Option<f32> {
    let value = if let Some(ms) = word.strip_suffix("ms") {
        ms.parse::<f32>().ok()? / 1000.0
    } else {
        word.strip_suffix('s')?.parse::<f32>().ok()?
    };
    (value.is_finite() && (0.0..=MAX_SECONDS).contains(&value)).then_some(value)
}

/// `transition: <property> <duration> [<easing>] [<delay>], ...` or `none`.
pub(crate) fn parse(source: &str) -> Result<Vec<Transition>, String> {
    crate::syntax::validate(source)?;
    let source = source.trim();
    if source == "none" {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut items = vec![String::new()];
    for c in source.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(item) = items.last_mut() {
            item.push(c);
        }
    }
    if items.len() > MAX_TRANSITIONS {
        return Err("At most eight transitions".into());
    }
    for item in items {
        let mut property = None;
        let mut named = false;
        let mut times = Vec::new();
        let mut easing = None;
        // Split at whitespace outside parentheses so cubic-bezier(...) stays whole.
        let mut words = Vec::new();
        let mut current = String::new();
        let mut depth = 0usize;
        for c in item.trim().chars() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if c.is_whitespace() && depth == 0 {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() {
            words.push(current);
        }
        if words.is_empty() {
            return Err("Empty transition".into());
        }
        for word in &words {
            let word = word.to_ascii_lowercase();
            if let Some(time) = seconds(&word) {
                if times.len() >= 2 {
                    return Err("A transition takes a duration and a delay".into());
                }
                times.push(time);
            } else if let Some(curve) = Easing::parse(&word) {
                if easing.replace(curve).is_some() {
                    return Err("A transition takes one easing".into());
                }
            } else if !named {
                named = true;
                property = match word.as_str() {
                    "all" => None,
                    "background" => Some(Property::BackgroundColor),
                    name => Some(Property::from_name(name).ok_or("Unknown transition property")?),
                };
            } else {
                return Err("Invalid transition".into());
            }
        }
        result.push(Transition {
            property,
            duration: times.first().copied().unwrap_or(0.0),
            delay: times.get(1).copied().unwrap_or(0.0),
            easing: easing.unwrap_or(Easing::CubicBezier([0.25, 0.1, 0.25, 1.0])),
        });
    }
    Ok(result)
}

/// The transition that applies to `property`, the last one listed for it, as in CSS.
pub fn find(transitions: &[Transition], property: Property) -> Option<Transition> {
    transitions
        .iter()
        .rev()
        .find(|t| t.property.is_none_or(|p| p == property) && t.duration > 0.0)
        .copied()
        .filter(|_| animatable(property))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_parse_durations_easings_and_delays() {
        let list = parse("background-color 140ms ease-out, border-color 0.2s cubic-bezier(.1, .7, 1, .1) 50ms, all 1s")
            .unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].property, Some(Property::BackgroundColor));
        assert!((list[0].duration - 0.14).abs() < 1e-6);
        assert_eq!(list[1].easing, Easing::CubicBezier([0.1, 0.7, 1.0, 0.1]));
        assert!((list[1].delay - 0.05).abs() < 1e-6);
        assert_eq!(find(&list, Property::Color).map(|t| t.duration), Some(1.0));
        assert_eq!(find(&list, Property::BoxShadow), None, "shadows switch at once");
        assert!(parse("none").unwrap().is_empty());
        for bad in ["color 99s", "color 1s 1s 1s", "nothing 1s", "color 1s ease ease", "cubic-bezier(2,0,0,1) 1s"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn easing_curves_start_at_zero_and_end_at_one() {
        for easing in ["linear", "ease", "ease-in", "ease-out", "ease-in-out", "steps(4)", "steps(2, jump-start)"] {
            let curve = Easing::parse(easing).unwrap();
            assert!(curve.apply(1.0) > 0.999, "{easing}");
            assert!(curve.apply(f32::NAN) > 0.999);
        }
        assert!(Easing::parse("ease").unwrap().apply(0.0).abs() < 1e-4);
        assert!((Easing::Linear.apply(0.25) - 0.25).abs() < 1e-6);
        assert_eq!(Easing::Steps(4, false).apply(0.3), 0.25);
    }
}
