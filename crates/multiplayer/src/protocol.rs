// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use suhina_collaboration::Edit;
use suhina_model::Project;

pub const PROTOCOL: u16 = 15;
pub const COPY_FORMAT: u16 = 3;

pub(crate) fn engine_revision() -> Result<String, String> {
    let pins: Value = serde_json::from_str(include_str!("../../../fork-sources.json"))
        .map_err(|_| "invalid pinned source identity")?;
    let revision = pins["suhina"]["commit"]
        .as_str()
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .ok_or("invalid pinned Suhina identity")?;
    Ok(revision.to_owned())
}

pub const MAX_FRAME: usize = 65536;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrackControl {
    pub track: u32,
    pub name: String,
    pub db: f64,
    pub pan: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub revision: u64,
    pub tracks: Vec<TrackControl>,
}

impl Controls {
    pub fn validate(&self) -> Result<(), String> {
        if self.tracks.is_empty() || self.tracks.len() > crate::midi::MAX_TRACKS {
            return Err("invalid shared track count".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for t in &self.tracks {
            if t.track == 0
                || !ids.insert(t.track)
                || t.name.trim().is_empty()
                || t.name.len() > 256
                || t.name.chars().any(char::is_control)
                || !t.db.is_finite()
                || !(-150.0..=6.0).contains(&t.db)
                || !t.pan.is_finite()
                || !(-1.0..=1.0).contains(&t.pan)
            {
                return Err("invalid shared track controls".into());
            }
        }
        Ok(())
    }
    pub fn from_project(project: &Project, revision: u64) -> Result<Self, String> {
        let mut tracks = Vec::new();
        for track in &project.tracks {
            let id = track.id.0;
            let channel = track
                .channel
                .and_then(|id| project.channels.iter().find(|c| c.id == id))
                .ok_or("starter channel missing")?;
            let db = if channel.volume > 0.0 { 20.0 * channel.volume.log10() } else { -150.0 };
            tracks.push(TrackControl {
                track: id,
                name: track.name.clone(),
                db: db.clamp(-150.0, 6.0),
                pan: channel.pan,
            });
        }
        let value = Self { revision, tracks };
        value.validate()?;
        Ok(value)
    }
    pub fn changes(&self, project: &Project) -> Result<Vec<(&'static str, Value)>, String> {
        self.validate()?;
        let current = Self::from_project(project, self.revision)?;
        if self.tracks.len() != current.tracks.len()
            || self.tracks.iter().zip(&current.tracks).any(|(a, b)| a.track != b.track)
        {
            return Err("shared track identities do not match the MIDI copy".into());
        }
        let mut changes = Vec::new();
        for (t, old) in self.tracks.iter().zip(current.tracks.iter()) {
            if t.name != old.name {
                changes.push(("track.rename", json!({"track": t.track,"name":t.name})));
            }
            if (t.db - old.db).abs() > 0.00001 {
                changes.push(("track.setVolume", json!({"tracks":[t.track],"db":t.db})));
            }
            if (t.pan - old.pan).abs() > 0.00001 {
                changes.push(("track.setPan", json!({"tracks":[t.track],"pan":t.pan})));
            }
        }
        Ok(changes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "command", deny_unknown_fields)]
pub enum WireEdit {
    #[serde(rename = "track.createShared")]
    TrackCreate { kind: String, name: String },
    #[serde(rename = "track.rename")]
    Rename { track: u32, name: String },
    #[serde(rename = "track.setVolume")]
    Volume { track: u32, db: f64 },
    #[serde(rename = "track.setPan")]
    Pan { track: u32, pan: f64 },
    #[serde(rename = "device.addShared")]
    DeviceAdd { track: u32, kind: String },
    #[serde(rename = "device.removeShared")]
    DeviceRemove { track: u32, device: usize },
    #[serde(rename = "device.setSharedParam")]
    DeviceParam { track: u32, device: usize, param: String, value: f64 },
    #[serde(rename = "note.add")]
    NoteAdd { clip: u32, time: f64, length: f64, key: u8, velocity: f64 },
    #[serde(rename = "note.update")]
    NoteUpdate { clip: u32, note: usize, time: f64, length: f64, key: u8, velocity: f64, muted: bool },
    #[serde(rename = "note.delete")]
    NoteDelete { clip: u32, note: usize },
    #[serde(rename = "clip.insertAudio")]
    AudioInsert { track: u32, start: f64, file: String, rate: u32, channels: u16, frames: usize },
    #[serde(rename = "scene.insert")]
    SceneInsert { scene: usize, name: String },
    #[serde(rename = "scene.rename")]
    SceneRename { scene: usize, name: String },
    #[serde(rename = "scene.duplicate")]
    SceneDuplicate { scene: usize },
    #[serde(rename = "scene.delete")]
    SceneDelete { scene: usize },
    #[serde(rename = "clip.createSharedSlot")]
    SlotCreate { track: u32, scene: usize, length: f64 },
    #[serde(rename = "clip.copyToSharedSlot")]
    SlotCopy { source: u32, track: u32, scene: usize },
    #[serde(rename = "clip.moveSlot")]
    SlotMove { track: u32, scene: usize, to_track: u32, to_scene: usize },
    #[serde(rename = "clip.create")]
    ClipCreate { track: u32, start: f64, length: f64 },
    #[serde(rename = "clip.move")]
    ClipMove { clip: u32, start: f64, track: u32 },
    #[serde(rename = "clip.trim")]
    ClipTrim { clip: u32, start: f64, end: f64 },
    #[serde(rename = "clip.duplicate")]
    ClipDuplicate { clip: u32 },
    #[serde(rename = "clip.delete")]
    ClipDelete { clip: u32 },
    #[serde(rename = "clip.setEnabled")]
    ClipEnabled { clip: u32, enabled: bool },
}

impl WireEdit {
    pub fn edit(&self) -> Result<Edit, String> {
        if matches!(self,Self::Rename{name,..} if name.chars().any(char::is_control)) {
            return Err("shared track names cannot contain control characters".into());
        }
        let (id, params) = self.command();
        Edit::new(id, params).map_err(|e| e.to_string())
    }
    pub fn command(&self) -> (&'static str, Value) {
        match self {
            Self::TrackCreate { kind, name } => ("track.createShared", json!({"kind":kind,"name":name})),
            Self::Rename { track, name } => ("track.rename", json!({"track":track,"name":name})),
            Self::Volume { track, db } => ("track.setVolume", json!({"tracks":[track],"db":db})),
            Self::Pan { track, pan } => ("track.setPan", json!({"tracks":[track],"pan":pan})),
            Self::DeviceAdd { track, kind } => ("device.addShared", json!({"track":track,"kind":kind})),
            Self::DeviceRemove { track, device } => ("device.removeShared", json!({"track":track,"device":device})),
            Self::DeviceParam { track, device, param, value } => {
                ("device.setSharedParam", json!({"track":track,"device":device,"param":param,"value":value}))
            }
            Self::NoteAdd { clip, time, length, key, velocity } => (
                "note.add",
                json!({"clip":clip,
                "notes":[{"time":time,"length":length,"key":key,"velocity":velocity}]}),
            ),
            Self::NoteUpdate { clip, note, time, length, key, velocity, muted } => (
                "note.update",
                json!({"clip":clip,"note":note,"time":time,"length":length,"key":key,"velocity":velocity,"muted":muted}),
            ),
            Self::NoteDelete { clip, note } => ("note.delete", json!({"clip":clip,"notes":[note]})),
            Self::AudioInsert { track, start, file, rate, channels, frames } => (
                "clip.insertAudio",
                json!({"track":track,"start":start,"file":file,"rate":rate,"channels":channels,"frames":frames}),
            ),
            Self::SceneInsert { scene, name } => ("scene.insert", json!({"scene":scene,"name":name})),
            Self::SceneRename { scene, name } => ("scene.rename", json!({"scene":scene,"name":name})),
            Self::SceneDuplicate { scene } => ("scene.duplicate", json!({"scene":scene})),
            Self::SceneDelete { scene } => ("scene.delete", json!({"scene":scene})),
            Self::SlotCreate { track, scene, length } => {
                ("clip.createSharedSlot", json!({"track":track,"scene":scene,"length":length}))
            }
            Self::SlotCopy { source, track, scene } => {
                ("clip.copyToSharedSlot", json!({"source":source,"track":track,"scene":scene}))
            }
            Self::SlotMove { track, scene, to_track, to_scene } => {
                ("clip.moveSlot", json!({"track":track,"scene":scene,"toTrack":to_track,"toScene":to_scene}))
            }
            Self::ClipCreate { track, start, length } => {
                ("clip.create", json!({"track":track,"start":start,"length":length,"stack":true}))
            }
            Self::ClipMove { clip, start, track } => {
                ("clip.move", json!({"clip":clip,"start":start,"toTrack":track,"stack":true}))
            }
            Self::ClipTrim { clip, start, end } => {
                ("clip.trim", json!({"clip":clip,"start":start,"end":end,"stack":true}))
            }
            Self::ClipDuplicate { clip } => ("clip.duplicate", json!({"clips":[{"clip":clip}],"stack":true})),
            Self::ClipDelete { clip } => ("clip.delete", json!({"clip":clip})),
            Self::ClipEnabled { clip, enabled } => {
                ("clip.setEnabled", json!({"clips":[{"clip":clip}],"enabled":enabled}))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub revision: u64,
    pub edit: WireEdit,
}

/// Verify contiguous history against a detached session before the UI sees any operation.
pub fn apply_operations(
    session: &mut suhina_session::Session,
    revision: &mut u64,
    controls: &Controls,
    operations: &[Operation],
) -> Result<(), String> {
    if operations.len() > 32 {
        return Err("too many shared operations".into());
    }
    for operation in operations {
        if revision.checked_add(1) != Some(operation.revision) {
            return Err("shared operation revision gap".into());
        }
        operation.edit.edit()?;
        let (id, params) = operation.edit.command();
        session.execute(id, &params).map_err(|e| e.to_string())?;
        crate::midi::encode_portable(session.project())?;
        *revision = operation.revision;
    }
    if controls.revision != *revision || !controls.changes(session.project())?.is_empty() {
        return Err("shared operations do not match the host snapshot".into());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ClientMessage {
    Hello {
        protocol: u16,
        format: u16,
        engine: String,
        secret: String,
    },
    /// An admitted guest coming back after its link dropped, with the token the host gave it.
    /// `seen_revision` is the last revision it applied completely; `None` asks for a fresh copy.
    Resume {
        protocol: u16,
        format: u16,
        engine: String,
        token: String,
        seen_revision: Option<u64>,
    },
    /// The guest leaves on purpose: the host frees its place at once.
    Leave {
        protocol: u16,
    },
    Download {
        protocol: u16,
        offset: usize,
    },
    AssetDownload {
        protocol: u16,
        asset: usize,
        offset: usize,
    },
    Publish {
        protocol: u16,
        sequence: u64,
        base_revision: u64,
        seen_revision: u64,
        track: u32,
        start: f64,
        asset: crate::assets::Asset,
    },
    UploadChunk {
        protocol: u16,
        offset: usize,
        data: String,
    },
    Poll {
        protocol: u16,
        seen_revision: u64,
    },
    Edit {
        protocol: u16,
        sequence: u64,
        base_revision: u64,
        seen_revision: u64,
        edit: WireEdit,
    },
    /// Linked playback: a host-ordered change to the shared transport.
    Transport {
        protocol: u16,
        seen_revision: u64,
        request: crate::linked::TransportRequest,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ServerMessage {
    /// The host refuses this hello and says why.
    Denied {
        refusal: Refusal,
    },
    /// The hello was accepted. `token` lets this guest resume after a dropped link.
    Admitted {
        protocol: u16,
        token: String,
    },
    /// A resume was accepted: `token` replaces the old one. With `copy` a fresh copy follows,
    /// otherwise a state carrying the operations since the guest's revision.
    Resumed {
        protocol: u16,
        token: String,
        copy: bool,
    },
    /// The host ended the session on purpose.
    Ended {
        protocol: u16,
    },
    UploadRequest {
        protocol: u16,
        offset: usize,
    },
    Project {
        protocol: u16,
        format: u16,
        revision: u64,
        bytes: usize,
        sha256: String,
    },
    Chunk {
        protocol: u16,
        offset: usize,
        data: String,
    },
    Assets {
        protocol: u16,
        manifest: Vec<crate::assets::Asset>,
    },
    AssetChunk {
        protocol: u16,
        asset: usize,
        offset: usize,
        data: String,
    },
    State {
        protocol: u16,
        guests: usize,
        controls: Box<Controls>,
        operations: Vec<Operation>,
        accepted: Option<bool>,
        error: Option<String>,
        /// The host clock when this state was made: milliseconds since the session started.
        host_ms: u64,
        /// The shared transport while playback is linked.
        transport: Option<crate::linked::SharedTransport>,
    },
}

/// Why a host refused a hello. Told to the guest so it can show what to do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub enum Refusal {
    /// A different Suhina build: engine revision or copy format.
    Build { host: String, guest: String },
    /// A different protocol number.
    Protocol { host: u16, guest: u16 },
    /// The hello could not be read.
    Unsupported,
    /// The code is not this session's current code (never redeemed, replaced or mistyped).
    Invalid,
    /// The right code, used after its five minutes.
    Expired,
    /// The right code, already used by another guest.
    Used,
    /// The right code, while three guests already hold a place.
    Full,
    /// The resume token is unknown, replaced or held past its grace period.
    Resume,
}
impl Refusal {
    /// The sentence the guest shows. Text from the host is reduced to a short build identifier.
    pub fn message(&self) -> String {
        match self {
            Self::Build { host, guest } => format!(
                "Your partner runs a different Suhina build (host {}, this computer {}). Install the same version on both computers.",
                build_label(host),
                build_label(guest)
            ),
            Self::Protocol { host, guest } => format!(
                "Your partner runs a different Suhina version (protocol {host} vs {guest}). Install the same version on both computers."
            ),
            Self::Unsupported => {
                "The host could not read this computer's request. Install the same Suhina version on both computers."
                    .into()
            }
            Self::Invalid => {
                "This invitation code is not valid for this session: it was replaced by a newer one or mistyped. Ask the host for a new code."
                    .into()
            }
            Self::Expired => {
                "This invitation code has expired (codes last five minutes). Ask the host for a new code.".into()
            }
            Self::Used => {
                "This invitation code was already used by another guest. Each code works once; ask the host for a new code."
                    .into()
            }
            Self::Full => "The session already has three guests. Wait for a guest to leave, then ask for a new code.".into(),
            Self::Resume => {
                "The connection was lost and the host no longer holds your place. Ask the host for a new invitation code to rejoin."
                    .into()
            }
        }
    }
}
/// A short, printable build identifier from text a peer sent: at most 7 letters or digits, then
/// an optional copy format.
fn build_label(text: &str) -> String {
    let (revision, format) = text.rsplit_once('/').unwrap_or((text, ""));
    let short: String = revision.chars().filter(char::is_ascii_alphanumeric).take(7).collect();
    let short = if short.is_empty() { "unknown".to_owned() } else { short };
    let format: String = format.chars().filter(char::is_ascii_digit).take(5).collect();
    if format.is_empty() { short } else { format!("{short} copy format {format}") }
}
/// How a build names itself in a refusal: its engine revision, and its copy format when asked.
pub(crate) fn build_identity(engine: &str, format: Option<u16>) -> String {
    let short: String = engine.chars().filter(char::is_ascii_alphanumeric).take(7).collect();
    match format {
        Some(format) => format!("{short}/{format}"),
        None => short,
    }
}

/// Checks an invitation's protocol before its exact shape, so a code from another version says so.
pub(crate) fn invitation_protocol(json: &[u8]) -> Result<(), String> {
    let value: Value = serde_json::from_slice(json).map_err(|_| "invalid invitation")?;
    match value.get("protocol").and_then(Value::as_u64) {
        Some(n) if n == u64::from(PROTOCOL) => Ok(()),
        Some(n) => Err(format!(
            "This invitation comes from a different Suhina version (protocol {n}, this computer uses {PROTOCOL}). Install the same version on both computers."
        )),
        None => Err("invalid invitation".into()),
    }
}

/// Reads the host's answer to a hello. A refusal is read leniently, so a newer host's reason
/// still yields a sentence the person can act on.
pub(crate) fn read_answer(io: &mut impl Read) -> Result<ServerMessage, String> {
    let value: Value = read(io)?;
    if value.get("kind").and_then(Value::as_str) == Some("Denied") {
        let refusal = value.get("refusal").cloned().and_then(|r| serde_json::from_value::<Refusal>(r).ok());
        return Ok(ServerMessage::Denied { refusal: refusal.unwrap_or(Refusal::Unsupported) });
    }
    serde_json::from_value(value).map_err(|_| "invalid network message".into())
}

pub(crate) fn validate_guest_count(guests: usize) -> Result<(), String> {
    if !(1..=crate::transport::MAX_GUESTS).contains(&guests) {
        return Err("invalid admitted guest count".into());
    }
    Ok(())
}

pub fn read<T: for<'de> Deserialize<'de>>(io: &mut impl Read) -> Result<T, String> {
    let mut prefix = [0; 4];
    io.read_exact(&mut prefix).map_err(|e| format!("connection closed or timed out: {e}"))?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err("invalid network frame length".into());
    }
    let mut body = vec![0; size];
    io.read_exact(&mut body).map_err(|e| format!("incomplete network frame: {e}"))?;
    serde_json::from_slice(&body).map_err(|_| "invalid network message".into())
}
pub fn write<T: Serialize>(io: &mut impl Write, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "could not encode network message")?;
    if bytes.len() > MAX_FRAME {
        return Err("network message is too large".into());
    }
    io.write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| io.write_all(&bytes))
        .and_then(|_| io.flush())
        .map_err(|e| format!("connection write failed: {e}"))
}
