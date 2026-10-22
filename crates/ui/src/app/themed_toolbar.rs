// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reorderable native toolbar modules. Normal controls retain their command paths and ids.
use super::*;
use crate::kit::skin::{self, State, Toolbar};
use crate::settings::settings_view::ThemeRequest;

fn edit_id() -> Id {
    Id::new("edit-theme-toolbar")
}

impl App {
    /// Returns true when the custom layout has rendered the bar.
    pub(super) fn themed_toolbar(
        &mut self,
        ui: &mut Ui,
        grid: Option<&Grid>,
        cpu: Option<u32>,
        ram: Option<u64>,
        actions: &mut Vec<Action>,
    ) -> bool {
        let rect = ui.max_rect();
        let response = ui.interact(rect, Id::new("toolbar-context"), Sense::click());
        let mut editing = ui.ctx().data(|d| d.get_temp::<bool>(edit_id())).unwrap_or(false);
        let mut theme_skin = self.themes.get(&self.settings.theme).map(|t| t.skin.clone()).unwrap_or_default();
        let mut changed = false;
        response.context_menu(|ui| {
            if ui.button(if editing { "Finish editing toolbar" } else { "Edit toolbar" }).clicked() {
                editing = !editing;
                if theme_skin.toolbar.is_none() {
                    theme_skin.toolbar = Some(Toolbar::default());
                    changed = true;
                }
                ui.close();
            }
            if ui.button("Restore default toolbar").clicked() {
                theme_skin.toolbar = None;
                changed = true;
                editing = false;
                ui.close();
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(edit_id(), editing));
        if changed {
            self.save_toolbar_skin(theme_skin.clone());
        }
        let Some(mut toolbar) =
            theme_skin.toolbar.clone().or_else(|| theme_skin.css.as_ref().map(|_| Toolbar::default()))
        else {
            return false;
        };
        if !editing {
            let modules: Vec<_> = toolbar
                .order
                .iter()
                .filter(|id| !toolbar.hidden.contains(id))
                .map(|id| (id.clone(), self.toolbar_module_width(id)))
                .collect();
            if let Some(modules) = crate::kit::native_css::toolbar(&modules, rect.size()) {
                let mut hovered_switch = None;
                for module in modules {
                    let at = module.rect.translate(rect.min.to_vec2());
                    if !at.intersects(ui.clip_rect()) {
                        continue;
                    }
                    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(at));
                    painter.rect_filled(
                        at,
                        module.style.radius,
                        module.style.background.gamma_multiply(module.style.opacity),
                    );
                    if let Some(grid) = grid {
                        let mut module_ui =
                            ui.new_child(egui::UiBuilder::new().id_salt(("css-toolbar", &module.name)).max_rect(at));
                        module_ui.set_clip_rect(ui.clip_rect().intersect(at));
                        self.toolbar_module(
                            &mut module_ui,
                            at,
                            &module.name,
                            grid,
                            cpu,
                            ram,
                            actions,
                            &mut hovered_switch,
                        );
                    }
                }
                self.hover_switch(ui.ctx(), hovered_switch);
                return true;
            }
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
        let mut move_before = None;
        let mut hidden_change = None;
        let mut hovered_switch = None;
        egui::ScrollArea::horizontal().id_salt("custom-toolbar-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_min_height(rect.height());
            ui.horizontal(|ui| {
                for module in &toolbar.order {
                    let hidden = toolbar.hidden.contains(module);
                    if hidden && !editing {
                        continue;
                    }
                    let width = if editing {
                        (module.len() as f32 * 8.0 + 20.0).max(70.0)
                    } else {
                        self.toolbar_module_width(module)
                    };
                    let (_, at) = ui.allocate_space(Vec2::new(width, (rect.height() - 4.0).max(24.0)));
                    if editing {
                        let response = ui.interact(at, Id::new(("move-toolbar", module)), Sense::click_and_drag());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Move toolbar {module}"))
                        });
                        response.dnd_set_drag_payload(module.clone());
                        let over = response.dnd_hover_payload::<String>().is_some();
                        let color = if hidden { theme::text_hint() } else { theme::text_primary() };
                        ui.painter().rect_filled(
                            at,
                            3.0,
                            if over { theme::control_active() } else { theme::control_background() },
                        );
                        widgets::text_in(
                            ui.painter(),
                            at,
                            Align2::CENTER_CENTER,
                            module,
                            theme::ui_font(theme::TEXT_SMALL),
                            color,
                        );
                        if let Some(from) = response.dnd_release_payload::<String>() {
                            move_before = Some(((*from).clone(), module.clone()));
                        }
                        response.context_menu(|ui| {
                            if ui.button(if hidden { "Show module" } else { "Hide module" }).clicked() {
                                hidden_change = Some((module.clone(), !hidden));
                                ui.close();
                            }
                        });
                    } else if let Some(grid) = grid {
                        self.toolbar_module(ui, at, module, grid, cpu, ram, actions, &mut hovered_switch);
                    }
                }
                if editing && ui.button("Done").clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(edit_id(), false));
                }
            });
        });
        let before = toolbar.clone();
        if let Some((from, to)) = move_before {
            toolbar.move_before(&from, &to);
        }
        if let Some((module, hide)) = hidden_change {
            toolbar.hidden.retain(|s| *s != module);
            if hide {
                toolbar.hidden.push(module);
            }
        }
        if toolbar != before {
            theme_skin.toolbar = Some(toolbar);
            self.save_toolbar_skin(theme_skin);
        }
        self.hover_switch(ui.ctx(), hovered_switch);
        true
    }

    fn toolbar_module_width(&self, module: &str) -> f32 {
        match module {
            "browser" | "metronome" => 28.0,
            "tempo" => 62.0,
            "signature" => 45.0,
            "grid" | "count-in" => 70.0,
            "transport" => 86.0,
            "position" => 78.0,
            "mapping" => 64.0,
            "performance" => 145.0,
            "views" => 172.0,
            "agents" => (self.agents.len() as f32 * 140.0).max(1.0),
            _ => 0.0,
        }
    }

    fn save_toolbar_skin(&mut self, skin: skin::Skin) {
        if let Some(id) = self.themes.request(&self.settings.theme, ThemeRequest::EditSkin(skin)) {
            self.settings.theme = id;
            self.settings_changed = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn toolbar_module(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        module: &str,
        grid: &Grid,
        cpu: Option<u32>,
        ram: Option<u64>,
        actions: &mut Vec<Action>,
        hovered_switch: &mut Option<View>,
    ) {
        let painter = ui.painter_at(rect);
        let middle = |index: f32, width: f32| {
            Rect::from_min_size(egui::pos2(rect.left() + index, rect.top()), Vec2::new(width, rect.height()))
        };
        match module {
            "browser" => {
                if widgets::button(
                    ui,
                    &painter,
                    rect,
                    "☰",
                    if self.settings.browser_open { "Hide browser" } else { "Show browser" },
                ) {
                    self.toggle_browser();
                }
            }
            "tempo" => {
                ui.ctx().data_mut(|d| d.insert_temp(Id::new("sampler-tempo"), grid.tempo_bpm));
                let shown = self.tempo_drag.unwrap_or(grid.tempo_bpm);
                let response = widgets::value_area(ui, rect, Id::new("tempo"), "Tempo", format!("{shown:.2}"));
                if let Drag::Done(bpm) = widgets::drag_value(
                    ui,
                    &response,
                    &mut self.tempo_drag,
                    grid.tempo_bpm,
                    0.1,
                    1.0,
                    (20.0, 999.0),
                    120.0,
                ) {
                    actions.push(Action::new("transport.setTempo", json!({"bpm":(bpm*100.0).round()/100.0})));
                }
                widgets::text_in(
                    &painter,
                    rect,
                    Align2::CENTER_CENTER,
                    &format!("{shown:.2}"),
                    theme::mono_font(theme::TEXT),
                    theme::text_value(),
                );
            }
            "signature" => {
                let ts = grid.time_signature();
                widgets::text_in(
                    &painter,
                    rect,
                    Align2::CENTER_CENTER,
                    &format!("{} / {}", ts.numerator, ts.denominator),
                    theme::mono_font(theme::TEXT),
                    theme::text_secondary(),
                );
            }
            "grid" => {
                let current = self.session().map_or(DEFAULT_QUANTIZATION, |s| quantization_label(s.quantization()));
                let mut chosen = current;
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    let response = widgets::combo_box(
                        ui,
                        egui::ComboBox::from_id_salt("quantization")
                            .width(rect.width())
                            .height(QUANTIZATION.len() as f32 * 2.0 * theme::ROW)
                            .selected_text(chosen),
                        |ui| {
                            for q in QUANTIZATION {
                                ui.selectable_value(&mut chosen, q, q);
                            }
                        },
                    )
                    .response;
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, QUANTIZATION_LABEL)
                    });
                });
                if chosen != current {
                    actions.push(Action::new("launcher.setQuantization", json!({"value":chosen})));
                }
            }
            "count-in" => crate::session::record_controls::count_in_menu(
                ui,
                rect,
                self.session().map_or(0, |s| s.count_in_bars()),
                actions,
            ),
            "metronome" => {
                crate::session::record_controls::metronome_button(ui, &painter, rect, grid.metronome, actions)
            }
            "transport" => {
                for (index, role, label, on) in [(0, "play", "Play", grid.running), (1, "stop", "Stop", false)] {
                    let at = middle(index as f32 * 29.0, 28.0);
                    let response = widgets::button_area(ui, at, Id::new(role), label);
                    if response.clicked() {
                        actions.push(if index == 0 { self.play_action(false) } else { self.stop_action() });
                    }
                    let state = State::response(&response, on);
                    let ink = if on { theme::accent() } else { theme::text_primary() };
                    if !skin::paint(&painter, at, "button", state, theme::control_background(), 0.0) {
                        painter.rect_filled(
                            at,
                            0.0,
                            if on { theme::control_active() } else { theme::control_background() },
                        );
                    }
                    if !skin::paint(&painter, at, role, state, ink, 0.0) {
                        if index == 0 {
                            widgets::play_glyph(&painter, at.center(), false, ink, 10.0);
                        } else {
                            painter.rect_filled(Rect::from_center_size(at.center(), Vec2::splat(8.0)), 0.0, ink);
                        }
                    }
                }
                crate::session::record_controls::record_button(
                    ui,
                    &painter,
                    middle(58.0, 28.0),
                    self.session().is_some_and(|s| s.records_arrangement()),
                    actions,
                );
            }
            "position" => widgets::text_in(
                &painter,
                rect,
                Align2::CENTER_CENTER,
                &grid.position(),
                theme::mono_font(theme::TEXT),
                theme::text_primary(),
            ),
            "mapping" => {
                if widgets::button(ui, &painter, rect, "Map keys", "Map keys to slots and scenes") {
                    self.toggle_mapping_mode();
                }
            }
            "performance" => {
                let text = format!(
                    "CPU {}  RAM {}",
                    cpu.map_or_else(|| "—".into(), |n| format!("{:.0}%", f64::from(n.min(1000)) / 10.0)),
                    ram.map_or_else(|| "—".into(), ram_text)
                );
                widgets::text_in(
                    &painter,
                    rect,
                    Align2::CENTER_CENTER,
                    &text,
                    theme::mono_font(theme::TEXT_SMALL),
                    theme::text_secondary(),
                );
                let response = ui.interact(rect, Id::new("cpu-ram"), Sense::hover());
                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &text));
            }
            "views" => {
                let carrying = self.carry.is_some() || self.arrangement.dragged_clip().is_some();
                for (index, view, name) in [(0, View::Session, "Session"), (1, View::Arrangement, "Arrangement")] {
                    let at = middle(index as f32 * 86.0, 86.0);
                    if widgets::button(ui, &painter, at, name, &format!("{name} view")) {
                        self.shown = view;
                    }
                    if self.shown == view {
                        painter.hline(at.x_range(), at.bottom() - 1.0, theme::hairline(theme::accent()));
                    }
                    if carrying && self.shown != view && ui.ctx().pointer_latest_pos().is_some_and(|p| at.contains(p)) {
                        *hovered_switch = Some(view);
                    }
                }
            }
            "agents" => {
                for (index, (connection, client)) in self.agents.clone().into_iter().enumerate() {
                    let label = format!("Disconnect {client}");
                    if widgets::button(ui, &painter, middle(index as f32 * 140.0, 140.0), &label, &label) {
                        self.agent_disconnects.push(connection);
                        self.status = Status::Done(format!("Disconnected {client}"));
                    }
                }
            }
            _ => {}
        }
    }
}
