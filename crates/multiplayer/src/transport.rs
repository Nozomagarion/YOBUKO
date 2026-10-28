// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::assets::{self, AudioAddition, SharedSnapshot};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, PoisonError,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use suhina_collaboration::{Host, PeerId, Request, Role};
use suhina_engine::AudioPool;
use suhina_model::Project;
use suhina_session::{Agents, CommandError, CommandKind, CommandSpec, ParamType, Policy};

use crate::invitation::{Invitation, new_secret, private, secret_hash};
use crate::protocol::{self, ClientMessage, Controls, Operation, PROTOCOL, ServerMessage, WireEdit};

pub const MAX_GUESTS: usize = 3;

const TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);

struct Canonical {
    host: Host,
    journal: VecDeque<Operation>,
    secret: [u8; 32],
    expires: Instant,
    redeemed: bool,
    guests: BTreeSet<PeerId>,
    assets: Vec<Arc<assets::Encoded>>,
    pending_audio: Vec<AudioAddition>,
}
impl Canonical {
    fn since(&self, seen: u64) -> Result<Vec<Operation>, String> {
        let revision = self.host.revision();
        if seen > revision || revision - seen > self.journal.len() as u64 {
            return Err("shared history is no longer available; rejoin with a new invitation".into());
        }
        Ok(self.journal.iter().filter(|op| op.revision > seen).cloned().collect())
    }
    fn submit(
        &mut self,
        author: suhina_collaboration::PeerId,
        sequence: u64,
        base: u64,
        wire: WireEdit,
    ) -> Result<(), String> {
        if matches!(wire, WireEdit::AudioInsert { .. }) && author != Host::owner() {
            return Err("only the host can publish a sound".into());
        }
        self.apply(author, sequence, base, wire)
    }
    fn check(
        &self,
        author: suhina_collaboration::PeerId,
        sequence: u64,
        base: u64,
        wire: &WireEdit,
    ) -> Result<bool, String> {
        let edit = wire.edit()?;
        if self
            .host
            .preflight(author, &Request { sequence, base_revision: base, edit })
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Ok(true);
        }
        let mut candidate = suhina_session::Session::new(self.host.project().clone());
        let (id, params) = wire.command();
        candidate.execute(id, &params).map_err(|e| e.to_string())?;
        crate::midi::encode_portable(candidate.project())?;
        Ok(false)
    }
    fn apply(
        &mut self,
        author: suhina_collaboration::PeerId,
        sequence: u64,
        base: u64,
        wire: WireEdit,
    ) -> Result<(), String> {
        self.check(author, sequence, base, &wire)?;
        let edit = wire.edit()?;
        let before = self.host.revision();
        self.host.submit(author, Request { sequence, base_revision: base, edit }).map_err(|e| e.to_string())?;
        if self.host.revision() != before {
            if self.journal.len() == 32 {
                self.journal.pop_front();
            }
            self.journal.push_back(Operation { revision: self.host.revision(), edit: wire });
        }
        Ok(())
    }
    fn check_take(
        &self,
        author: suhina_collaboration::PeerId,
        sequence: u64,
        base: u64,
        target: (u32, f64),
        meta: &assets::Asset,
    ) -> Result<bool, String> {
        assets::validate_asset(meta)?;
        let cached = self.check(author, sequence, base, &take_edit(target, meta))?;
        if let Some(old) = self.assets.iter().find(|a| a.meta.hash == meta.hash) {
            if old.meta != *meta {
                return Err("known sound metadata changed".into());
            }
        } else if self.pending_audio.len() >= 16
            || self.assets.len() >= 64
            || self.assets.iter().map(|a| a.meta.bytes).sum::<usize>() + meta.bytes > assets::MAX_TOTAL
        {
            return Err("session audio budget reached or view is busy".into());
        }
        Ok(cached)
    }
    fn commit_take(
        &mut self,
        author: suhina_collaboration::PeerId,
        sequence: u64,
        base: u64,
        target: (u32, f64),
        asset: Arc<assets::Encoded>,
        audio: Option<AudioAddition>,
    ) -> Result<(), String> {
        self.check_take(author, sequence, base, target, &asset.meta)?;
        let exists = self.assets.iter().any(|a| a.meta.hash == asset.meta.hash);
        if !exists && audio.is_none() {
            return Err("verified take collection missing".into());
        }
        self.apply(author, sequence, base, take_edit(target, &asset.meta))?;
        if !exists {
            self.assets.push(asset);
            if let Some(audio) = audio {
                self.pending_audio.push(audio);
            }
        }
        Ok(())
    }
    fn emit(&mut self, events: &SyncSender<Event>, last: &mut (u64, usize)) -> Result<(), String> {
        let current = (self.host.revision(), self.guests.len());
        if current != *last {
            let controls = Controls::from_project(self.host.project(), current.0)?;
            let operations = self.since(last.0)?;
            let audio = std::mem::take(&mut self.pending_audio);
            match events.try_send(Event::State(controls, operations, current.1, audio)) {
                Ok(()) => *last = current,
                Err(mpsc::TrySendError::Full(Event::State(_, _, _, audio))) => self.pending_audio = audio,
                Err(_) => return Err("multiplayer view closed".into()),
            }
        }
        Ok(())
    }
    fn response(&self, seen: u64, outcome: Option<Result<(), String>>) -> Result<ServerMessage, String> {
        let operations = self.since(seen)?;
        let accepted = outcome.as_ref().map(Result::is_ok);
        let error = outcome.and_then(Result::err);
        Ok(ServerMessage::State {
            protocol: PROTOCOL,
            guests: self.guests.len(),
            controls: Box::new(Controls::from_project(self.host.project(), self.host.revision())?),
            operations,
            accepted,
            error,
        })
    }
    fn renew(&mut self, invitation: &Invitation, now: Instant, events: &SyncSender<Event>) -> Result<(), String> {
        if self.guests.len() >= MAX_GUESTS {
            return Err("the session already has three guests; wait for a guest to leave".into());
        }
        let secret = secret_hash(&invitation.secret);
        if bool::from(self.secret.ct_eq(&secret)) {
            return Err("could not create a fresh invitation".into());
        }
        let expires = now.checked_add(Duration::from_secs(300)).ok_or("invitation clock exhausted")?;
        // Publish under the admission lock before revoking: backpressure must leave the old token usable.
        events.try_send(Event::Invitation(invitation.encode()?)).map_err(|_| "invitation view is busy; try again")?;
        self.secret = secret;
        self.expires = expires;
        self.redeemed = false;
        Ok(())
    }
    fn admit(&mut self, secret: &str, now: Instant) -> Result<suhina_collaboration::PeerId, String> {
        if secret.len() != 64
            || !bool::from(self.secret.ct_eq(&secret_hash(secret)))
            || self.redeemed
            || self.guests.len() >= MAX_GUESTS
            || now >= self.expires
        {
            return Err("admission refused".into());
        }
        let peer = self.host.add_peer(Role::Editor).map_err(|e| e.to_string())?;
        self.redeemed = true;
        self.guests.insert(peer);
        Ok(peer)
    }
}
enum Event {
    Ready(String, Controls, Box<SharedSnapshot>),
    Connected(Controls, Box<SharedSnapshot>, usize),
    Invitation(String),
    Receiving(usize, usize),
    Upload(Option<(usize, usize)>),
    State(Controls, Vec<Operation>, usize, Vec<AudioAddition>),
    Rejected(String),
    Failed(String),
    Closed,
}
enum Input {
    RenewInvitation,
    Edit { base: u64, edit: WireEdit },
    Publish { base: u64, track: u32, start: f64, audio: Arc<suhina_engine::AudioData> },
}

