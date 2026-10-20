// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Named Arrangement row sizes, their customization and settings-file compatibility.

/// A named row height; units are only exposed in Advanced settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSize {
    Compact,
    Small,
    Medium,
    Large,
}

impl TrackSize {
    pub const ALL: [Self; 4] = [Self::Compact, Self::Small, Self::Medium, Self::Large];
    pub fn index(self) -> usize {
        match self {
            Self::Compact => 0,
            Self::Small => 1,
            Self::Medium => 2,
            Self::Large => 3,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(value))
    }
}

/// Ordered, bounded sizes. Private heights prevent invalid order entering the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackSizes {
    heights: [u16; 4],
    pub tracks: TrackSize,
    pub returns: TrackSize,
    pub advanced: bool,
}

impl Default for TrackSizes {
    fn default() -> Self {
        Self { heights: [20, 40, 64, 104], tracks: TrackSize::Small, returns: TrackSize::Compact, advanced: false }
    }
}

impl TrackSizes {
    pub fn height(self, size: TrackSize) -> u16 {
        self.heights[size.index()]
    }
    /// Neighbours impose strict order; all sizes stay within the view's supported range.
    pub fn bounds(self, size: TrackSize) -> (u16, u16) {
        let i = size.index();
        let lower = if i == 0 { 20 } else { self.heights[i - 1] + 1 };
        let upper = if i == 3 { 400 } else { self.heights[i + 1] - 1 };
        (lower, upper)
    }
    pub fn set_height(&mut self, size: TrackSize, height: u16) {
        let (lower, upper) = self.bounds(size);
        self.heights[size.index()] = height.clamp(lower, upper);
    }
    pub fn to_text(self) -> String {
        let [a, b, c, d] = self.heights;
        format!(
            "arrangement_track_size = {}\narrangement_return_size = {}\narrangement_size_heights = {a},{b},{c},{d}\narrangement_sizes_advanced = {}\n",
            self.tracks.name().to_lowercase(),
            self.returns.name().to_lowercase(),
            if self.advanced { "on" } else { "off" }
        )
    }
    /// Read one complete setting, accepting old files through defaults.
    pub fn parse(&mut self, key: &str, value: &str) -> bool {
        match key {
            "arrangement_track_size" | "arrangement_return_size" => {
                let Some(size) = TrackSize::parse(value) else { return false };
                if key == "arrangement_track_size" {
                    self.tracks = size;
                } else {
                    self.returns = size;
                }
            }
            "arrangement_sizes_advanced" if matches!(value, "on" | "off") => self.advanced = value == "on",
            "arrangement_size_heights" => {
                let mut parsed = [0_u16; 4];
                let mut values = value.split(',');
                for entry in &mut parsed {
                    let Some(Ok(n)) = values.next().map(|s| s.trim().parse::<u16>()) else { return false };
                    *entry = n;
                }
                if values.next().is_some() {
                    return false;
                }
                // Normalize the entire tuple together, independent of settings-line ordering.
                let mut previous = 19;
                for (i, entry) in parsed.iter_mut().enumerate() {
                    *entry = (*entry).clamp(previous + 1, 400 - (3 - i) as u16);
                    previous = *entry;
                }
                self.heights = parsed;
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn presets_and_custom_sizes_round_trip_and_old_files_keep_defaults() {
        let mut settings = Settings::default();
        let sizes = &mut settings.view.track_sizes;
        sizes.tracks = TrackSize::Medium;
        sizes.returns = TrackSize::Small;
        sizes.set_height(TrackSize::Medium, 78);
        sizes.advanced = true;
        let (read, warnings) = Settings::from_text(&settings.to_text());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(read.view.track_sizes, settings.view.track_sizes);
        assert_eq!(Settings::from_text("").0.view.track_sizes, TrackSizes::default());
    }

    #[test]
    fn edits_and_hostile_settings_never_invert_neighbour_sizes() {
        let mut sizes = TrackSizes::default();
        for size in TrackSize::ALL {
            for value in [0, 1, 20, 400, u16::MAX] {
                sizes.set_height(size, value);
                assert!(sizes.heights.windows(2).all(|w| w[0] < w[1]));
                assert!(sizes.heights.iter().all(|h| (20..=400).contains(h)));
            }
        }
        assert!(sizes.parse("arrangement_size_heights", "65535,0,0,0"));
        assert_eq!(sizes.heights, [397, 398, 399, 400]);
        let before = sizes;
        assert!(!sizes.parse("arrangement_size_heights", "20,NaN,64,100"));
        assert_eq!(sizes, before);
        assert!(!sizes.parse("arrangement_size_heights", "20,40,64,100,200"));
        assert_eq!(sizes, before);
    }
}
