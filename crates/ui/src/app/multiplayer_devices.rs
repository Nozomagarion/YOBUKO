// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Revision-bound drafts using the same generated SDK parameters and widgets as ordinary device panels.
use super::*;
use crate::detail::detail_view::{GraphParameter, parameter_widget, value_text};
use suhina_engine::model::DeviceKind;

#[derive(Default)]
pub(super) struct DeviceEditor {
    pub(super) target: Option<(u32, usize)>,
    pub(super) param: Option<String>,
    pub(super) value: f64,
    base: Option<u64>,
    filter: String,
    selection_required: bool,
    pub(super) chain: super::multiplayer_device_chain::ChainDraft,
}

impl DeviceEditor {
    pub(super) fn chain_changed(&mut self) {
        self.target = None;
        self.param = None;
        self.base = None;
        self.value = 0.0;
        self.filter.clear();
        self.selection_required = true;
    }
}
impl App {
    pub(super) fn show_shared_devices(&mut self, ui: &mut Ui) {
        self.show_shared_device_chain(ui);
        let Some(session) = self.multiplayer.bound.and_then(|id| self.projects.get(id)) else { return };
        let project = session.project();
        let mut targets = Vec::new();
        for track in &project.tracks {
            let Some(channel) = track.channel.and_then(|id| project.channel(id)) else { continue };
            for (index, device) in channel.devices.iter().enumerate().take(16) {
                if let DeviceKind::Builtin(kind) = &device.kind
                    && matches!(kind.as_str(), "fm-synth" | "utility")
                    && device.state.is_none()
                    && let Some(panel) = suhina_device_registry::panel(kind)
                {
                    targets.push((
                        (track.id.0, index),
                        format!("{} · {} · device {}", track.name, device.name, index + 1),
                        device,
                        panel,
                    ));
                }
            }
        }
        if targets.is_empty() {
            ui.weak("No shared devices yet. Choose Device track above and add FM Synth or Utility.");
            return;
        }
        let revision = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
        let editor = &mut self.multiplayer.devices;
        if editor.target.is_none() && !editor.selection_required {
            editor.target = targets.first().map(|r| r.0);
        }
        let old_target = editor.target;
        egui::ComboBox::from_label("Shared device")
            .width(420.0)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(
                targets.iter().find(|r| Some(r.0) == editor.target).map_or("Choose device", |r| r.1.as_str()),
            )
            .show_ui(ui, |ui| {
                for row in &targets {
                    ui.selectable_value(&mut editor.target, Some(row.0), &row.1);
                }
            });
        if old_target != editor.target {
            editor.selection_required = false;
            editor.param = None;
            editor.base = None;
            editor.filter.clear();
        }
        let Some(row) = targets.iter().find(|r| Some(r.0) == editor.target) else {
            ui.weak("The device chain changed. Choose Shared device explicitly before editing; the previous draft was discarded.");
            return;
        };
        let parameters = &row.3.parameters;
        if editor.param.is_none() {
            editor.param = parameters
                .iter()
                .find(|p| matches!(p.id.as_str(), "volume" | "gain"))
                .or_else(|| parameters.first())
                .map(|p| p.id.clone());
        }
        ui.horizontal(|ui| {
            let label = ui.label("Find parameter");
            ui.add(egui::TextEdit::singleline(&mut editor.filter).desired_width(240.0).char_limit(64))
                .labelled_by(label.id);
        });
        let filter = editor.filter.to_ascii_lowercase();
        let old_param = editor.param.clone();
        egui::ComboBox::from_label("Shared parameter")
            .width(300.0)
            .height(240.0)
            .selected_text(
                parameters
                    .iter()
                    .find(|p| Some(&p.id) == editor.param.as_ref())
                    .map_or("Choose parameter", |p| p.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for p in parameters
                    .iter()
                    .filter(|p| format!("{} {} {}", p.name, p.id, p.ui.group).to_ascii_lowercase().contains(&filter))
                {
                    ui.selectable_value(&mut editor.param, Some(p.id.clone()), &p.name);
                }
            });
        let Some(parameter) = parameters.iter().find(|p| Some(&p.id) == editor.param.as_ref()) else { return };
        let confirmed = row.2.params.iter().find(|p| p.id == parameter.id).map_or(parameter.default, |p| p.value);
        if editor.base.is_none() || old_param != editor.param {
            editor.base = Some(revision);
            editor.value = confirmed;
        }
        let graph = GraphParameter::new(parameter, confirmed);
        ui.label(format!("Confirmed value: {}", value_text(&graph, confirmed)));
        if !row.2.enabled {
            ui.weak("This device is off. Values are stored without switching it on.");
        }
        ui.weak("Edit a local draft, then apply it to both copies. Each computer keeps its own playback.");
        let stale = editor.base != Some(revision);
        // Reserve the status row even for a current draft, so a network update cannot move a widget during a click.
        ui.label(if stale {
            "The shared project changed. Refresh this device draft before applying."
        } else {
            "This device draft matches the confirmed revision."
        });
        if ui
            .add_enabled(stale || self.multiplayer.error.is_some(), egui::Button::new("Refresh device draft"))
            .clicked()
        {
            editor.base = Some(revision);
            editor.value = confirmed;
        }
        ui.add_enabled_ui(editor.base == Some(revision) && !self.multiplayer.submitted, |ui| {
            ui.horizontal(|ui| {
                ui.label("Draft value");
                ui.allocate_ui(Vec2::new(260.0, 28.0), |ui| {
                    let rect = ui.available_rect_before_wrap();
                    let response = parameter_widget(ui, &graph, rect, None, &mut editor.value);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, "Parameter value")
                    });
                });
                ui.label(value_text(&graph, editor.value));
            });
            if ui.button("Reset draft to default").clicked() {
                editor.value = parameter.default;
            }
        });
        let ready = editor.base == Some(revision) && !self.multiplayer.submitted;
        let (apply, remove) = ui.horizontal(|ui| {
            let apply = ui.add_enabled(ready, egui::Button::new("Apply shared parameter")).clicked();
            let remove = ui.add_enabled(ready, egui::Button::new("Remove selected device"))
                .on_hover_text("Removes this device, its settings and automation from both shared copies. Notes and clips are kept.")
                .clicked();
            (apply, remove)
        }).inner;
        if apply || remove {
            let edit = if remove {
                suhina_multiplayer::WireEdit::DeviceRemove { track: row.0.0, device: row.0.1 }
            } else {
                suhina_multiplayer::WireEdit::DeviceParam {
                    track: row.0.0,
                    device: row.0.1,
                    param: parameter.id.clone(),
                    value: editor.value,
                }
            };
            let base = editor.base.unwrap_or(revision);
            match serde_json::to_string(&edit) {
                Ok(edit) => self.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit})),
                Err(error) => self.multiplayer.error = Some(error.to_string()),
            }
        }
    }
}