struct Running {
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<TcpStream>>>,
    inputs: SyncSender<Input>,
    events: Receiver<Event>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for socket in self.sockets.lock().unwrap_or_else(PoisonError::into_inner).iter() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

/// UI-owned service. Its commands only queue bounded control work; never call on an audio thread.
#[derive(Default)]
pub struct Service {
    running: Option<Running>,
    mode: String,
    controls: Option<Controls>,
    invitation: Option<String>,
    error: Option<String>,
    guest_count: usize,
    selected: Option<(Project, AudioPool)>,
    project: Option<SharedSnapshot>,
    operations: Vec<Operation>,
    receiving: Option<(usize, usize)>,
    storage: Option<PathBuf>,
    selected_take: Option<Arc<suhina_engine::AudioData>>,
    received_audio: Vec<AudioAddition>,
    sending: Option<(usize, usize)>,
}

impl Service {
    /// Trusted local selection only; remote inputs must use the bounded MIDI decoder instead.
    pub fn select_project(&mut self, project: &Project) -> Result<(), CommandError> {
        if self.active() {
            return Err("leave the current session first".into());
        }
        self.selected = None;
        crate::midi::validate(project).map_err(CommandError::from)?;
        self.selected = Some((project.clone(), AudioPool::new()));
        Ok(())
    }
    /// Explicit local selection of decoded audio; never follows a filename supplied by a peer.
    pub fn select_audio_project(&mut self, project: &Project, audio: &AudioPool) -> Result<(), CommandError> {
        if self.active() {
            return Err("leave the current session first".into());
        }
        self.selected = None;
        crate::midi::validate_audio_source(project).map_err(CommandError::from)?;
        let mut wanted = AudioPool::new();
        for content in &project.contents {
            for lane in &content.lanes {
                if let suhina_model::LaneData::Audio(region) = &lane.data {
                    if region.file.is_empty() {
                        continue;
                    }
                    if wanted.files.len() >= 64 && !wanted.files.contains_key(&region.file) {
                        return Err("at most 64 source sounds can be shared".into());
                    }
                    let data = audio
                        .files
                        .get(&region.file)
                        .ok_or("a project sound is not loaded; load or relink it in Studio first")?;
                    wanted.files.insert(region.file.clone(), Arc::clone(data));
                }
            }
        }
        self.selected = Some((project.clone(), wanted));
        Ok(())
    }
    /// Trusted local destination only. A peer can never choose this path.
    pub fn set_storage_root(&mut self, root: Option<PathBuf>) -> Result<(), CommandError> {
        if self.active() {
            return Err("leave the current session first".into());
        }
        if root.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err("shared project storage must be absolute".into());
        }
        self.storage = root;
        Ok(())
    }
    /// Trusted desktop selection of a finished loaded take; never a path or network upload.
    pub fn select_take(&mut self, audio: Arc<suhina_engine::AudioData>) -> Result<(), CommandError> {
        if !self.active() || !matches!(self.mode.as_str(), "hosting" | "connected") {
            return Err("connect before selecting a loaded take".into());
        }
        if self.sending.is_some() {
            return Err("a take is already being published".into());
        }
        if audio.samples.len() > (assets::MAX_ASSET - 128) / 4 {
            return Err("the take exceeds 64 MiB".into());
        }
        self.selected_take = Some(audio);
        Ok(())
    }
    pub fn take_audio(&mut self) -> Vec<AudioAddition> {
        std::mem::take(&mut self.received_audio)
    }
    pub fn take_shared(&mut self) -> Option<SharedSnapshot> {
        self.project.take()
    }
    /// MIDI-only compatibility helper; audio callers should use take_shared and its verified pool.
    pub fn take_project(&mut self) -> Option<Project> {
        self.take_snapshot().map(|(project, _)| project)
    }
    pub fn take_snapshot(&mut self) -> Option<(Project, u64)> {
        let snapshot = self.project.take()?;
        let project = snapshot.project.clone();
        let revision = snapshot.revision;
        snapshot.keep();
        Some((project, revision))
    }
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value, CommandError> {
        let spec = COMMANDS.iter().find(|c| c.id == id).ok_or("unknown multiplayer command")?;
        // The session registry's schema checker is private; enforce the same exact-key shape here.
        let object = params.as_object().ok_or("multiplayer parameters must be an object")?;
        if object.len() != spec.params.len() || spec.params.iter().any(|p| !object.contains_key(p.name)) {
            return Err("unexpected multiplayer parameters".into());
        }
        (spec.run)(self, params)
    }
    pub fn poll(&mut self) {
        let mut closed = false;
        if let Some(running) = &self.running {
            loop {
                match running.events.try_recv() {
                    Ok(Event::Ready(invitation, controls, project)) => {
                        self.project = Some(*project);
                        self.operations.clear();
                        self.receiving = None;
                        self.mode = "hosting".into();
                        self.invitation = Some(invitation);
                        self.controls = Some(controls);
                    }
                    Ok(Event::Connected(controls, project, guests)) => {
                        self.project = Some(*project);
                        self.operations.clear();
                        self.receiving = None;
                        self.mode = "connected".into();
                        self.guest_count = guests;
                        self.controls = Some(controls);
                    }
                    Ok(Event::Invitation(invitation)) => self.invitation = Some(invitation),
                    Ok(Event::Upload(progress)) => self.sending = progress,
                    Ok(Event::Receiving(done, total)) => self.receiving = Some((done, total)),
                    Ok(Event::State(controls, operations, guests, audio)) => {
                        self.receiving = None;
                        if self.operations.len() + operations.len() > 128
                            || self.received_audio.len() + audio.len() > 64
                        {
                            self.error =
                                Some("shared editor is not keeping up; the last local copy was preserved".into());
                            self.mode = "error".into();
                            closed = true;
                            break;
                        }
                        self.received_audio.extend(audio);
                        self.operations.extend(operations);
                        self.controls = Some(controls);
                        self.guest_count = guests;
                    }
                    Ok(Event::Rejected(error)) => self.error = Some(error),
                    Ok(Event::Failed(error)) => {
                        self.error = Some(error);
                        self.mode = "error".into();
                        closed = true;
                        break;
                    }
                    Ok(Event::Closed) => {
                        self.mode = "disconnected".into();
                        closed = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.mode = "disconnected".into();
                        closed = true;
                        break;
                    }
                }
            }
        }
        if closed {
            self.sending = None;
            self.running = None;
            self.invitation = None;
            self.guest_count = 0;
        }
    }
    pub fn active(&self) -> bool {
        self.running.is_some()
    }
    fn start(
        &mut self,
        work: impl FnOnce(Arc<AtomicBool>, Arc<Mutex<Vec<TcpStream>>>, Receiver<Input>, SyncSender<Event>) + Send + 'static,
    ) -> Result<Value, CommandError> {
        if self.active() {
            return Err("leave the current multiplayer session first".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let sockets = Arc::new(Mutex::new(Vec::new()));
        let (inputs, receive) = mpsc::sync_channel(16);
        let (send, events) = mpsc::sync_channel(32);
        let (thread_stop, thread_sockets) = (Arc::clone(&stop), Arc::clone(&sockets));
        std::thread::Builder::new()
            .name("suhina-lan".into())
            .spawn(move || work(thread_stop, thread_sockets, receive, send))
            .map_err(|e| CommandError::from(format!("could not start multiplayer: {e}")))?;
        self.mode = "starting".into();
        self.error = None;
        self.controls = None;
        self.invitation = None;
        self.guest_count = 0;
        self.project = None;
        self.operations.clear();
        self.receiving = None;
        self.selected_take = None;
        self.received_audio.clear();
        self.sending = None;
        self.running = Some(Running { stop, sockets, inputs, events });
        Ok(json!({"queued":true}))
    }
}

const fn param(name: &'static str, ty: ParamType, doc: &'static str) -> suhina_session::Param {
    suhina_session::Param { name, ty, required: true, doc }
}
const LOCAL: Policy = Policy { agents: Agents::Denied("the person creates and joins LAN sessions"), ..Policy::DEFAULT };
const COMMANDS: &[CommandSpec<Service>] = &[
    CommandSpec {
        id: "multiplayer.host",
        label: "Host a restricted LAN starter session",
        params: &[param("address", ParamType::Text, "private IPv4 address:port; port 0 chooses a free port")],
        kind: CommandKind::Projects,
        run: start_host,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.hostProject",
        label: "Host a copy of the selected project and its prepared audio",
        params: &[param("address", ParamType::Text, "private IPv4 address:port")],
        kind: CommandKind::Projects,
        run: start_selected_host,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.join",
        label: "Join using a private invitation",
        params: &[param("invitation", ParamType::Text, "private invitation")],
        kind: CommandKind::Projects,
        run: start_guest,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.renewInvitation",
        label: "Create a new invitation without replacing the hosted project",
        params: &[],
        kind: CommandKind::Projects,
        run: renew_invitation,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.edit",
        label: "Submit a supported shared edit",
        params: &[
            param("base_revision", ParamType::Integer, "last observed revision"),
            param("edit", ParamType::Text, "typed edit as JSON text"),
        ],
        kind: CommandKind::Projects,
        run: edit,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.publishTake",
        label: "Publish the locally selected loaded take to an existing shared audio track",
        params: &[
            param("base_revision", ParamType::Integer, "observed shared revision"),
            param("track", ParamType::Integer, "existing audio-only track id"),
            param("start", ParamType::Number, "arrangement beat, 0..120000"),
        ],
        kind: CommandKind::Projects,
        run: publish_take,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.state",
        label: "Poll local LAN state",
        params: &[],
        kind: CommandKind::Projects,
        run: state,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.stop",
        label: "Leave and revoke the LAN session",
        params: &[],
        kind: CommandKind::Projects,
        run: stop,
        policy: LOCAL,
    },
];
pub fn commands() -> &'static [CommandSpec<Service>] {
    COMMANDS
}
fn state(s: &mut Service, _: &Value) -> Result<Value, CommandError> {
    s.poll();
    Ok(
        json!({"mode":s.mode,"controls":s.controls,"operations":std::mem::take(&mut s.operations),"invitation":s.invitation,"error":s.error,"guest_connected":s.guest_count > 0,"guests_connected":s.guest_count,"max_guests":MAX_GUESTS,"receiving":s.receiving,"sending":s.sending}),
    )
}
fn renew_invitation(s: &mut Service, _: &Value) -> Result<Value, CommandError> {
    s.poll();
    if s.mode != "hosting" || s.guest_count >= MAX_GUESTS {
        return Err("only a host with space for another guest can create a new invitation".into());
    }
    s.running
        .as_ref()
        .ok_or("host a session first")?
        .inputs
        .try_send(Input::RenewInvitation)
        .map_err(|_| CommandError::from("multiplayer queue is busy; try again"))?;
    s.error = None;
    Ok(json!({"queued":true}))
}
fn stop(s: &mut Service, _: &Value) -> Result<Value, CommandError> {
    s.running = None;
    s.invitation = None;
    s.mode = "idle".into();
    s.guest_count = 0;
    s.selected = None;
    s.project = None;
    s.receiving = None;
    s.selected_take = None;
    s.received_audio.clear();
    s.sending = None;
    Ok(json!({"stopped":true}))
}
fn address(p: &Value) -> Result<SocketAddr, CommandError> {
    let address: SocketAddr = p["address"]
        .as_str()
        .ok_or("address must be text")?
        .parse()
        .map_err(|_| CommandError::from("use a private IPv4 address:port"))?;
    if !private(address.ip()) {
        return Err("use a private or loopback IPv4 address".into());
    }
    Ok(address)
}
fn start_host(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    start_source(s, address(p)?, crate::starter_project(), AudioPool::new())
}
fn start_selected_host(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let address = address(p)?;
    let (source, audio) = s.selected.take().ok_or("select a local MIDI project first")?;
    start_source(s, address, source, audio)
}
fn start_source(
    s: &mut Service,
    address: SocketAddr,
    source: Project,
    audio: AudioPool,
) -> Result<Value, CommandError> {
    let storage = s.storage.clone();
    s.start(move |stop, sockets, inputs, events| {
        if let Err(e) = host(address, source, (audio, storage), &stop, &sockets, inputs, &events) {
            let _ = events.try_send(Event::Failed(e));
        }
    })
}
fn start_guest(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let invitation =
        Invitation::decode(p["invitation"].as_str().ok_or("invitation must be text")?).map_err(CommandError::from)?;
    let storage = s.storage.clone();
    s.start(move |stop, sockets, inputs, events| {
        if let Err(e) = guest(invitation, storage, &stop, &sockets, inputs, &events)
            && !stop.load(Ordering::Relaxed)
        {
            let _ = events.try_send(Event::Failed(e));
        }
    })
}
fn edit(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let text = p["edit"].as_str().ok_or("edit must be JSON text")?;
    if text.len() > 1024 {
        return Err("edit is too large".into());
    }
    let edit: WireEdit = serde_json::from_str(text).map_err(|_| CommandError::from("invalid shared edit"))?;
    if matches!(edit, WireEdit::AudioInsert { .. }) {
        return Err("use explicit loaded-take selection".into());
    }
    edit.edit().map_err(CommandError::from)?;
    let base = p["base_revision"].as_u64().ok_or("invalid revision")?;
    s.running
        .as_ref()
        .ok_or("no multiplayer session")?
        .inputs
        .try_send(Input::Edit { base, edit })
        .map_err(|_| CommandError::from("multiplayer is busy or disconnected"))?;
    s.error = None;
    Ok(json!({"queued":true}))
}

fn publish_take(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    if !matches!(s.mode.as_str(), "hosting" | "connected") || s.sending.is_some() {
        return Err("connect and wait for the pending take before publishing".into());
    }
    let base = p["base_revision"].as_u64().ok_or("invalid revision")?;
    let track = p["track"].as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0).ok_or("invalid track")?;
    let start = p["start"].as_f64().filter(|n| (0.0..=120_000.0).contains(n)).ok_or("invalid start beat")?;
    let audio = s.selected_take.take().ok_or("select a finished loaded take first")?;
    s.running
        .as_ref()
        .ok_or("no multiplayer session")?
        .inputs
        .try_send(Input::Publish { base, track, start, audio })
        .map_err(|_| CommandError::from("multiplayer is busy or disconnected"))?;
    s.error = None;
    s.sending = Some((0, 0));
    Ok(json!({"queued":true}))
}

