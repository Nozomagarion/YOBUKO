// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ commands own a separate catalog worker; they never mutate a shared project.

use crate::commands::{optional, param};
use crate::{Agents, CommandError, CommandKind, CommandSpec, ParamType, PathKind, Policy, Projects};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use suhina_dj::{Catalog, PlaylistEdit};

#[derive(Debug)]
enum Job {
    Import(PathBuf, Option<PathBuf>),
    Playlist(u64, PlaylistEdit),
}
struct Reply {
    catalog: Catalog,
    created: Option<u32>,
}

#[derive(Default)]
pub(crate) struct Dj {
    catalog: Catalog,
    worker: Option<SyncSender<Job>>,
    results: Option<Receiver<Result<Reply, String>>>,
    busy: bool,
    error: Option<String>,
    created: Option<u32>,
}

impl Dj {
    fn start(&mut self, path: Option<PathBuf>) -> Result<(), String> {
        if self.results.is_some() {
            return Ok(());
        }
        let path = path.ok_or("the DJ library needs a settings folder")?;
        let (send, jobs) = mpsc::sync_channel::<Job>(1);
        let (done, results) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("suhina-dj".into())
            .spawn(move || {
                let mut catalog = match Catalog::open(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = done.send(Err(e));
                        return;
                    }
                };
                if done.send(Ok(Reply { catalog: catalog.clone(), created: None })).is_err() {
                    return;
                }
                while let Ok(job) = jobs.recv() {
                    let mut next = catalog.clone();
                    let result = match job {
                        Job::Import(file, project) => next.import(&file, project).map(|_| None),
                        Job::Playlist(revision, edit) => next.edit_playlist(revision, edit),
                    }
                    .and_then(|created| {
                        next.save_checked(&catalog, &path)?;
                        Ok(created)
                    })
                    .map(|created| {
                        catalog = next;
                        Reply { catalog: catalog.clone(), created }
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
                        Ok(reply) => {
                            self.catalog = reply.catalog;
                            self.created = reply.created;
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.error.get_or_insert_with(|| "the DJ worker stopped; use Refresh DJ library".into());
                    break;
                }
            }
        }
    }

    fn queue(&mut self, job: Job) -> Result<Value, CommandError> {
        if self.busy {
            return Err("the DJ library is busy; wait for its current job".into());
        }
        self.worker
            .as_ref()
            .ok_or("the DJ worker is unavailable")?
            .try_send(job)
            .map_err(|_| CommandError::from("the DJ worker is unavailable; use Refresh DJ library"))?;
        self.error = None;
        self.created = None;
        self.busy = true;
        Ok(json!({"queued": true}))
    }
}

const LOCAL: Policy = Policy { agents: Agents::Denied("the person manages their local DJ library"), ..Policy::DEFAULT };
macro_rules! command {
    ($id:literal, $label:literal, $params:expr, $run:ident) => {
        CommandSpec { id: $id, label: $label, params: $params, kind: CommandKind::Library, run: $run, policy: LOCAL }
    };
}
const REVISION: crate::commands::Param = param("revision", ParamType::Integer, "displayed DJ catalog revision");
const PLAYLIST: crate::commands::Param = param("playlist", ParamType::Integer, "local DJ playlist id");
const TRACK: crate::commands::Param = param("track", ParamType::Integer, "local DJ catalog track id");
const NAME: crate::commands::Param = param("name", ParamType::Text, "playlist name");
static COMMANDS: &[CommandSpec<Projects>] = &[
    command!("dj.library", "Read the local DJ library and analysis status", &[], library),
    command!("dj.reload", "Refresh DJ library from disk", &[], reload),
    command!(
        "dj.import",
        "Analyse an audio file with rbxport and add it to the local DJ library",
        &[
            param("path", ParamType::Path(PathKind::File), "audio file"),
            optional("sourceProject", ParamType::Path(PathKind::File), "saved source project"),
        ],
        import
    ),
    command!("dj.playlist.create", "Create a local DJ playlist", &[NAME, REVISION], create),
    command!("dj.playlist.rename", "Rename a local DJ playlist", &[PLAYLIST, NAME, REVISION], rename),
    command!("dj.playlist.delete", "Delete a local DJ playlist without deleting audio", &[PLAYLIST, REVISION], delete),
    command!("dj.playlist.addTrack", "Add a library track to a DJ playlist", &[PLAYLIST, TRACK, REVISION], add),
    command!(
        "dj.playlist.removeTrack",
        "Remove a track from a DJ playlist without deleting audio",
        &[PLAYLIST, TRACK, REVISION],
        remove
    ),
    command!(
        "dj.playlist.moveTrack",
        "Move a track to a final one-based playlist position",
        &[PLAYLIST, TRACK, param("position", ParamType::Integer, "final one-based position"), REVISION,],
        move_track
    ),
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
    Ok(json!({"tracks": projects.dj.catalog.tracks, "playlists": projects.dj.catalog.playlists,
        "revision": projects.dj.catalog.revision, "created_playlist": projects.dj.created.take(),
        "busy": projects.dj.busy, "error": projects.dj.error, "analysis": "rbxport", "format": suhina_dj::FORMAT}))
}
fn reload(projects: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    ready(projects)?;
    if projects.dj.busy {
        return Err("the DJ library is busy; wait before refreshing".into());
    }
    projects.dj = Dj::default();
    ready(projects)?;
    Ok(json!({"queued": true}))
}
fn import(projects: &mut Projects, params: &Value) -> Result<Value, CommandError> {
    ready(projects)?;
    let path = params["path"].as_str().filter(|p| !p.is_empty()).ok_or("choose an audio file")?;
    let project = params["sourceProject"].as_str().map(PathBuf::from);
    projects.dj.queue(Job::Import(PathBuf::from(path), project))
}
fn number(params: &Value, name: &str) -> Result<u64, CommandError> {
    params[name].as_u64().ok_or_else(|| CommandError::from(format!("{name} must be an unsigned integer")))
}
fn id(params: &Value, name: &str) -> Result<u32, CommandError> {
    u32::try_from(number(params, name)?).ok().filter(|n| *n > 0).ok_or_else(|| "invalid DJ id".into())
}
fn name(params: &Value) -> Result<String, CommandError> {
    let name = params["name"].as_str().ok_or("playlist name is required")?;
    if name.len() > 128 || name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err("choose a playlist name of 1–128 bytes without control characters".into());
    }
    Ok(name.trim().into())
}
fn edit(projects: &mut Projects, params: &Value, edit: PlaylistEdit) -> Result<Value, CommandError> {
    ready(projects)?;
    projects.dj.queue(Job::Playlist(number(params, "revision")?, edit))
}
fn create(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    edit(p, v, PlaylistEdit::Create(name(v)?))
}
fn rename(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    edit(p, v, PlaylistEdit::Rename { playlist: id(v, "playlist")?, name: name(v)? })
}
fn delete(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    edit(p, v, PlaylistEdit::Delete(id(v, "playlist")?))
}
fn add(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    edit(p, v, PlaylistEdit::Add { playlist: id(v, "playlist")?, track: id(v, "track")? })
}
fn remove(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    edit(p, v, PlaylistEdit::Remove { playlist: id(v, "playlist")?, track: id(v, "track")? })
}
fn move_track(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    let position = usize::try_from(number(v, "position")?).map_err(|_| CommandError::from("invalid DJ position"))?;
    edit(p, v, PlaylistEdit::Move { playlist: id(v, "playlist")?, track: id(v, "track")?, position })
}
