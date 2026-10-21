// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native wallpaper material. Transparent clearing is allowed only after a successful
//! Mica request; an unsupported OS never falls through to desktop transparency.

#[derive(Default)]
pub struct Mica {
    requested: Option<(bool, bool)>,
    active: bool,
}

impl Mica {
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn sync(&mut self, frame: &eframe::Frame, enabled: bool, dark: bool) -> Option<String> {
        self.sync_with(enabled, dark, |enabled, dark| apply(frame, enabled, dark))
    }

    fn sync_with(
        &mut self,
        enabled: bool,
        dark: bool,
        apply: impl FnOnce(bool, bool) -> Result<(), String>,
    ) -> Option<String> {
        if self.requested == Some((enabled, dark)) {
            return None;
        }
        self.requested = Some((enabled, dark));
        // Nothing to remove on startup or after a failed enable request.
        if !enabled && !self.active {
            return None;
        }
        self.active = false;
        match apply(enabled, dark) {
            Ok(()) => {
                self.active = enabled;
                None
            }
            Err(error) => Some(format!("Windows Mica: {error}; using a solid background")),
        }
    }
}

#[cfg(target_os = "windows")]
fn apply(frame: &eframe::Frame, enabled: bool, dark: bool) -> Result<(), String> {
    if enabled { window_vibrancy::apply_mica(frame, Some(dark)) } else { window_vibrancy::clear_mica(frame) }
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
fn apply(_frame: &eframe::Frame, enabled: bool, _dark: bool) -> Result<(), String> {
    if enabled { Err("requires Windows 11".into()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_enable_keeps_the_window_opaque_and_does_not_retry_each_frame() {
        let mut mica = Mica::default();
        assert!(mica.sync_with(true, true, |_, _| Err("unsupported".into())).is_some());
        assert!(!mica.active());
        let mut called = false;
        mica.sync_with(true, true, |_, _| {
            called = true;
            Ok(())
        });
        assert!(!called);
        mica.sync_with(false, true, |_, _| Ok(()));
        mica.sync_with(true, true, |_, _| Ok(()));
        assert!(mica.active());
    }

    #[test]
    fn failed_disable_still_restores_opaque_drawing() {
        let mut mica = Mica::default();
        mica.sync_with(true, false, |_, _| Ok(()));
        assert!(mica.active());
        assert!(mica.sync_with(false, false, |_, _| Err("unavailable".into())).is_some());
        assert!(!mica.active());
    }
}
