// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Edit actual native elements, retaining their anchors and command widgets underneath.
use super::App;
use crate::kit::{
    control_layout::{self, Direction, Entry, Placements},
    theme,
};
use egui::{Id, Sense, Vec2};

#[derive(Clone)]
struct Draft {
    theme: String,
    original: Placements,
    controls: Placements,
    selected: String,
}
fn id() -> Id {
    Id::new("interface-edit-draft")
}

impl App {
    pub(super) fn control_editor_theme_changed(&self, ctx: &egui::Context) {
        if ctx.data(|data| data.get_temp::<Draft>(id())).is_some_and(|draft| draft.theme != self.settings.theme) {
            control_layout::draft(ctx, None);
            ctx.data_mut(|data| data.remove::<Draft>(id()));
        }
    }
    pub(super) fn start_control_editor(&self, ctx: &egui::Context) {
        let original =
            self.themes.get(&self.settings.theme).map(|theme| theme.skin.controls.clone()).unwrap_or_default();
        control_layout::draft(ctx, Some(original.clone()));
        ctx.data_mut(|data| {
            data.insert_temp(
                id(),
                Draft {
                    theme: self.settings.theme.clone(),
                    controls: original.clone(),
                    original,
                    selected: String::new(),
                },
            )
        });
        ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
        ctx.request_repaint();
    }

