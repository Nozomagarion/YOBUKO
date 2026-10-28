// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

impl App {
    pub(in crate::app) fn stop_dj_preview(&mut self) {
        if !self.dj.preview_owned {
            return;
        }
        match self.projects.execute("dj.preview.stop", &json!({})) {
            Ok(_) => {
                self.dj.preview_owned = false;
                self.dj.preview_started = false;
                if !self.dj.snapshot.is_null() {
                    self.dj.snapshot["preview"] = json!({"playing":false,"loading":false,"position":0.0});
                }
            }
            Err(e) => self.status = Status::Error(e.to_string()),
        }
    }
    pub(super) fn prepare_dj_track(&mut self, id: u64) {
        self.stop_dj_preview();
        if self.dj.preview_owned {
            return;
        }
        self.dj.preparing = Some(id);
        self.dj.seek = 0.0;
        self.dj.cue_selected = None;
        self.dj.cue_revision = None;
        self.dj.cue_delete = None;
    }
    fn audition_dj(&mut self, track: u64, from: f64) {
        match self.projects.execute("dj.preview.play", &json!({"track":track,"from":from})) {
            Ok(_) => {
                self.dj.preview_owned = true;
                self.dj.preview_started = false;
                self.dj.snapshot["busy"] = json!(true);
                self.dj.snapshot["error"] = serde_json::Value::Null;
                self.dj.next_poll = None;
            }
            Err(e) => self.dj.snapshot["error"] = json!(e.to_string()),
        }
    }
    fn cue_dj(&mut self, command: &str, params: serde_json::Value) {
        match self.projects.execute(command, &params) {
            Ok(_) => {
                self.dj.cue_queued = true;
                self.dj.snapshot["busy"] = json!(true);
                self.dj.next_poll = None;
                self.dj.cue_delete = None;
            }
            Err(e) => self.dj.snapshot["error"] = json!(e.to_string()),
        }
    }
    pub(super) fn show_dj_prepare(&mut self, ui: &mut Ui, busy: bool) {
        let Some(id) = self.dj.preparing else { return };
        let Some(track) = self.dj.snapshot["tracks"]
            .as_array()
            .and_then(|t| t.iter().find(|t| t["id"].as_u64() == Some(id)))
            .cloned()
        else {
            if !busy {
                self.dj.preparing = None;
                self.stop_dj_preview();
            }
            return;
        };
        let revision = self.dj.snapshot["revision"].as_u64().unwrap_or(0);
        let cues = self.dj.snapshot["cues"]
            .as_array()
            .and_then(|sets| sets.iter().find(|s| s["track"].as_u64() == Some(id)))
            .and_then(|s| s["points"].as_array())
            .cloned()
            .unwrap_or_default();
        if self.dj.cue_revision.is_none() {
            let cue = cues.iter().find(|c| c["id"].as_u64() == self.dj.cue_selected);
            self.dj.cue_name = cue.and_then(|c| c["name"].as_str()).unwrap_or_default().into();
            self.dj.cue_seconds = cue.and_then(|c| c["seconds"].as_f64()).unwrap_or(self.dj.seek);
            if cue.is_none() {
                self.dj.cue_selected = None;
            }
            self.dj.cue_revision = Some(revision);
        }
        if self.dj.cue_delete.is_some_and(|(_, _, r)| r != revision) {
            self.dj.cue_delete = None;
        }
        let end = (track["seconds"].as_f64().unwrap_or(0.001) - 0.001).max(0.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(format!("Prepare · {}", track["title"].as_str().unwrap_or("Track")));
            if ui.button("Close preparation").clicked() {
                self.stop_dj_preview();
                if !self.dj.preview_owned {
                    self.dj.preparing = None;
                }
            }
        });
        let current = self.dj.snapshot["preview"].clone();
        let playing = current["track"].as_u64() == Some(id) && current["playing"] == true;
        ui.horizontal(|ui| {
            let label = ui.label("Start position seconds");
            ui.add(egui::Slider::new(&mut self.dj.seek, 0.0..=end).fixed_decimals(3)).labelled_by(label.id);
            if ui.add_enabled(!busy, egui::Button::new("Play from position")).clicked() {
                self.audition_dj(id, self.dj.seek);
            }
            if ui.button("Stop audition").clicked() {
                self.stop_dj_preview();
            }
            let position = current["position"].as_f64().unwrap_or(0.0);
            ui.label(format!("{} · {position:.3} s", if playing { "Playing" } else { "Stopped" }));
        });
        ui.weak("Preview −6 dB · headphones output when configured, otherwise main output.");
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New cue").clicked() {self.dj.cue_selected=None; self.dj.cue_revision=None; self.dj.cue_delete=None;}
                if ui.add_enabled(playing,egui::Button::new("Use playback position")).clicked() {
                    self.dj.cue_seconds=current["position"].as_f64().unwrap_or(0.0).clamp(0.0,end);
                }
                let label=ui.label("Cue name");
                ui.add(egui::TextEdit::singleline(&mut self.dj.cue_name).char_limit(64).desired_width(150.0)).labelled_by(label.id);
                let label=ui.label("Cue position seconds");
                ui.add(egui::DragValue::new(&mut self.dj.cue_seconds).range(0.0..=end).speed(0.01).max_decimals(3)).labelled_by(label.id);
            });
            let stale=self.dj.cue_revision!=Some(revision);
            if stale {
                ui.horizontal(|ui| {ui.label("Library changed; refresh this cue draft before saving.");
                    if ui.button("Refresh cue draft").clicked() {self.dj.cue_revision=None;self.dj.cue_delete=None;}});
            }
            ui.horizontal(|ui| {
                if let Some(cue)=self.dj.cue_selected {
                    if ui.add_enabled(!stale && !self.dj.cue_name.trim().is_empty(),egui::Button::new("Save cue name")).clicked() {
                        self.cue_dj("dj.cue.rename",json!({"track":id,"revision":self.dj.cue_revision,"cue":cue,"name":self.dj.cue_name}));
                    }
                    if ui.add_enabled(!stale,egui::Button::new("Move cue to position")).clicked() {
                        self.cue_dj("dj.cue.move",json!({"track":id,"revision":self.dj.cue_revision,"cue":cue,"seconds":self.dj.cue_seconds}));
                    }
                    if ui.add_enabled(!stale,egui::Button::new("Delete selected cue…")).clicked() {self.dj.cue_delete=Some((id,cue,revision));}
                } else if ui.add_enabled(!stale && !self.dj.cue_name.trim().is_empty() && cues.len()<16,egui::Button::new("Add cue")).clicked() {
                    self.cue_dj("dj.cue.add",json!({"track":id,"revision":self.dj.cue_revision,"name":self.dj.cue_name,"seconds":self.dj.cue_seconds}));
                }
            });
            if let Some((track,cue,base))=self.dj.cue_delete {
                ui.horizontal(|ui| {
                    ui.label("Delete this cue? The audio stays unchanged.");
                    if ui.button("Confirm delete cue").clicked() {self.cue_dj("dj.cue.delete",json!({"track":track,"cue":cue,"revision":base}));}
                    if ui.button("Cancel cue deletion").clicked() {self.dj.cue_delete=None;}
                });
            }
            egui::ScrollArea::vertical().id_salt("dj-cue-points").max_height(120.0).show(ui,|ui| {
                for cue in &cues {
                    let cue_id=cue["id"].as_u64().unwrap_or(0);
                    let seconds=cue["seconds"].as_f64().unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        ui.label(format!("{seconds:.3} s · {}",cue["name"].as_str().unwrap_or("Cue")));
                        if ui.button(format!("Select cue {cue_id}")).clicked() {self.dj.cue_selected=Some(cue_id);self.dj.cue_revision=None;self.dj.cue_delete=None;self.dj.seek=seconds;}
                        if ui.button(format!("Play cue {cue_id}")).clicked() {self.dj.seek=seconds;self.audition_dj(id,seconds);}
                    });
                }
            });
        });
        ui.separator();
    }
}
