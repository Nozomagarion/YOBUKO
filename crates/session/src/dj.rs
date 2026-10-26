// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ commands own a separate catalog worker; they never mutate a shared project.

use crate::commands::{optional, param};
use crate::{Agents, CommandError, CommandKind, CommandSpec, ParamType, PathKind, Policy, Projects};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Default)]
pub(crate) struct Dj {
    catalog: suhina_dj::Catalog,
    worker: Option<Sender<(PathBuf, Option<PathBuf>)>>,
    results: Option<Receiver<Result<suhina_dj::Catalog, String>>>,
    busy: bool,
    error: Option<String>,
}

impl Dj {
    fn start(&mut self, path: Option<PathBuf>) -> Result<(), String> {
        if self.results.is_some() {
            return Ok(());
        }
        let path = path.ok_or("the DJ library needs a settings folder")?;
        let (send, jobs) = mpsc::channel::<(PathBuf, Option<PathBuf>)>();
        let (done, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("suhina-dj".into())
            .spawn(move || {
                let mut catalog = match suhina_dj::Catalog::open(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = done.send(Err(e));
                        return;
                    }
                };
                let _ = done.send(Ok(catalog.clone()));
                while let Ok((file, project)) = jobs.recv() {
                    let mut next = catalog.clone();
                    let result = next.import(&file, project).and_then(|_| next.save(&path));
                    let result = result.map(|_| {
                        catalog = next;
                        catalog.clone()
                    });
                    if done.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("could not start the DJ library: {e}"))?;
        self.worker = Some(send);
        self.results = Some(results);
        self.busy = true;
        Ok(())
    }

    fn poll(&mut self) {
        let Some(results) = &self.results else { return };
        loop {
            match results.try_recv() {
                Ok(result) => {
                    self.busy = false;
                    match result {
                        Ok(catalog) => {
                            self.catalog = catalog;
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.error.get_or_insert_with(|| "the DJ worker stopped; restart the app".into());
                    break;
                }
            }
        }
    }
}

const LOCAL: Policy = Policy { agents: Agents::Denied("the person manages their local DJ library"), ..Policy::DEFAULT };
static COMMANDS: &[CommandSpec<Projects>] = &[
    CommandSpec {
        id: "dj.library",
        label: "Read the local DJ library and analysis status",
        params: &[],
        kind: CommandKind::Library,
        run: library,
        policy: LOCAL,
    },
    CommandSpec {
        id: "dj.import",
        label: "Analyse an audio file with rbxport and add it to the local DJ library",
        params: &[
            param("path", ParamType::Path(PathKind::File), "audio file"),
            optional("sourceProject", ParamType::Path(PathKind::File), "saved source project"),
        ],
        kind: CommandKind::Library,
        run: import,
        policy: LOCAL,
    },
];
pub(crate) fn commands() -> &'static [CommandSpec<Projects>] {
    COMMANDS
}

fn ready(projects: &mut Projects) -> Result<(), CommandError> {
    let path = projects.browser.files.data_folder().and_then(|p| p.parent().map(|p| p.join("dj-library.json")));
    projects.dj.start(path).map_err(CommandError::from)?;
    projects.dj.poll();
    Ok(())
}
fn library(projects: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    ready(projects)?;
    Ok(json!({"tracks": projects.dj.catalog.tracks, "busy": projects.dj.busy, "error": projects.dj.error,
        "analysis": "rbxport", "format": suhina_dj::FORMAT}))
}
fn import(projects: &mut Projects, params: &Value) -> Result<Value, CommandError> {
    ready(projects)?;
    if projects.dj.busy {
        return Err("the DJ library is busy; wait for its current job".into());
    }
    let path = params["path"].as_str().filter(|p| !p.is_empty()).ok_or("choose an audio file")?;
    let project = params["sourceProject"].as_str().map(PathBuf::from);
    let worker = projects.dj.worker.as_ref().ok_or("the DJ worker is unavailable")?;
    worker.send((PathBuf::from(path), project)).map_err(|_| CommandError::from("the DJ worker stopped"))?;
    projects.dj.error = None;
    projects.dj.busy = true;
    Ok(json!({"queued": true}))
}