    pub(super) fn control_editor(&mut self, ui: &egui::Ui, entries: &[Entry]) {
        let Some(mut draft) = ui.ctx().data(|data| data.get_temp::<Draft>(id())) else {
            return;
        };
        if draft.theme != self.settings.theme {
            control_layout::draft(ui.ctx(), None);
            ui.ctx().data_mut(|data| data.remove::<Draft>(id()));
            return;
        }
        let mut done = false;
        let mut cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
        let area = ui.max_rect();
        ui.ctx().set_sublayer(
            egui::LayerId::new(egui::Order::Foreground, Id::new("interface-edit-overlay")),
            egui::LayerId::new(egui::Order::Foreground, Id::new("interface-edit-tools")),
        );
        egui::Area::new(Id::new("interface-edit-overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min)
            .movable(false)
            .show(ui.ctx(), |overlay| {
                overlay.set_min_size(area.size());
                overlay.set_clip_rect(area);
                // Consume input across the workspace, including the gaps between controls.
                overlay.interact(area, Id::new("interface-edit-shield"), Sense::click_and_drag());
                let ink = theme::accent();
                for x in (0..=(area.width().min(16384.0) / 8.0) as usize).map(|n| area.left() + n as f32 * 8.0) {
                    overlay.painter().vline(x, area.y_range(), egui::Stroke::new(0.5, ink.gamma_multiply(0.12)));
                }
                for y in (0..=(area.height().min(16384.0) / 8.0) as usize).map(|n| area.top() + n as f32 * 8.0) {
                    overlay.painter().hline(area.x_range(), y, egui::Stroke::new(0.5, ink.gamma_multiply(0.12)));
                }
                for (index, entry) in entries.iter().enumerate() {
                    let response = overlay.interact(
                        entry.at.intersect(area),
                        Id::new(("move-interface", &entry.name, index)),
                        Sense::click_and_drag(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("Move {} instance {}", entry.name, index + 1),
                        )
                    });
                    let selected = draft.selected == entry.name;
                    overlay.painter().rect_stroke(
                        entry.at,
                        1.0,
                        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, ink),
                        egui::StrokeKind::Inside,
                    );
                    if response.clicked() || response.drag_started() {
                        draft.selected = entry.name.clone();
                    }
                    if response.drag_started() {
                        overlay.ctx().data_mut(|data| data.insert_temp(response.id.with("origin"), entry.at.min));
                    }
                    if response.dragged()
                        && (draft.controls.contains_key(&entry.name)
                            || draft.controls.len() < control_layout::MAX_CONTROLS)
                    {
                        let origin = overlay
                            .ctx()
                            .data(|data| data.get_temp::<egui::Pos2>(response.id.with("origin")))
                            .unwrap_or(entry.at.min);
                        let desired = origin + response.total_drag_delta().unwrap_or_default();
                        let snapped = egui::pos2((desired.x / 8.0).round() * 8.0, (desired.y / 8.0).round() * 8.0);
                        let layout = draft.controls.entry(entry.name.clone()).or_default();
                        layout.bottom = None;
                        layout.x = (snapped.x - entry.native.left()).round().clamp(-16384.0, 16384.0) as i32;
                        layout.y = (snapped.y - entry.native.top()).round().clamp(-16384.0, 16384.0) as i32;
                        overlay.ctx().request_repaint();
                    }
                    response.on_hover_text(format!("{} · drag to move · 8 point grid", entry.name));
                }
            });
        egui::Window::new("Edit interface")
            .id(Id::new("interface-edit-tools"))
            .order(egui::Order::Foreground)
            .default_pos(area.min + Vec2::new(16.0, 110.0))
            .resizable(false)
            .collapsible(false)
            .show(ui.ctx(), |tools| {
                tools.label("Move outlined elements · 8 point snap grid");
                tools.label("Matching elements share the theme layout.");
                let names: std::collections::BTreeSet<_> =
                    entries.iter().map(|entry| entry.name.clone()).chain(draft.controls.keys().cloned()).collect();
                egui::ComboBox::from_id_salt("interface-element")
                    .selected_text(if draft.selected.is_empty() { "Select an element" } else { &draft.selected })
                    .show_ui(tools, |tools| {
                        for name in names {
                            tools.selectable_value(&mut draft.selected, name.clone(), &name);
                        }
                    });
                let can_edit =
                    draft.controls.contains_key(&draft.selected) || draft.controls.len() < control_layout::MAX_CONTROLS;
                if !can_edit {
                    tools.label("The theme already overrides 256 elements. Reset an override to add another.");
                }
                if !draft.selected.is_empty() && can_edit {
                    let mut layout = draft.controls.get(&draft.selected).copied().unwrap_or_default();
                    let before = layout;
                    tools.horizontal(|tools| {
                        tools.label("Offset X");
                        tools.add(egui::DragValue::new(&mut layout.x).range(-16384..=16384));
                        tools.label("Y");
                        tools.add(egui::DragValue::new(&mut layout.y).range(-16384..=16384));
                    });
                    tools.horizontal(|tools| {
                        for (name, dimension) in [("Width", &mut layout.width), ("Height", &mut layout.height)] {
                            let mut custom = dimension.is_some();
                            if tools.checkbox(&mut custom, name).changed() {
                                *dimension = custom.then_some(20);
                            }
                            if let Some(value) = dimension {
                                tools.add(egui::DragValue::new(value).range(1..=4096));
                            }
                        }
                    });
                    tools.horizontal(|tools| {
                        let mut bottom = layout.bottom.is_some();
                        if tools.checkbox(&mut bottom, "Anchor to bottom").changed() {
                            layout.bottom = bottom.then_some(0);
                        }
                        if let Some(value) = &mut layout.bottom {
                            tools.add(egui::DragValue::new(value).range(-16384..=16384));
                        }
                        tools.add_enabled(
                            !draft.selected.starts_with("toolbar-"),
                            egui::Checkbox::new(&mut layout.hidden, "Hidden"),
                        );
                    });
                    if matches!(draft.selected.as_str(), "mixer-volume" | "arrangement-volume") {
                        tools.horizontal(|tools| {
                            tools.selectable_value(&mut layout.direction, Direction::Vertical, "Vertical");
                            tools.selectable_value(&mut layout.direction, Direction::Horizontal, "Horizontal");
                        });
                    }
                    if layout != before {
                        draft.controls.insert(draft.selected.clone(), layout);
                    }
                    if tools.button("Reset selected element").clicked() {
                        draft.controls.remove(&draft.selected);
                    }
                }
                tools.horizontal(|tools| {
                    done = tools.button("Done").clicked();
                    cancel |= tools.button("Cancel").clicked();
                });
            });
        if done || cancel {
            control_layout::draft(ui.ctx(), None);
            ui.ctx().data_mut(|data| data.remove::<Draft>(id()));
            if done && draft.controls != draft.original {
                let mut skin =
                    self.themes.get(&self.settings.theme).map(|theme| theme.skin.clone()).unwrap_or_default();
                // Merge only edited entries into the latest live theme.
                let keys: std::collections::BTreeSet<_> = draft.original.keys().chain(draft.controls.keys()).collect();
                for key in keys {
                    if draft.controls.get(key) == draft.original.get(key) {
                        continue;
                    }
                    if let Some(value) = draft.controls.get(key) {
                        skin.controls.insert(key.clone(), *value);
                    } else {
                        skin.controls.remove(key);
                    }
                }
                self.save_toolbar_skin(skin);
            }
        } else {
            control_layout::draft(ui.ctx(), Some(draft.controls.clone()));
            ui.ctx().data_mut(|data| data.insert_temp(id(), draft));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn window() -> Harness<'static, App> {
        let mut app = App::new();
        app.set_session(suhina_session::Session::new(crate::demo::demo_project()), &[]);
        let mut harness =
            Harness::builder().with_size((1280.0, 900.0)).build_ui_state(|ui, app: &mut App| app.ui(ui), app);
        theme::install(&harness.ctx);
        harness.run_steps(3);
        harness
    }

    fn start(harness: &mut Harness<'static, App>) {
        harness.state().start_control_editor(&harness.ctx);
        harness.run_steps(4);
    }

    #[test]
    fn noop_and_cancel_keep_the_theme_and_native_layout() {
        let mut harness = window();
        let original = harness.state().settings.theme.clone();
        let original_rect = harness.get_by_label_contains("Drums level:").rect();
        start(&mut harness);
        harness.get_by_label("Done").click();
        harness.run_steps(3);
        assert_eq!(harness.state().settings.theme, original);
        assert_eq!(harness.get_by_label_contains("Drums level:").rect(), original_rect);
        start(&mut harness);
        let at = harness.query_all_by_label_contains("Move mixer-meter instance").next().unwrap().rect().center();
        harness.hover_at(at);
        harness.drag_at(at);
        harness.run_steps(1);
        harness.hover_at(at + Vec2::new(24.0, 0.0));
        harness.run_steps(2);
        harness.drop_at(at + Vec2::new(24.0, 0.0));
        harness.run_steps(2);
        assert_ne!(harness.get_by_label_contains("Drums level:").rect(), original_rect);
        assert!(harness.state().session().is_some_and(|session| !session.can_undo()));
        harness.get_by_label("Cancel").click();
        harness.run_steps(3);
        assert_eq!(harness.state().settings.theme, original);
        assert_eq!(harness.get_by_label_contains("Drums level:").rect(), original_rect);
    }

    #[test]
    fn project_shortcuts_do_not_delete_tracks_while_placing_elements() {
        let mut harness = window();
        harness.state_mut().view.selected = Some(suhina_engine::model::TrackId(1));
        harness.state_mut().view.track_focus = true;
        let tracks = harness.state().session().unwrap().project().tracks.len();
        start(&mut harness);
        harness.key_press(egui::Key::Delete);
        harness.run_steps(3);
        assert_eq!(harness.state().session().unwrap().project().tracks.len(), tracks);
        assert!(!harness.state().session().unwrap().can_undo());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(harness.query_by_label("Done").is_none());
    }

    #[test]
    fn done_saves_the_snapped_rectangle_and_theme_switch_discards_the_draft() {
        let mut harness = window();
        start(&mut harness);
        let at = harness.query_all_by_label_contains("Move mixer-meter instance").next().unwrap().rect().center();
        harness.hover_at(at);
        harness.drag_at(at);
        harness.run_steps(1);
        harness.hover_at(at + Vec2::new(24.0, 0.0));
        harness.run_steps(2);
        harness.drop_at(at + Vec2::new(24.0, 0.0));
        harness.run_steps(2);
        let preview = harness.get_by_label_contains("Drums level:").rect();
        assert!((preview.left() / 8.0).fract().abs() < 0.001);
        harness.get_by_label("Done").click();
        harness.run_steps(3);
        assert_eq!(harness.get_by_label_contains("Drums level:").rect(), preview);
        assert!(
            harness
                .state()
                .themes
                .get(&harness.state().settings.theme)
                .unwrap()
                .skin
                .controls
                .contains_key("mixer-meter")
        );
        start(&mut harness);
        harness.state_mut().settings.theme = theme::BUILT_IN.into();
        harness.run_steps(3);
        assert!(harness.query_by_label("Done").is_none());
        assert_eq!(harness.state().settings.theme, theme::BUILT_IN);
    }
}