struct DeadlineSocket {
    stream: TcpStream,
    deadline: Instant,
}
impl DeadlineSocket {
    fn new(stream: TcpStream) -> Self {
        Self { stream, deadline: Instant::now() + TIMEOUT }
    }
    fn renew(&mut self) {
        self.deadline = Instant::now() + TIMEOUT;
    }
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .map(|d| Duration::from_millis(d.as_millis().max(1) as u64))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "LAN exchange deadline"))
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.stream
            .set_read_timeout(Some(self.remaining()?))
            .map_err(|e| std::io::Error::new(e.kind(), format!("read timeout setup: {e}")))?;
        self.stream.read(bytes)
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.stream
            .set_write_timeout(Some(self.remaining()?))
            .map_err(|e| std::io::Error::new(e.kind(), format!("write timeout setup: {e}")))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
struct ConnectionGuard {
    sockets: Arc<Mutex<Vec<TcpStream>>>,
    peer: Option<SocketAddr>,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.sockets.lock().unwrap_or_else(PoisonError::into_inner).retain(|socket| {
            if socket.peer_addr().ok() == self.peer {
                let _ = socket.shutdown(Shutdown::Both);
                false
            } else {
                true
            }
        });
    }
}

fn socket(stream: &TcpStream) -> Result<(), String> {
    // Accepted sockets can inherit the listener's nonblocking mode on macOS.
    // TLS exchanges below use blocking I/O with an absolute exchange deadline.
    stream
        .set_nonblocking(false)
        .and_then(|_| stream.set_read_timeout(Some(TIMEOUT)))
        .and_then(|_| stream.set_write_timeout(Some(TIMEOUT)))
        .and_then(|_| stream.set_nodelay(true))
        .map_err(|e| e.to_string())
}
fn register(stream: &TcpStream, sockets: &Mutex<Vec<TcpStream>>) -> Result<(), String> {
    sockets.lock().unwrap_or_else(PoisonError::into_inner).push(stream.try_clone().map_err(|e| e.to_string())?);
    Ok(())
}
fn host(
    address: SocketAddr,
    source: Project,
    resources: (AudioPool, Option<PathBuf>),
    stop: &Arc<AtomicBool>,
    sockets: &Arc<Mutex<Vec<TcpStream>>>,
    inputs: Receiver<Input>,
    events: &SyncSender<Event>,
) -> Result<(), String> {
    let prepared = assets::prepare(&source, &resources.0, resources.1.as_deref(), stop)?;
    let source = prepared.snapshot.project.clone();
    let initial_assets = prepared.assets;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let key = rcgen::generate_simple_self_signed(vec!["suhina.local".into()])
        .map_err(|_| "could not create the host certificate")?;
    let der = key.cert.der().clone();
    let tls = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(
            vec![der.clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.signing_key.serialize_der())),
        )
        .map_err(|e| e.to_string())?;
    let tls = Arc::new(tls);
    let listener = TcpListener::bind(address).map_err(|e| format!("could not host on {address}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let secret = new_secret()?;
    let mut invitation = Invitation {
        protocol: PROTOCOL,
        endpoint: listener.local_addr().map_err(|e| e.to_string())?,
        certificate: URL_SAFE_NO_PAD.encode(der.as_ref()),
        secret: secret.clone(),
    };
    let invite = invitation.encode()?;
    let host = Host::new(source.clone()).map_err(|e| e.to_string())?;
    let initial = Controls::from_project(host.project(), 0)?;
    let canonical = Arc::new(Mutex::new(Canonical {
        host,
        journal: VecDeque::new(),
        secret: secret_hash(&secret),
        expires: Instant::now() + Duration::from_secs(300),
        redeemed: false,
        guests: BTreeSet::new(),
        assets: initial_assets,
        pending_audio: Vec::new(),
    }));
    events
        .try_send(Event::Ready(invite, initial, Box::new(prepared.snapshot)))
        .map_err(|_| "multiplayer view closed")?;
    let pending = Arc::new(AtomicUsize::new(0));
    let mut handlers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let mut sequence = 1u64;
    let mut last = (0u64, 0usize);
    while !stop.load(Ordering::Relaxed) {
        while let Ok(input) = inputs.try_recv() {
            let result = match input {
                Input::RenewInvitation => {
                    let result = new_secret().and_then(|secret| {
                        invitation.secret = secret;
                        canonical.lock().unwrap_or_else(PoisonError::into_inner).renew(
                            &invitation,
                            Instant::now(),
                            events,
                        )
                    });
                    if let Err(error) = result {
                        let _ = events.try_send(Event::Rejected(error));
                    }
                    continue;
                }
                Input::Edit { base, edit } => {
                    canonical.lock().unwrap_or_else(PoisonError::into_inner).submit(Host::owner(), sequence, base, edit)
                }
                Input::Publish { base, track, start, audio } => {
                    let result =
                        publish(&canonical, sequence, base, track, start, &audio, (resources.1.as_deref(), stop));
                    events.try_send(Event::Upload(None)).map_err(|_| "multiplayer view closed")?;
                    result
                }
            };
            match result {
                Ok(_) => sequence = sequence.checked_add(1).ok_or("operation counter exhausted")?,
                Err(e) => {
                    let _ = events.try_send(Event::Rejected(e));
                }
            }
        }
        {
            canonical.lock().unwrap_or_else(PoisonError::into_inner).emit(events, &mut last)?;
        }
        match listener.accept() {
            Ok((stream, _)) if pending.load(Ordering::Relaxed) < 4 => {
                socket(&stream)?;
                // Keep only live handler sockets; cap retained descriptors independently.
                {
                    let mut saved = sockets.lock().unwrap_or_else(PoisonError::into_inner);
                    if saved.len() >= 16 {
                        saved.retain(|s| s.peer_addr().is_ok() && s.take_error().ok().flatten().is_none());
                    }
                    if saved.len() >= 16 {
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                }
                register(&stream, sockets)?;
                let (tls, canonical, stop, pending, registry) = (
                    Arc::clone(&tls),
                    Arc::clone(&canonical),
                    Arc::clone(stop),
                    Arc::clone(&pending),
                    Arc::clone(sockets),
                );
                let storage = resources.1.clone();
                pending.fetch_add(1, Ordering::Relaxed);
                match std::thread::Builder::new().name("suhina-lan-peer".into()).spawn(move || {
                    let _guard = ConnectionGuard { sockets: registry, peer: stream.peer_addr().ok() };
                    let _ = serve(stream, tls, &canonical, &stop, storage.as_deref());
                    pending.fetch_sub(1, Ordering::Relaxed);
                }) {
                    Ok(handle) => handlers.push(handle),
                    Err(_) => return Err("could not accept a LAN peer".into()),
                }
            }
            Ok((stream, _)) => {
                let _ = stream.shutdown(Shutdown::Both);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(format!("LAN listener stopped: {e}")),
        }
        handlers.retain(|h| !h.is_finished());
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

fn take_edit(target: (u32, f64), meta: &assets::Asset) -> WireEdit {
    WireEdit::AudioInsert {
        track: target.0,
        start: target.1,
        file: meta.path(),
        rate: meta.rate,
        channels: meta.channels,
        frames: meta.frames,
    }
}
fn publish(
    canonical: &Mutex<Canonical>,
    sequence: u64,
    base: u64,
    track: u32,
    start: f64,
    audio: &suhina_engine::AudioData,
    storage: (Option<&std::path::Path>, &AtomicBool),
) -> Result<(), String> {
    let asset = assets::encode_take(audio)?;
    let exists = {
        let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        state.check_take(Host::owner(), sequence, base, (track, start), &asset.meta)?;
        state.assets.iter().any(|a| a.meta.hash == asset.meta.hash)
    };
    let collected = if exists { None } else { Some(assets::collect_take(&asset, storage.0, storage.1)?) };
    canonical.lock().unwrap_or_else(PoisonError::into_inner).commit_take(
        Host::owner(),
        sequence,
        base,
        (track, start),
        asset,
        collected,
    )
}

fn write_update(
    io: &mut StreamOwned<ServerConnection, DeadlineSocket>,
    response: &ServerMessage,
    project: &Project,
    assets: &[Arc<assets::Encoded>],
    known: &mut assets::Known,
    stop: &AtomicBool,
) -> Result<(), String> {
    protocol::write(io, response)?;
    io.sock.deadline = Instant::now() + Duration::from_secs(180);
    assets::send(io, &assets::missing(project, known)?, assets, stop)?;
    for a in assets.iter().filter(|a| assets::references(project).contains(&a.meta.path())) {
        known.insert(a.meta.path(), a.meta.clone());
    }
    Ok(())
}
fn receive_update(
    io: &mut StreamOwned<ClientConnection, DeadlineSocket>,
    project: &Project,
    revision: u64,
    root: Option<&std::path::Path>,
    known: &mut assets::Known,
    stop: &AtomicBool,
    events: &SyncSender<Event>,
) -> Result<Option<AudioAddition>, String> {
    io.sock.deadline = Instant::now() + Duration::from_secs(180);
    let audio =
        assets::receive_known(io, assets::missing(project, known)?, revision, root, stop, known, |done, total| {
            // One notification leaves queue space for Audio/State even during initial admission.
            if done == 0 && total > 0 {
                let _ = events.try_send(Event::Receiving(0, total));
            }
        })?;
    known.extend(audio.metas.iter().map(|a| (a.path(), a.clone())));
    Ok((!audio.audio.files.is_empty()).then(|| audio.into_audio()))
}

fn receive_publication(
    io: &mut StreamOwned<ServerConnection, DeadlineSocket>,
    canonical: &Mutex<Canonical>,
    identity: (suhina_collaboration::PeerId, u64, u64, u64),
    target: (u32, f64),
    meta: &assets::Asset,
    storage: (Option<&std::path::Path>, &AtomicBool),
) -> Result<Result<(), String>, String> {
    let (peer, sequence, base, seen) = identity;
    let existing = {
        let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        state.since(seen)?;
        if let Err(error) = state.check_take(peer, sequence, base, target, meta) {
            return Ok(Err(error));
        }
        state.assets.iter().find(|a| a.meta.hash == meta.hash).cloned()
    };
    let (asset, audio) = if let Some(asset) = existing {
        (asset, None)
    } else {
        if storage.0.is_none() {
            return Ok(Err("host has no shared audio storage".into()));
        }
        io.sock.deadline = Instant::now() + Duration::from_secs(180);
        let (asset, audio) = assets::receive_take(io, meta, storage.0, storage.1)?;
        (asset, Some(audio))
    };
    let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
    if storage.1.load(Ordering::Relaxed) {
        return Err("session stopped".into());
    }
    Ok(state.commit_take(peer, sequence, base, target, asset, audio))
}

fn serve(
    stream: TcpStream,
    tls: Arc<ServerConfig>,
    canonical: &Mutex<Canonical>,
    stop: &AtomicBool,
    storage: Option<&std::path::Path>,
) -> Result<(), String> {
    let connection = ServerConnection::new(tls).map_err(|e| e.to_string())?;
    let mut io = StreamOwned::new(connection, DeadlineSocket::new(stream));
    let hello: ClientMessage = protocol::read(&mut io)?;
    let peer = {
        let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        let result = match hello {
            ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret } => state.admit(&secret, Instant::now()),
            _ => Err("admission refused".into()),
        };
        match result {
            Ok(peer) => peer,
            Err(_) => {
                drop(state);
                protocol::write(&mut io, &ServerMessage::Denied)?;
                return Err("admission refused".into());
            }
        }
    };
    let result = (|| {
        io.sock.deadline = Instant::now() + Duration::from_secs(30);
        let (snapshot, revision, assets) = {
            let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
            (state.host.project().clone(), state.host.revision(), state.assets.clone())
        };
        let (_, document) = crate::midi::encode_portable(&snapshot)?;
        crate::transfer::send(&mut io, &document, revision, stop)?;
        io.sock.deadline = Instant::now() + Duration::from_secs(180);
        assets::send(&mut io, &snapshot, &assets, stop)?;
        let mut known: assets::Known = assets
            .iter()
            .filter(|a| assets::references(&snapshot).contains(&a.meta.path()))
            .map(|a| (a.meta.path(), a.meta.clone()))
            .collect();
        io.sock.renew();
        let (initial, project, assets) = {
            let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
            (state.response(revision, None)?, state.host.project().clone(), state.assets.clone())
        };
        write_update(&mut io, &initial, &project, &assets, &mut known, stop)?;
        while !stop.load(Ordering::Relaxed) {
            io.sock.renew();
            let message: ClientMessage = protocol::read(&mut io)?;
            let upload = if let ClientMessage::Publish {
                protocol: PROTOCOL,
                sequence,
                base_revision,
                seen_revision,
                track,
                start,
                asset,
            } = &message
            {
                Some((
                    *seen_revision,
                    receive_publication(
                        &mut io,
                        canonical,
                        (peer, *sequence, *base_revision, *seen_revision),
                        (*track, *start),
                        asset,
                        (storage, stop),
                    )?,
                ))
            } else {
                None
            };
            let (response, project, assets) = {
                let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
                let (seen, outcome) = match (upload, &message) {
                    (Some((seen, result)), _) => (seen, Some(result)),
                    (None, ClientMessage::Poll { protocol: PROTOCOL, seen_revision }) => (*seen_revision, None),
                    (
                        None,
                        ClientMessage::Edit { protocol: PROTOCOL, sequence, base_revision, seen_revision, edit },
                    ) if *sequence > 0 => {
                        // Validate delivery cursor before allowing a mutation.
                        state.since(*seen_revision)?;
                        (*seen_revision, Some(state.submit(peer, *sequence, *base_revision, edit.clone())))
                    }
                    _ => return Err("unsupported LAN protocol message".into()),
                };
                (state.response(seen, outcome)?, state.host.project().clone(), state.assets.clone())
            };
            if let (ClientMessage::Publish { asset, .. }, ServerMessage::State { accepted: Some(true), .. }) =
                (&message, &response)
            {
                known.insert(asset.path(), asset.clone());
            }
            write_update(&mut io, &response, &project, &assets, &mut known, stop)?;
            // Rate bound also applies to a guest bypassing our client's polling cadence.
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    })();
    let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = state.host.remove_peer(peer);
    state.guests.remove(&peer);
    result
}

struct PreparedTake {
    base: u64,
    track: u32,
    start: f64,
    asset: Arc<assets::Encoded>,
    audio: AudioAddition,
}
struct PendingTake {
    ready: Receiver<Result<PreparedTake, String>>,
}
fn prepare_take(
    base: u64,
    track: u32,
    start: f64,
    audio: Arc<suhina_engine::AudioData>,
    root: Option<PathBuf>,
    stop: Arc<AtomicBool>,
) -> Result<PendingTake, String> {
    let (send, ready) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("suhina-take-copy".into())
        .spawn(move || {
            let prepared = (|| {
                if stop.load(Ordering::Relaxed) {
                    return Err("session stopped".into());
                }
                let asset = assets::encode_take(&audio)?;
                let collected = assets::collect_take(&asset, root.as_deref(), &stop)?;
                Ok(PreparedTake { base, track, start, asset, audio: collected })
            })();
            let _ = send.try_send(prepared);
        })
        .map_err(|e| format!("cannot prepare take: {e}"))?;
    Ok(PendingTake { ready })
}

fn guest(
    invitation: Invitation,
    storage: Option<PathBuf>,
    stop: &Arc<AtomicBool>,
    sockets: &Mutex<Vec<TcpStream>>,
    inputs: Receiver<Input>,
    events: &SyncSender<Event>,
) -> Result<(), String> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(invitation.der()?)).map_err(|_| "invalid invitation certificate")?;
    let tls = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = TcpStream::connect_timeout(&invitation.endpoint, TIMEOUT)
        .map_err(|e| format!("could not reach the LAN host: {e}"))?;
    socket(&stream)?;
    register(&stream, sockets)?;
    let name = ServerName::try_from("suhina.local").map_err(|_| "invalid server name")?;
    let mut io = StreamOwned::new(
        ClientConnection::new(Arc::new(tls), name).map_err(|e| e.to_string())?,
        DeadlineSocket::new(stream),
    );
    protocol::write(&mut io, &ClientMessage::Hello { protocol: PROTOCOL, format: 2, secret: invitation.secret })?;
    io.sock.deadline = Instant::now() + Duration::from_secs(30);
    let offer: ServerMessage = protocol::read(&mut io)?;
    let (document, mut revision) = crate::transfer::receive(&mut io, offer, stop, |done, total| {
        let _ = events.try_send(Event::Receiving(done, total));
    })?;
    let project = crate::midi::decode_portable(&document)?;
    io.sock.deadline = Instant::now() + Duration::from_secs(180);
    let mut snapshot = assets::receive(&mut io, project, revision, storage.as_deref(), stop, |done, total| {
        let _ = events.try_send(Event::Receiving(done, total));
    })?;
    let mut known: assets::Known = snapshot.metas.iter().map(|a| (a.path(), a.clone())).collect();
    let mut mirror = suhina_session::Session::new(snapshot.project.clone());
    io.sock.renew();
    let first: ServerMessage = protocol::read(&mut io)?;
    let ServerMessage::State { protocol: PROTOCOL, controls, operations, guests, accepted: None, error: None } = first
    else {
        return Err("invitation refused, expired or already used".into());
    };
    protocol::validate_guest_count(guests)?;
    let mut previous_guests = guests;
    let controls = *controls;
    protocol::apply_operations(&mut mirror, &mut revision, &controls, &operations)?;
    if let Some(audio) =
        receive_update(&mut io, mirror.project(), revision, storage.as_deref(), &mut known, stop, events)?
    {
        snapshot.add_audio(audio);
    }
    let mut previous = controls.clone();
    snapshot.project = mirror.project().clone();
    snapshot.revision = revision;
    events.try_send(Event::Connected(controls, Box::new(snapshot), guests)).map_err(|_| "multiplayer view closed")?;
    let mut sequence = 1u64;
    let mut pending: Option<PendingTake> = None;
    while !stop.load(Ordering::Relaxed) {
        io.sock.renew();
        let input = if pending.is_none() { inputs.try_recv().ok() } else { None };
        let edit = match input {
            Some(Input::Publish { base, track, start, audio }) => {
                pending = Some(prepare_take(base, track, start, audio, storage.clone(), Arc::clone(stop))?);
                None
            }
            Some(Input::Edit { base, edit }) => Some((base, edit)),
            Some(Input::RenewInvitation) => return Err("only the host can renew invitations".into()),
            None => None,
        };
        let prepared = match pending.as_ref().map(|p| p.ready.try_recv()) {
            Some(Ok(Ok(take))) => {
                pending = None;
                Some(take)
            }
            Some(Ok(Err(error))) => {
                pending = None;
                events.try_send(Event::Upload(None)).map_err(|_| "multiplayer view closed")?;
                events.try_send(Event::Rejected(error)).map_err(|_| "multiplayer view closed")?;
                None
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => return Err("take preparation stopped".into()),
            _ => None,
        };
        let edited = prepared.is_some() || edit.is_some();
        let request = if let Some(take) = &prepared {
            ClientMessage::Publish {
                protocol: PROTOCOL,
                sequence,
                base_revision: take.base,
                seen_revision: revision,
                track: take.track,
                start: take.start,
                asset: take.asset.meta.clone(),
            }
        } else if let Some((base, edit)) = edit {
            ClientMessage::Edit { protocol: PROTOCOL, sequence, base_revision: base, seen_revision: revision, edit }
        } else {
            ClientMessage::Poll { protocol: PROTOCOL, seen_revision: revision }
        };
        protocol::write(&mut io, &request)?;
        let response = if let Some(take) = &prepared {
            io.sock.deadline = Instant::now() + Duration::from_secs(180);
            assets::send_take(&mut io, &take.asset, stop, |done, total| {
                let _ = events.try_send(Event::Upload(Some((done, total))));
            })?
        } else {
            protocol::read(&mut io)?
        };
        let ServerMessage::State { protocol: PROTOCOL, controls, operations, guests, accepted, error } = response
        else {
            return Err("invalid host response".into());
        };
        protocol::validate_guest_count(guests)?;
        let controls = *controls;
        let was_publication = prepared.is_some();
        let mut audio = Vec::new();
        if let Some(take) = prepared
            && accepted == Some(true)
        {
            let expected = take_edit((take.track, take.start), &take.asset.meta);
            if !operations.iter().any(|op| Some(op.revision) == take.base.checked_add(1) && op.edit == expected) {
                return Err("host did not confirm the offered take operation".into());
            }
            if assets::remember_take(&mut known, &take.asset.meta)? {
                audio.push(take.audio);
            }
        }
        protocol::apply_operations(&mut mirror, &mut revision, &controls, &operations)?;
        if let Some(addition) =
            receive_update(&mut io, mirror.project(), revision, storage.as_deref(), &mut known, stop, events)?
        {
            audio.push(addition);
        }
        if controls.revision < previous.revision || controls.revision == previous.revision && controls != previous {
            return Err("host control revision mismatch".into());
        }
        if was_publication {
            events.try_send(Event::Upload(None)).map_err(|_| "multiplayer view closed")?;
        }
        if edited {
            match accepted {
                Some(true) => sequence = sequence.checked_add(1).ok_or("operation counter exhausted")?,
                Some(false) => {
                    events
                        .try_send(Event::Rejected(error.unwrap_or_else(|| "the host refused this edit".into())))
                        .map_err(|_| "multiplayer view closed")?;
                }
                None => return Err("host did not acknowledge the edit".into()),
            }
        }
        if controls != previous || guests != previous_guests {
            events
                .try_send(Event::State(controls.clone(), operations, guests, audio))
                .map_err(|_| "multiplayer view is not keeping up")?;
            previous = controls;
            previous_guests = guests;
        }
        std::thread::sleep(POLL);
    }
    let _ = events.try_send(Event::Closed);
    Ok(())
}

/// Local routing lookup only: UDP connect does not send a datagram. Never use this as admission.
pub fn suggested_address() -> SocketAddr {
    std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| {
            s.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            s.local_addr()
        })
        .ok()
        .filter(|a| private(a.ip()))
        .map(|a| SocketAddr::new(a.ip(), 0))
        .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
