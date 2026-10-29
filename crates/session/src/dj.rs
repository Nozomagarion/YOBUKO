// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ commands own a separate catalog worker; they never mutate a shared project.

use crate::commands::{optional, param};
use crate::{Agents, CommandError, CommandKind, CommandSpec, ParamType, PathKind, Policy, Projects};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use suhina_dj::{Catalog, CueEdit, PlaylistEdit};

#[derive(Debug)]
pub(crate) enum Job {
    Import(PathBuf, Option<PathBuf>),
    Playlist(u64, PlaylistEdit),
    Cue(u64, u32, CueEdit),
    Grid(u64, u32, Option<(f64, f64)>),
    Preview(u64, u32),
    Waveform(u64, u32),
    Deck(usize, u64, u32),
}
struct Reply {
    catalog: Catalog,
    created: Option<u32>,
    audio: Option<crate::dj_preview::Loaded>,
    waveform: Option<crate::dj_waveform::Loaded>,
    deck: Option<crate::dj_decks::Loaded>,
}

#[derive(Default)]
pub(crate) struct Dj {
    pub(crate) catalog: Catalog,
    pub(crate) audition: crate::dj_preview::Audition,
    pub(crate) decks: crate::dj_decks::Decks,
    pub(crate) waveform: crate::dj_waveform::Waveform,
    worker: Option<SyncSender<Job>>,
    results: Option<Receiver<Result<Reply, String>>>,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
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
                if done
                    .send(Ok(Reply {
                        deck: None,
                        catalog: catalog.clone(),
                        created: None,
                        waveform: None,
                        audio: None,
                    }))
                    .is_err()
                {
                    return;
                }
                let mut cache = None;
                while let Ok(job) = jobs.recv() {
                    if let Job::Deck(i, token, track) = job {
                        let data = catalog
                            .tracks
                            .iter()
                            .find(|t| t.id == track)
                            .ok_or_else(|| "unknown DJ track".to_string())
                            .and_then(|t| {
                                crate::dj_preview::prepare_limited(t, &mut cache, crate::dj_decks::MAX_SAMPLES)
                            })
                            .map_err(|e| format!("Deck loading (128 MiB decoded maximum): {e}"));
                        if done
                            .send(Ok(Reply {
                                catalog: catalog.clone(),
                                created: None,
                                audio: None,
                                waveform: None,
                                deck: Some((i, token, data)),
                            }))
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                    if let Job::Waveform(token, track) = job {
                        let data = catalog
                            .tracks
                            .iter()
                            .find(|t| t.id == track)
                            .ok_or_else(|| "unknown DJ track".to_string())
                            .and_then(|t| crate::dj_preview::prepare(t, &mut cache))
                            .map(|data| std::sync::Arc::new(suhina_media::waveform::peaks(&data).without_samples()));
                        if done
                            .send(Ok(Reply {
                                deck: None,
                                catalog: catalog.clone(),
                                created: None,
                                audio: None,
                                waveform: Some((token, data)),
                            }))
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                    if let Job::Preview(token, track) = job {
                        let data = catalog
                            .tracks
                            .iter()
                            .find(|t| t.id == track)
                            .ok_or_else(|| "unknown DJ track".to_string())
                            .and_then(|t| crate::dj_preview::prepare(t, &mut cache));
                        if done
                            .send(Ok(Reply {
                                deck: None,
                                catalog: catalog.clone(),
                                created: None,
                                waveform: None,
                                audio: Some((token, data)),
                            }))
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                    let mut next = catalog.clone();
                    let result = match job {
                        Job::Import(file, project) => next.import(&file, project).map(|_| None),
                        Job::Playlist(revision, edit) => next.edit_playlist(revision, edit),
                        Job::Cue(revision, track, edit) => next.edit_cue(revision, track, edit).map(|_| None),
                        Job::Grid(revision, track, value) => next.set_grid(revision, track, value).map(|_| None),
                        Job::Preview(..) | Job::Waveform(..) | Job::Deck(..) => Err("invalid DJ worker job".into()),
                    }
                    .and_then(|created| {
                        next.save_checked(&catalog, &path)?;
                        Ok(created)
                    })
                    .map(|created| {
                        catalog = next;
                        Reply { deck: None, catalog: catalog.clone(), created, waveform: None, audio: None }
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
                            self.audition.loaded = reply.audio;
                            if let Some(deck) = reply.deck {
                                self.decks.apply(deck);
                            }
                            if let Some(waveform) = reply.waveform {
                                self.waveform.apply(waveform);
                            }
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

    pub(crate) fn queue(&mut self, job: Job) -> Result<Value, CommandError> {
        if self.busy {
            return Err("the DJ library is busy; wait for its current job".into());
        }
        let importing = matches!(job, Job::Import(..));
        self.worker
            .as_ref()
            .ok_or("the DJ worker is unavailable")?
            .try_send(job)
            .map_err(|_| CommandError::from("the DJ worker is unavailable; use Refresh DJ library"))?;
        if importing {
            self.waveform.clear();
        }
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
const DECK: crate::commands::Param = param("deck", ParamType::Text, "deck A or B");
static COMMANDS: &[CommandSpec<Projects>] = &[
    command!("dj.deck.load", "Load a local track into a stopped DJ deck", &[DECK, TRACK], deck_load),
    command!("dj.deck.play", "Play or resume a DJ deck", &[DECK], deck_play),
    command!("dj.deck.pause", "Pause a DJ deck", &[DECK], deck_pause),
    command!("dj.deck.stop", "Stop and rewind a DJ deck", &[DECK], deck_stop),
    command!("dj.deck.clear", "Stop and unload a DJ deck", &[DECK], deck_clear),
    command!(
        "dj.deck.seek",
        "Seek a DJ deck in source seconds",
        &[DECK, param("from", ParamType::Number, "source seconds")],
        deck_seek
    ),
    command!("dj.deck.stopAll", "Stop decks and cancel pending loads", &[], deck_stop_all),
    command!(
        "dj.mixer.set",
        "Set the DJ crossfader, channel levels and headphone cues",
        &[
            param("crossfade", ParamType::Number, "0=A, 1=B"),
            param("levelA", ParamType::Number, "A level 0–1"),
            param("levelB", ParamType::Number, "B level 0–1"),
            param("cueA", ParamType::Bool, "A headphone cue"),
            param("cueB", ParamType::Bool, "B headphone cue")
        ],
        deck_mixer
    ),
    command!(
        "dj.grid.set",
        "Save the manual constant DJ beat grid",
        &[
            TRACK,
            REVISION,
            param("bpm", ParamType::Number, "manual tempo in BPM"),
            param("firstBeat", ParamType::Number, "seconds of beat 1 of bar 1")
        ],
        grid_set
    ),
    command!(
        "dj.grid.clear",
        "Delete the saved DJ beat grid without changing audio or cues",
        &[TRACK, REVISION],
        grid_clear
    ),
    command!("dj.waveform.load", "Prepare a DJ waveform without playing audio", &[TRACK], waveform_load),
    command!("dj.waveform.clear", "Cancel and discard the prepared DJ waveform", &[], waveform_clear),
    command!(
        "dj.preview.play",
        "Audition a DJ track from seconds",
        &[TRACK, param("from", ParamType::Number, "start position in seconds")],
        preview_play
    ),
    command!("dj.preview.stop", "Stop or cancel DJ audition", &[], preview_stop),
    command!(
        "dj.cue.add",
        "Add a named DJ cue point",
        &[TRACK, REVISION, NAME, param("seconds", ParamType::Number, "cue position in seconds")],
        cue_add
    ),
    command!(
        "dj.cue.rename",
        "Rename a DJ cue point",
        &[TRACK, REVISION, NAME, param("cue", ParamType::Integer, "local cue id")],
        cue_rename
    ),
    command!(
        "dj.cue.move",
        "Move a DJ cue point",
        &[
            TRACK,
            REVISION,
            param("cue", ParamType::Integer, "local cue id"),
            param("seconds", ParamType::Number, "cue position in seconds")
        ],
        cue_move
    ),
    command!(
        "dj.cue.delete",
        "Delete a DJ cue point without changing audio",
        &[TRACK, REVISION, param("cue", ParamType::Integer, "local cue id")],
        cue_delete
    ),
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

pub(crate) fn ready(projects: &mut Projects) -> Result<(), CommandError> {
    let path = projects.browser.files.data_folder().and_then(|p| p.parent().map(|p| p.join("dj-library.json")));
    projects.dj.start(path).map_err(CommandError::from)?;
    projects.dj.poll();
    crate::dj_preview::sync(projects);
    crate::dj_decks::sync(projects);
    Ok(())
}
fn library(projects: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    ready(projects)?;
    Ok(json!({"tracks": projects.dj.catalog.tracks, "playlists": projects.dj.catalog.playlists,
        "performance":crate::dj_decks::snapshot(projects), "grids":projects.dj.catalog.grids, "waveform":projects.dj.waveform.snapshot(),
        "cues": projects.dj.catalog.cues, "preview": crate::dj_preview::snapshot(projects),
        "revision": projects.dj.catalog.revision, "created_playlist": projects.dj.created.take(),
        "busy": projects.dj.busy, "error": projects.dj.error, "analysis": "rbxport", "format": suhina_dj::FORMAT}))
}
fn reload(projects: &mut Projects, _: &Value) -> Result<Value, CommandError> {
    ready(projects)?;
    if projects.dj.busy {
        return Err("the DJ library is busy; wait before refreshing".into());
    }
    crate::dj_preview::stop(projects, &json!({}))?;
    crate::dj_decks::stop_all(projects, &json!({}))?;
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
pub(crate) fn id(params: &Value, name: &str) -> Result<u32, CommandError> {
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

fn preview_play(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_preview::play(p, v)
}
fn preview_stop(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_preview::stop(p, v)
}
fn seconds(v: &Value) -> Result<f64, CommandError> {
    v["seconds"]
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .ok_or_else(|| "cue seconds must be finite and nonnegative".into())
}
fn cue_edit(p: &mut Projects, v: &Value, edit: CueEdit) -> Result<Value, CommandError> {
    ready(p)?;
    p.dj.queue(Job::Cue(number(v, "revision")?, id(v, "track")?, edit))
}
fn cue_add(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    cue_edit(p, v, CueEdit::Add { name: name(v)?, seconds: seconds(v)? })
}
fn cue_rename(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    cue_edit(p, v, CueEdit::Rename { cue: id(v, "cue")?, name: name(v)? })
}
fn cue_move(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    cue_edit(p, v, CueEdit::Move { cue: id(v, "cue")?, seconds: seconds(v)? })
}
fn cue_delete(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    cue_edit(p, v, CueEdit::Delete(id(v, "cue")?))
}

fn waveform_load(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_waveform::load(p, v)
}
fn waveform_clear(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_waveform::clear(p, v)
}

fn grid_set(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    ready(p)?;
    let bpm =
        v["bpm"].as_f64().filter(|n| n.is_finite() && (20.0..=400.0).contains(n)).ok_or("grid BPM must be 20–400")?;
    let first = v["firstBeat"]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or("grid first beat must be nonnegative seconds")?;
    p.dj.queue(Job::Grid(number(v, "revision")?, id(v, "track")?, Some((bpm, first))))
}
fn grid_clear(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    ready(p)?;
    p.dj.queue(Job::Grid(number(v, "revision")?, id(v, "track")?, None))
}

fn deck_load(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::load(p, v)
}
fn deck_play(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::play(p, v)
}
fn deck_pause(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::pause(p, v)
}
fn deck_stop(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::stop(p, v)
}
fn deck_clear(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::clear(p, v)
}
fn deck_seek(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::seek(p, v)
}
fn deck_stop_all(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::stop_all(p, v)
}
fn deck_mixer(p: &mut Projects, v: &Value) -> Result<Value, CommandError> {
    crate::dj_decks::mixer(p, v)
}
