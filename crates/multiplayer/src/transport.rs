// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::assets::{self, AudioAddition, SharedSnapshot};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
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

use crate::invitation::{Invitation, Invite, new_secret, private, secret_hash};
use crate::linked::{ClockEstimate, Linked, SharedTransport, TransportRequest};
use crate::online::{Lobby, Net, OnlineInvitation, Reach, ShortCode};
use crate::protocol::{self, ClientMessage, Controls, Operation, PROTOCOL, Refusal, ServerMessage, WireEdit};
use crate::resume::{Backoff, FAREWELL, Tuning, Watch};

pub const MAX_GUESTS: usize = 3;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);

struct Canonical {
    host: Host,
    journal: VecDeque<Operation>,
    secret: [u8; 32],
    expires: Instant,
    redeemed: bool,
    /// Admitted guests, connected or holding their place after a dropped link.
    members: BTreeMap<PeerId, Member>,
    /// Away guests the view was last told about.
    away_shown: usize,
    assets: Vec<Arc<assets::Encoded>>,
    pending_audio: Vec<AudioAddition>,
    linked: Linked,
}
/// One admitted guest. Its place lasts while it is connected and for a grace period after its
/// link drops, during which its token resumes it as the same peer.
struct Member {
    /// SHA-256 of the current resume token; replaced at every resume.
    token: [u8; 32],
    /// Counts this guest's connections; a resumed connection supersedes the older one.
    generation: u64,
    /// When the place is released, while the guest is away.
    away: Option<Instant>,
    /// Sounds this guest is known to hold, kept for a resume without a fresh copy.
    known: assets::Known,
}
/// What a hello opens on the host.
enum Opening {
    Admitted { peer: PeerId, generation: u64, token: String },
    Resumed { peer: PeerId, generation: u64, token: String, resume: Option<(assets::Known, u64)> },
}
/// How a guest's connection ended, as its handler saw it.
#[derive(Clone, Copy)]
enum Departure {
    /// The guest said goodbye, or broke the protocol: its place is freed at once.
    Left,
    /// The link dropped: the place is kept for the grace period.
    Dropped,
    /// A newer connection of the same guest took over.
    Superseded,
}
impl Canonical {
    /// Guests connected now.
    fn connected(&self) -> usize {
        self.members.values().filter(|m| m.away.is_none()).count()
    }
    /// Guests connected now, or away and holding their place.
    fn seats(&self) -> usize {
        self.members.len()
    }
    /// Releases the places whose grace period has ended; their tokens stop working.
    fn expire(&mut self, now: Instant) {
        let ended: Vec<PeerId> =
            self.members.iter().filter(|(_, m)| m.away.is_some_and(|until| now >= until)).map(|(p, _)| *p).collect();
        for peer in ended {
            self.members.remove(&peer);
            let _ = self.host.remove_peer(peer);
        }
    }
    /// Records how a guest's connection ended, unless a newer connection already replaced it.
    fn depart(&mut self, peer: PeerId, generation: u64, how: Departure, now: Instant, grace: Duration) {
        if self.members.get(&peer).is_none_or(|m| m.generation != generation) {
            return;
        }
        match how {
            Departure::Superseded => {}
            Departure::Left => {
                self.members.remove(&peer);
                let _ = self.host.remove_peer(peer);
            }
            Departure::Dropped => {
                if let Some(member) = self.members.get_mut(&peer) {
                    member.away = now.checked_add(grace).or(Some(now));
                }
            }
        }
    }
    /// Whether this connection is still the guest's current one.
    fn current(&self, peer: PeerId, generation: u64) -> bool {
        self.members.get(&peer).is_some_and(|m| m.generation == generation)
    }
    /// Saves what the guest holds after a complete exchange, for a later resume.
    fn remember(&mut self, peer: PeerId, generation: u64, known: &assets::Known) {
        if let Some(member) = self.members.get_mut(&peer).filter(|m| m.generation == generation) {
            member.known.clone_from(known);
        }
    }
    /// Checks a hello and admits or resumes its guest. Secrets and tokens are compared in
    /// constant time and a wrong one says nothing about how close it was.
    fn open(&mut self, hello: Value, now: Instant) -> Result<Opening, Refusal> {
        let guest = hello.get("protocol").and_then(Value::as_u64);
        if guest != Some(u64::from(PROTOCOL)) {
            let guest = guest.and_then(|n| u16::try_from(n).ok()).unwrap_or(0);
            return Err(Refusal::Protocol { host: PROTOCOL, guest });
        }
        let hello: ClientMessage = serde_json::from_value(hello).map_err(|_| Refusal::Unsupported)?;
        let ours = protocol::engine_revision().map_err(|_| Refusal::Unsupported)?;
        let (format, engine) = match &hello {
            ClientMessage::Hello { format, engine, .. } | ClientMessage::Resume { format, engine, .. } => {
                (*format, engine.clone())
            }
            _ => return Err(Refusal::Unsupported),
        };
        if engine != ours || format != protocol::COPY_FORMAT {
            let formats = (engine == ours).then_some((protocol::COPY_FORMAT, format));
            return Err(Refusal::Build {
                host: protocol::build_identity(&ours, formats.map(|f| f.0)),
                guest: protocol::build_identity(&engine, formats.map(|f| f.1)),
            });
        }
        self.expire(now);
        match hello {
            ClientMessage::Hello { secret, .. } => {
                let (peer, token) = self.admit(&secret, now)?;
                Ok(Opening::Admitted { peer, generation: 0, token })
            }
            ClientMessage::Resume { token, seen_revision, .. } => self.resume(&token, seen_revision),
            _ => Err(Refusal::Unsupported),
        }
    }
    fn resume(&mut self, token: &str, seen: Option<u64>) -> Result<Opening, Refusal> {
        let presented = secret_hash(token);
        // Compare against every member, without stopping early.
        let mut found = None;
        for (peer, member) in &self.members {
            if bool::from(member.token.ct_eq(&presented)) && token.len() == 64 {
                found = Some(*peer);
            }
        }
        let peer = found.ok_or(Refusal::Resume)?;
        let fresh = new_secret().map_err(|_| Refusal::Unsupported)?;
        let member = self.members.get_mut(&peer).ok_or(Refusal::Resume)?;
        member.token = secret_hash(&fresh);
        member.generation = member.generation.wrapping_add(1);
        member.away = None;
        let generation = member.generation;
        let known = member.known.clone();
        // A resume without a fresh copy needs the history since the guest's revision.
        let resume = seen.filter(|seen| self.since(*seen).is_ok()).map(|seen| (known, seen));
        Ok(Opening::Resumed { peer, generation, token: fresh, resume })
    }
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
            || self.assets.len() >= assets::MAX_SOUNDS
            || self.assets.iter().map(|a| a.meta.bytes as u64).sum::<u64>() + meta.bytes as u64 > assets::MAX_TOTAL
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
    fn emit(&mut self, events: &SyncSender<Event>, last: &mut (u64, usize, u64)) -> Result<(), String> {
        self.expire(Instant::now());
        let current = (self.host.revision(), self.connected());
        if current != (last.0, last.1) {
            let controls = Controls::from_project(self.host.project(), current.0)?;
            let operations = self.since(last.0)?;
            let audio = std::mem::take(&mut self.pending_audio);
            match events.try_send(Event::State(controls, operations, current.1, audio)) {
                Ok(()) => (last.0, last.1) = current,
                Err(mpsc::TrySendError::Full(Event::State(_, _, _, audio))) => self.pending_audio = audio,
                Err(_) => return Err("multiplayer view closed".into()),
            }
        }
        let away = self.seats() - self.connected();
        if away != self.away_shown {
            match events.try_send(Event::Away(away)) {
                Ok(()) => self.away_shown = away,
                Err(mpsc::TrySendError::Full(_)) => {}
                Err(_) => return Err("multiplayer view closed".into()),
            }
        }
        let seq = self.linked.seq();
        if seq != last.2 {
            // The host's own clock: there is no offset to estimate.
            let clock = LocalClock { epoch: self.linked.clock.epoch(), offset: 0 };
            match events.try_send(Event::Transport(self.linked.view(), clock)) {
                Ok(()) => last.2 = seq,
                Err(mpsc::TrySendError::Full(_)) => {}
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
            guests: self.connected(),
            controls: Box::new(Controls::from_project(self.host.project(), self.host.revision())?),
            operations,
            accepted,
            error,
            host_ms: self.linked.clock.now_ms(),
            transport: self.linked.view(),
        })
    }
    fn renew(&mut self, invitation: &dyn Invite, now: Instant, events: &SyncSender<Event>) -> Result<(), String> {
        self.expire(now);
        if self.seats() >= MAX_GUESTS {
            return Err(if self.connected() < MAX_GUESTS {
                "a guest whose connection dropped still holds a place for two minutes; wait for it to come back or for its place to free".into()
            } else {
                "the session already has three guests; wait for a guest to leave".into()
            });
        }
        let secret = secret_hash(invitation.secret());
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
    /// Admits the holder of the current code and gives it a resume token. Only the holder of the
    /// right code learns that it expired, was used or that the session is full.
    fn admit(&mut self, secret: &str, now: Instant) -> Result<(PeerId, String), Refusal> {
        let matches = bool::from(self.secret.ct_eq(&secret_hash(secret)));
        if secret.len() != 64 || !matches {
            return Err(Refusal::Invalid);
        }
        if self.redeemed {
            return Err(Refusal::Used);
        }
        if now >= self.expires {
            return Err(Refusal::Expired);
        }
        if self.seats() >= MAX_GUESTS {
            return Err(Refusal::Full);
        }
        let token = new_secret().map_err(|_| Refusal::Unsupported)?;
        let peer = self.host.add_peer(Role::Editor).map_err(|_| Refusal::Full)?;
        self.redeemed = true;
        self.members.insert(
            peer,
            Member { token: secret_hash(&token), generation: 0, away: None, known: assets::Known::new() },
        );
        Ok((peer, token))
    }
}
enum Event {
    Ready(String, Controls, Box<SharedSnapshot>),
    Connected(Controls, Box<SharedSnapshot>, usize),
    Invitation(String),
    /// The short code of the lobby that hands out the current online invitation.
    ShortCode(String),
    Receiving(usize, usize),
    Upload(Option<(usize, usize)>),
    State(Controls, Vec<Operation>, usize, Vec<AudioAddition>),
    Transport(Option<SharedTransport>, LocalClock),
    Rejected(String),
    Failed(String),
    /// Host: guests whose link dropped and who still hold their place.
    Away(usize),
    /// Guest: the link dropped; trying to come back.
    Reconnecting,
    /// Guest: back in the session as the same participant.
    Resumed,
    /// Guest: the host ended the session on purpose.
    Ended,
    Closed,
}
enum Input {
    RenewInvitation,
    Edit { base: u64, edit: WireEdit },
    Publish { base: u64, track: u32, start: f64, audio: Arc<suhina_engine::AudioData> },
    Transport(TransportRequest),
}

/// A local clock and its estimated distance to the host clock, so the view can schedule a start.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LocalClock {
    epoch: Instant,
    /// Host clock minus the clock that starts at `epoch`, in milliseconds.
    offset: i64,
}
impl LocalClock {
    fn host_now_ms(&self) -> i64 {
        i64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(i64::MAX).saturating_add(self.offset)
    }
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
        // Leave the workers a moment to say goodbye (a guest leaving, a host ending the session)
        // before the remaining sockets are shut down.
        let sockets = Arc::clone(&self.sockets);
        let close = move || {
            for socket in sockets.lock().unwrap_or_else(PoisonError::into_inner).iter() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        };
        let later = close.clone();
        let spawned = std::thread::Builder::new().name("suhina-lan-close".into()).spawn(move || {
            std::thread::sleep(FAREWELL + FAREWELL);
            later();
        });
        if spawned.is_err() {
            close();
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
    short_code: Option<String>,
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
    transport: Option<SharedTransport>,
    clock: Option<LocalClock>,
    /// Host: guests whose link dropped and who still hold their place.
    guests_away: usize,
    tuning: Tuning,
}

impl Service {
    /// Test timings and fault injection for the next session.
    #[cfg(test)]
    pub(crate) fn set_tuning(&mut self, tuning: Tuning) {
        self.tuning = tuning;
    }
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
                    if wanted.files.len() >= assets::MAX_SOUNDS && !wanted.files.contains_key(&region.file) {
                        return Err(format!("at most {} source sounds can be shared", assets::MAX_SOUNDS).into());
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
        if audio.samples.len() > (assets::MAX_ASSET - 68) / 4 {
            return Err(format!("the take exceeds {} MiB", assets::MAX_ASSET >> 20).into());
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
                    Ok(Event::Invitation(invitation)) => {
                        self.invitation = Some(invitation);
                        self.short_code = None;
                    }
                    Ok(Event::ShortCode(code)) => self.short_code = Some(code),
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
                    Ok(Event::Transport(transport, clock)) => {
                        self.transport = transport;
                        self.clock = Some(clock);
                    }
                    Ok(Event::Rejected(error)) => self.error = Some(error),
                    Ok(Event::Away(count)) => self.guests_away = count,
                    Ok(Event::Reconnecting) => self.mode = "reconnecting".into(),
                    Ok(Event::Resumed) => self.mode = "connected".into(),
                    Ok(Event::Ended) => {
                        self.error = Some("The host ended the session. Your copy of the project is kept.".into());
                        self.mode = "disconnected".into();
                        closed = true;
                        break;
                    }
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
            self.short_code = None;
            self.guest_count = 0;
            self.guests_away = 0;
            self.transport = None;
            self.clock = None;
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
        self.short_code = None;
        self.guest_count = 0;
        self.project = None;
        self.operations.clear();
        self.receiving = None;
        self.selected_take = None;
        self.received_audio.clear();
        self.sending = None;
        self.transport = None;
        self.clock = None;
        self.guests_away = 0;
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
        id: "multiplayer.linkPlayback",
        label: "Link or unlink playback: every participant then plays, stops and moves together",
        params: &[param("linked", ParamType::Bool, "true links playback for everyone, false unlinks it")],
        kind: CommandKind::Projects,
        run: link_playback,
        policy: LOCAL,
    },
    CommandSpec {
        id: "multiplayer.transport",
        label: "Play, stop or move the linked transport of every participant",
        params: &[
            param("action", ParamType::Text, "play, stop or seek"),
            param("beats", ParamType::Number, "where play or seek starts, 0..120000; stop ignores it"),
        ],
        kind: CommandKind::Projects,
        run: shared_transport,
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
    // The local delay to the shared start, so the view needs no clock arithmetic of its own.
    let starts_in = match (s.transport, s.clock) {
        (Some(t), Some(clock)) if t.playing => Some(crate::linked::starts_in_ms(t.start_ms, clock.host_now_ms())),
        _ => None,
    };
    Ok(
        json!({"mode":s.mode,"controls":s.controls,"operations":std::mem::take(&mut s.operations),"invitation":s.invitation,"short_code":s.short_code,"error":s.error,"guest_connected":s.guest_count > 0,"guests_connected":s.guest_count,"guests_reconnecting":s.guests_away,"reconnecting":s.mode == "reconnecting","max_guests":MAX_GUESTS,"receiving":s.receiving,"sending":s.sending,"linked":s.transport.is_some(),"transport":s.transport,"transport_starts_in_ms":starts_in}),
    )
}
fn queue_transport(s: &mut Service, request: TransportRequest) -> Result<Value, CommandError> {
    s.poll();
    if !matches!(s.mode.as_str(), "hosting" | "connected") {
        return Err("connect before linking playback".into());
    }
    request.validate().map_err(CommandError::from)?;
    s.running
        .as_ref()
        .ok_or("no multiplayer session")?
        .inputs
        .try_send(Input::Transport(request))
        .map_err(|_| CommandError::from("multiplayer is busy or disconnected"))?;
    s.error = None;
    Ok(json!({"queued":true}))
}
fn link_playback(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let linked = p["linked"].as_bool().ok_or("linked must be true or false")?;
    queue_transport(s, TransportRequest::Link { linked })
}
fn shared_transport(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let beat = p["beats"]
        .as_f64()
        .filter(|b| b.is_finite() && (0.0..=crate::linked::MAX_BEAT).contains(b))
        .ok_or("beats must be a number between 0 and 120000")?;
    let request = match p["action"].as_str() {
        Some("play") => TransportRequest::Play { beat },
        Some("stop") => TransportRequest::Stop {},
        Some("seek") => TransportRequest::Seek { beat },
        _ => return Err("action must be play, stop or seek".into()),
    };
    s.poll();
    if s.transport.is_none() {
        return Err("turn on Link playback first".into());
    }
    queue_transport(s, request)
}
fn renew_invitation(s: &mut Service, _: &Value) -> Result<Value, CommandError> {
    s.poll();
    if s.mode != "hosting" || s.guest_count + s.guests_away >= MAX_GUESTS {
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
    s.short_code = None;
    s.mode = "idle".into();
    s.guest_count = 0;
    s.selected = None;
    s.project = None;
    s.receiving = None;
    s.selected_take = None;
    s.received_audio.clear();
    s.sending = None;
    s.transport = None;
    s.clock = None;
    s.guests_away = 0;
    Ok(json!({"stopped":true}))
}
fn address(p: &Value) -> Result<Place, CommandError> {
    let text = p["address"].as_str().ok_or("address must be text")?;
    match text.trim() {
        ONLINE => return Ok(Place::Online(Reach::Internet)),
        ONLINE_LOOPBACK => return Ok(Place::Online(Reach::Loopback)),
        _ => {}
    }
    let address: SocketAddr = text.parse().map_err(|_| CommandError::from("use a private IPv4 address:port"))?;
    if !private(address.ip()) {
        return Err("use a private or loopback IPv4 address".into());
    }
    Ok(Place::Lan(address))
}
/// Host address that invites over the Internet with a code.
pub const ONLINE: &str = "online";
/// Online session reachable from this computer only (two instances side by side, tests).
pub const ONLINE_LOOPBACK: &str = "online-loopback";
fn start_host(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    start_source(s, address(p)?, crate::starter_project(), AudioPool::new())
}
fn start_selected_host(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let place = address(p)?;
    let (source, audio) = s.selected.take().ok_or("select a local MIDI project first")?;
    start_source(s, place, source, audio)
}
fn start_source(s: &mut Service, place: Place, source: Project, audio: AudioPool) -> Result<Value, CommandError> {
    let (storage, tuning) = (s.storage.clone(), s.tuning.clone());
    s.start(move |stop, sockets, inputs, events| {
        if let Err(e) = host((place, &tuning), source, (audio, storage), &stop, &sockets, inputs, &events) {
            let _ = events.try_send(Event::Failed(e));
        }
    })
}
fn start_guest(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let text = p["invitation"].as_str().ok_or("invitation must be text")?;
    if ShortCode::looks_like(text) {
        // Checked here, before any network use; the lobby is asked on the session thread.
        let code = ShortCode::parse(text).map_err(CommandError::from)?;
        let (storage, tuning) = (s.storage.clone(), s.tuning.clone());
        return s.start(move |stop, _, inputs, events| {
            let joined = crate::online::redeem(&code).and_then(|(net, invitation)| {
                online_guest(invitation, Some(net), storage, &tuning, &stop, inputs, &events)
            });
            if let Err(e) = joined
                && !stop.load(Ordering::Relaxed)
            {
                let _ = events.try_send(Event::Failed(e));
            }
        });
    }
    if OnlineInvitation::is_online(text) {
        let invitation = OnlineInvitation::decode(text).map_err(CommandError::from)?;
        let (storage, tuning) = (s.storage.clone(), s.tuning.clone());
        return s.start(move |stop, _, inputs, events| {
            if let Err(e) = online_guest(invitation, None, storage, &tuning, &stop, inputs, &events)
                && !stop.load(Ordering::Relaxed)
            {
                let _ = events.try_send(Event::Failed(e));
            }
        });
    }
    let invitation = Invitation::decode(text).map_err(CommandError::from)?;
    let (storage, tuning) = (s.storage.clone(), s.tuning.clone());
    s.start(move |stop, sockets, inputs, events| {
        if let Err(e) = guest(invitation, storage, &tuning, &stop, &sockets, inputs, &events)
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
/// A blocking, authenticated stream to one peer whose exchanges share an absolute deadline.
pub(crate) trait Link: Read + Write {
    fn set_deadline(&mut self, deadline: Instant);
    fn renew(&mut self) {
        self.set_deadline(Instant::now() + TIMEOUT);
    }
}
impl<L: Link + ?Sized> Link for Box<L> {
    fn set_deadline(&mut self, deadline: Instant) {
        (**self).set_deadline(deadline);
    }
}
impl Link for StreamOwned<ServerConnection, DeadlineSocket> {
    fn set_deadline(&mut self, deadline: Instant) {
        self.sock.deadline = deadline;
    }
}
impl Link for StreamOwned<ClientConnection, DeadlineSocket> {
    fn set_deadline(&mut self, deadline: Instant) {
        self.sock.deadline = deadline;
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
/// Where a host listens: a private LAN address, or the Internet through an invitation code.
#[derive(Clone, Copy, Debug)]
enum Place {
    Lan(SocketAddr),
    Online(Reach),
}
enum Acceptor {
    Lan { listener: TcpListener, tls: Arc<ServerConfig> },
    Online { net: Net, incoming: Receiver<crate::online::Stream> },
}
fn lan_listener(address: SocketAddr, secret: String) -> Result<(TcpListener, Arc<ServerConfig>, Invitation), String> {
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
    let listener = TcpListener::bind(address).map_err(|e| format!("could not host on {address}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let invitation = Invitation {
        protocol: PROTOCOL,
        endpoint: listener.local_addr().map_err(|e| e.to_string())?,
        certificate: URL_SAFE_NO_PAD.encode(der.as_ref()),
        secret,
    };
    Ok((listener, Arc::new(tls), invitation))
}
fn host(
    (place, tuning): (Place, &Tuning),
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
    let secret = new_secret()?;
    let (acceptor, mut invitation): (Acceptor, Box<dyn Invite>) = match place {
        Place::Lan(address) => {
            let (listener, tls, invitation) = lan_listener(address, secret.clone())?;
            (Acceptor::Lan { listener, tls }, Box::new(invitation))
        }
        Place::Online(reach) => {
            let net = Net::bind(reach)?;
            let invitation = OnlineInvitation { protocol: PROTOCOL, endpoint: net.address(), secret: secret.clone() };
            // Four handshakes at once, as on the LAN: three guests plus one being admitted.
            let incoming = net.listen(MAX_GUESTS + 1);
            (Acceptor::Online { net, incoming }, Box::new(invitation))
        }
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
        members: BTreeMap::new(),
        away_shown: 0,
        assets: initial_assets,
        pending_audio: Vec::new(),
        linked: Linked::new(),
    }));
    events
        .try_send(Event::Ready(invite.clone(), initial, Box::new(prepared.snapshot)))
        .map_err(|_| "multiplayer view closed")?;
    // Online, a lobby trades a short code for the invitation; declared after the acceptor, so it
    // closes before the endpoint's runtime does.
    let mut lobby = open_lobby(&acceptor, invite, events);
    let pending = Arc::new(AtomicUsize::new(0));
    let mut handlers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let mut sequence = 1u64;
    let mut last = (0u64, 0usize, 0u64);
    while !stop.load(Ordering::Relaxed) {
        while let Ok(input) = inputs.try_recv() {
            let result = match input {
                Input::RenewInvitation => {
                    let result = new_secret().and_then(|secret| {
                        invitation.set_secret(secret);
                        canonical.lock().unwrap_or_else(PoisonError::into_inner).renew(
                            &*invitation,
                            Instant::now(),
                            events,
                        )
                    });
                    match result.and_then(|()| invitation.encode()) {
                        Ok(invite) => {
                            // The old code dies with its lobby before the new one is announced.
                            drop(lobby.take());
                            lobby = open_lobby(&acceptor, invite, events);
                        }
                        Err(error) => {
                            let _ = events.try_send(Event::Rejected(error));
                        }
                    }
                    continue;
                }
                Input::Transport(request) => {
                    let result = canonical.lock().unwrap_or_else(PoisonError::into_inner).linked.apply(request);
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
                        publish(&canonical, sequence, base, (track, start), &audio, (resources.1.as_deref(), stop));
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
        match &acceptor {
            Acceptor::Lan { listener, tls } => {
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
                            Arc::clone(tls),
                            Arc::clone(&canonical),
                            Arc::clone(stop),
                            Arc::clone(&pending),
                            Arc::clone(sockets),
                        );
                        let (storage, tuning) = (resources.1.clone(), tuning.clone());
                        pending.fetch_add(1, Ordering::Relaxed);
                        match std::thread::Builder::new().name("suhina-lan-peer".into()).spawn(move || {
                            let _guard = ConnectionGuard { sockets: registry, peer: stream.peer_addr().ok() };
                            let _ = serve(stream, tls, &canonical, &stop, storage.as_deref(), &tuning);
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
            }
            Acceptor::Online { incoming, .. } => match incoming.try_recv() {
                Ok(stream) => {
                    let (canonical, stop) = (Arc::clone(&canonical), Arc::clone(stop));
                    let (storage, tuning) = (resources.1.clone(), tuning.clone());
                    match std::thread::Builder::new().name("suhina-online-peer".into()).spawn(move || {
                        let _ = serve_link(tuning.wrap(stream), &canonical, &stop, storage.as_deref(), tuning.grace);
                    }) {
                        Ok(handle) => handlers.push(handle),
                        Err(_) => return Err("could not accept an online guest".into()),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Err("online listener stopped".into()),
            },
        }
        handlers.retain(|h| !h.is_finished());
        std::thread::sleep(Duration::from_millis(25));
    }
    // Give each guest's handler a moment to say the host left before the endpoint closes.
    let farewell = Instant::now() + FAREWELL + FAREWELL;
    while handlers.iter().any(|h| !h.is_finished()) && Instant::now() < farewell {
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

/// Opens the short-code lobby of an online host; the view gets the code once it is reachable.
fn open_lobby(acceptor: &Acceptor, invite: String, events: &SyncSender<Event>) -> Option<Lobby> {
    let Acceptor::Online { net, .. } = acceptor else { return None };
    let events = events.clone();
    Some(net.open_lobby(invite, move |code| {
        let _ = events.try_send(match code {
            Ok(code) => Event::ShortCode(code),
            Err(error) => Event::Rejected(format!("no short code ({error}); share the full invitation instead")),
        });
    }))
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
    (track, start): (u32, f64),
    audio: &Arc<suhina_engine::AudioData>,
    storage: (Option<&std::path::Path>, &AtomicBool),
) -> Result<(), String> {
    let asset = assets::encode_take(audio, storage.0, storage.1)?;
    let exists = {
        let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        state.check_take(Host::owner(), sequence, base, (track, start), &asset.meta)?;
        state.assets.iter().any(|a| a.meta.hash == asset.meta.hash)
    };
    let collected =
        if exists { None } else { Some(assets::collect_take(&asset, Arc::clone(audio), storage.0, storage.1)?) };
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
    io: &mut impl Link,
    response: &ServerMessage,
    project: &Project,
    assets: &[Arc<assets::Encoded>],
    known: &mut assets::Known,
    stop: &AtomicBool,
) -> Result<(), String> {
    protocol::write(io, response)?;
    io.set_deadline(Instant::now() + Duration::from_secs(180));
    assets::send(io, &assets::missing(project, known)?, assets, stop)?;
    for a in assets.iter().filter(|a| assets::references(project).contains(&a.meta.path())) {
        known.insert(a.meta.path(), a.meta.clone());
    }
    Ok(())
}
fn receive_update(
    io: &mut impl Link,
    project: &Project,
    revision: u64,
    root: Option<&std::path::Path>,
    known: &mut assets::Known,
    stop: &AtomicBool,
    events: &SyncSender<Event>,
) -> Result<Option<AudioAddition>, String> {
    io.set_deadline(Instant::now() + Duration::from_secs(180));
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
    io: &mut impl Link,
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
        io.set_deadline(Instant::now() + Duration::from_secs(180));
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
    tuning: &Tuning,
) -> Result<(), String> {
    let connection = ServerConnection::new(tls).map_err(|e| e.to_string())?;
    let link = tuning.wrap(StreamOwned::new(connection, DeadlineSocket::new(stream)));
    serve_link(link, canonical, stop, storage, tuning.grace)
}
fn serve_link(
    io: impl Link,
    canonical: &Mutex<Canonical>,
    stop: &AtomicBool,
    storage: Option<&std::path::Path>,
    grace: Duration,
) -> Result<(), String> {
    let mut io = Watch::new(io);
    let hello: Value = protocol::read(&mut io)?;
    let opening = canonical.lock().unwrap_or_else(PoisonError::into_inner).open(hello, Instant::now());
    let (peer, generation, resume, answer) = match opening {
        Ok(Opening::Admitted { peer, generation, token }) => {
            (peer, generation, None, ServerMessage::Admitted { protocol: PROTOCOL, token })
        }
        Ok(Opening::Resumed { peer, generation, token, resume }) => {
            let copy = resume.is_none();
            (peer, generation, resume, ServerMessage::Resumed { protocol: PROTOCOL, token, copy })
        }
        Err(refusal) => {
            protocol::write(&mut io, &ServerMessage::Denied { refusal: refusal.clone() })?;
            return Err(refusal.message());
        }
    };
    let result = protocol::write(&mut io, &answer)
        .and_then(|()| serve_member(&mut io, canonical, (peer, generation), resume, stop, storage));
    let how = match &result {
        Ok(how) => *how,
        // A broken protocol frees the place; only a failed network keeps it for a resume.
        Err(_) if !io.failed() => Departure::Left,
        Err(_) => Departure::Dropped,
    };
    canonical.lock().unwrap_or_else(PoisonError::into_inner).depart(peer, generation, how, Instant::now(), grace);
    result.map(|_| ())
}
/// What the host sent a guest with its copy: the sounds it holds, and the sounds the copy offered
/// (in manifest order), which the guest may still fetch in the background on this connection.
type Copy = (assets::Known, Vec<Arc<assets::Encoded>>);
/// Sends a guest the whole project: the portable copy, its sounds and a first state.
fn send_copy(io: &mut impl Link, canonical: &Mutex<Canonical>, stop: &AtomicBool) -> Result<Copy, String> {
    io.set_deadline(Instant::now() + Duration::from_secs(30));
    let (snapshot, revision, assets) = {
        let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        (state.host.project().clone(), state.host.revision(), state.assets.clone())
    };
    let (_, document) = crate::midi::encode_portable(&snapshot)?;
    crate::transfer::send(io, &document, revision, stop)?;
    io.set_deadline(Instant::now() + Duration::from_secs(180));
    let offered = assets::send(io, &snapshot, &assets, stop)?;
    let mut known: assets::Known = assets
        .iter()
        .filter(|a| assets::references(&snapshot).contains(&a.meta.path()))
        .map(|a| (a.meta.path(), a.meta.clone()))
        .collect();
    io.renew();
    let (initial, project, assets) = {
        let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        (state.response(revision, None)?, state.host.project().clone(), state.assets.clone())
    };
    write_update(io, &initial, &project, &assets, &mut known, stop)?;
    Ok((known, offered))
}
/// Serves one admitted or resumed guest until it leaves, its link drops or the host stops.
/// `resume` holds what a resumed guest has and its revision; without it the guest gets a copy.
fn serve_member(
    io: &mut impl Link,
    canonical: &Mutex<Canonical>,
    member: (PeerId, u64),
    resume: Option<(assets::Known, u64)>,
    stop: &AtomicBool,
    storage: Option<&std::path::Path>,
) -> Result<Departure, String> {
    let (peer, generation) = member;
    // Background slices index the copy's manifest, so only a connection that sent a copy serves
    // them; a resumed guest still missing sounds asks for a fresh copy instead.
    let (mut known, offered) = match resume {
        None => send_copy(io, canonical, stop)?,
        Some((mut known, seen)) => {
            io.renew();
            let (response, project, assets) = {
                let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
                (state.response(seen, None)?, state.host.project().clone(), state.assets.clone())
            };
            write_update(io, &response, &project, &assets, &mut known, stop)?;
            (known, Vec::new())
        }
    };
    let mut budget = assets::background_budget(&offered);
    canonical.lock().unwrap_or_else(PoisonError::into_inner).remember(peer, generation, &known);
    loop {
        if stop.load(Ordering::Relaxed) {
            // Tell the guest the host left on purpose, so it does not try to come back.
            io.set_deadline(Instant::now() + FAREWELL);
            if protocol::write(io, &ServerMessage::Ended { protocol: PROTOCOL }).is_ok() {
                // Wait for the guest's next request, so the link stays open until it has read why.
                let _ = protocol::read::<Value>(io);
            }
            return Ok(Departure::Left);
        }
        if !canonical.lock().unwrap_or_else(PoisonError::into_inner).current(peer, generation) {
            return Ok(Departure::Superseded);
        }
        io.renew();
        let message: ClientMessage = protocol::read(io)?;
        if matches!(message, ClientMessage::Leave { protocol: PROTOCOL }) {
            return Ok(Departure::Left);
        }
        if let ClientMessage::Fetch { protocol: PROTOCOL, asset, offset, tree, .. } = &message {
            if offered.is_empty() {
                return Err("no background sounds on this connection".into());
            }
            assets::serve_slice(io, &offered, (*asset, *offset, *tree), &mut budget, stop)?;
        }
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
                    io,
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
            if !state.current(peer, generation) {
                return Ok(Departure::Superseded);
            }
            let (seen, outcome) = match (upload, &message) {
                (Some((seen, result)), _) => (seen, Some(result)),
                (None, ClientMessage::Poll { protocol: PROTOCOL, seen_revision })
                | (None, ClientMessage::Fetch { protocol: PROTOCOL, seen_revision, .. }) => (*seen_revision, None),
                (None, ClientMessage::Edit { protocol: PROTOCOL, sequence, base_revision, seen_revision, edit })
                    if *sequence > 0 =>
                {
                    // Validate delivery cursor before allowing a mutation.
                    state.since(*seen_revision)?;
                    (*seen_revision, Some(state.submit(peer, *sequence, *base_revision, edit.clone())))
                }
                (None, ClientMessage::Transport { protocol: PROTOCOL, seen_revision, request }) => {
                    state.since(*seen_revision)?;
                    (*seen_revision, Some(state.linked.apply(*request)))
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
        write_update(io, &response, &project, &assets, &mut known, stop)?;
        canonical.lock().unwrap_or_else(PoisonError::into_inner).remember(peer, generation, &known);
        // Rate bound also applies to a guest bypassing our client's polling cadence.
        std::thread::sleep(Duration::from_millis(50));
    }
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
                let asset = assets::encode_take(&audio, root.as_deref(), &stop)?;
                let collected = assets::collect_take(&asset, Arc::clone(&audio), root.as_deref(), &stop)?;
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
    tuning: &Tuning,
    stop: &Arc<AtomicBool>,
    sockets: &Mutex<Vec<TcpStream>>,
    inputs: Receiver<Input>,
    events: &SyncSender<Event>,
) -> Result<(), String> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(invitation.der()?)).map_err(|_| "invalid invitation certificate")?;
    let tls = Arc::new(
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let name = ServerName::try_from("suhina.local").map_err(|_| "invalid server name")?;
    let endpoint = invitation.endpoint;
    let dial = || -> Result<Box<dyn Link + Send>, String> {
        // A dropped link's socket is closed before the next attempt.
        for old in sockets.lock().unwrap_or_else(PoisonError::into_inner).drain(..) {
            let _ = old.shutdown(Shutdown::Both);
        }
        let stream =
            TcpStream::connect_timeout(&endpoint, TIMEOUT).map_err(|e| format!("could not reach the LAN host: {e}"))?;
        socket(&stream)?;
        register(&stream, sockets)?;
        let connection = ClientConnection::new(Arc::clone(&tls), name.clone()).map_err(|e| e.to_string())?;
        Ok(tuning.wrap(StreamOwned::new(connection, DeadlineSocket::new(stream))))
    };
    guest_link(dial, invitation.secret, storage, tuning, stop, inputs, events)
}
fn online_guest(
    invitation: OnlineInvitation,
    net: Option<Net>,
    storage: Option<PathBuf>,
    tuning: &Tuning,
    stop: &Arc<AtomicBool>,
    inputs: Receiver<Input>,
    events: &SyncSender<Event>,
) -> Result<(), String> {
    // The guest's own endpoint reaches the host by its key, directly when possible, else by relay.
    let reach = if invitation
        .endpoint
        .addrs
        .iter()
        .all(|a| matches!(a, iroh::TransportAddr::Ip(ip) if ip.ip().is_loopback()))
    {
        Reach::Loopback
    } else {
        Reach::Internet
    };
    // A short code's lobby endpoint is reused; a long code binds one.
    let net = match net {
        Some(net) => net,
        None => Net::bind(reach)?,
    };
    let endpoint = invitation.endpoint;
    let dial = || net.connect(endpoint.clone()).map(|stream| tuning.wrap(stream));
    let result = guest_link(dial, invitation.secret, storage, tuning, stop, inputs, events);
    drop(net);
    result
}

/// The guest's verified copy of the shared project, kept across reconnections.
struct Mirror {
    session: suhina_session::Session,
    revision: u64,
    known: assets::Known,
    previous: Controls,
    previous_guests: usize,
    /// Set while an update is half applied: a resume then asks for a fresh copy.
    dirty: bool,
    /// Sounds of a heavy copy still arriving in the background. Their slices refer to the
    /// copy's manifest, which only the connection that sent it knows, so a resume with sounds
    /// pending asks for a fresh copy (cheap: the sound cache keeps what already arrived).
    background: Option<assets::Background>,
}
impl Mirror {
    /// The revision a resume can continue from, or `None` to ask for a fresh copy.
    fn resumable(&self) -> Option<u64> {
        (!self.dirty && self.background.is_none()).then_some(self.revision)
    }
}
/// A request sent and not yet answered. After a resume it is sent again with the same sequence,
/// which the host's retry cache applies at most once.
enum Flight {
    Edit { base: u64, edit: WireEdit },
    Publish(PreparedTake),
    Transport(TransportRequest),
}
/// The guest's own progress, kept across reconnections.
struct Progress {
    sequence: u64,
    pending: Option<PendingTake>,
    flight: Option<Flight>,
    /// Linked playback: the host clock estimated from round trips, and what the view last got.
    local: crate::linked::Clock,
    estimate: ClockEstimate,
    shown: Option<(Option<SharedTransport>, i64)>,
}
/// Why a guest's exchanges ended without an error.
enum GuestEnd {
    /// This computer left the session.
    Stopped,
    /// The host ended the session on purpose.
    Ended,
}
const LOST: &str = "Lost the connection to the host and could not reconnect. Your copy of the project is kept; ask the host for a new invitation code to rejoin.";

fn hello_fields() -> Result<(u16, u16, String), String> {
    Ok((PROTOCOL, crate::protocol::COPY_FORMAT, protocol::engine_revision()?))
}
fn guest_link<L: Link>(
    mut dial: impl FnMut() -> Result<L, String>,
    secret: String,
    storage: Option<PathBuf>,
    tuning: &Tuning,
    stop: &Arc<AtomicBool>,
    inputs: Receiver<Input>,
    events: &SyncSender<Event>,
) -> Result<(), String> {
    let (protocol, format, engine) = hello_fields()?;
    let mut io = Watch::new(dial()?);
    protocol::write(&mut io, &ClientMessage::Hello { protocol, format, engine: engine.clone(), secret })?;
    io.set_deadline(Instant::now() + Duration::from_secs(30));
    let mut token = match protocol::read_answer(&mut io)? {
        ServerMessage::Admitted { protocol: PROTOCOL, token } => token,
        ServerMessage::Denied { refusal } => return Err(refusal.message()),
        _ => return Err("invalid host answer".into()),
    };
    let mut mirror: Option<Mirror> = None;
    let mut progress = Progress {
        sequence: 1,
        pending: None,
        flight: None,
        local: crate::linked::Clock::new(),
        estimate: ClockEstimate::default(),
        shown: None,
    };
    let context = (storage.as_deref(), stop, &inputs, events);
    let mut copy = true;
    let mut resumed = false;
    loop {
        match guest_round(&mut io, &mut mirror, &mut progress, (copy, resumed), context) {
            Ok(GuestEnd::Stopped) => {
                io.set_deadline(Instant::now() + FAREWELL);
                let _ = protocol::write(&mut io, &ClientMessage::Leave { protocol: PROTOCOL });
                let _ = events.try_send(Event::Closed);
                return Ok(());
            }
            Ok(GuestEnd::Ended) => {
                let _ = events.try_send(Event::Ended);
                return Ok(());
            }
            Err(_) if io.failed() && !stop.load(Ordering::Relaxed) => {}
            Err(error) => return Err(error),
        }
        // The link dropped: come back as the same participant, without a new code.
        let _ = events.try_send(Event::Reconnecting);
        let mut backoff = Backoff::new(tuning.retry);
        let (link, fresh) = loop {
            if stop.load(Ordering::Relaxed) {
                let _ = events.try_send(Event::Closed);
                return Ok(());
            }
            let Some(wait) = backoff.wait() else {
                return Err(LOST.into());
            };
            pause(wait, stop);
            let Ok(link) = dial() else {
                continue;
            };
            let mut link = Watch::new(link);
            let seen_revision = mirror.as_ref().and_then(Mirror::resumable);
            let hello =
                ClientMessage::Resume { protocol, format, engine: engine.clone(), token: token.clone(), seen_revision };
            match protocol::write(&mut link, &hello).and_then(|()| protocol::read_answer(&mut link)) {
                Ok(ServerMessage::Resumed { protocol: PROTOCOL, token: next, copy }) => {
                    token = next;
                    break (link, copy);
                }
                Ok(ServerMessage::Denied { refusal }) => return Err(refusal.message()),
                Ok(_) => return Err("invalid host answer".into()),
                Err(_) if link.failed() && !backoff.spent() => {}
                Err(_) if link.failed() => return Err(LOST.into()),
                Err(error) => return Err(error),
            }
        };
        io = link;
        if !fresh && mirror.as_ref().is_some_and(|m| m.resumable().is_none()) {
            return Err("the host did not send the fresh copy this guest asked for".into());
        }
        copy = fresh || mirror.is_none();
        resumed = true;
    }
}
/// Sleeps for `wait`, waking early when the session stops.
fn pause(wait: Duration, stop: &AtomicBool) {
    let until = Instant::now() + wait;
    while !stop.load(Ordering::Relaxed) {
        let Some(left) = until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) else {
            return;
        };
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}
type GuestContext<'a> = (Option<&'a std::path::Path>, &'a Arc<AtomicBool>, &'a Receiver<Input>, &'a SyncSender<Event>);
/// One connection's exchanges: a copy first when needed, then polls, edits and publications.
/// `start` says whether a copy comes first and whether this link resumed an earlier one.
fn guest_round(
    io: &mut impl Link,
    mirror: &mut Option<Mirror>,
    progress: &mut Progress,
    start: (bool, bool),
    context: GuestContext<'_>,
) -> Result<GuestEnd, String> {
    let (copy, resumed) = start;
    if copy {
        // An unfinished background download lets go of its partial sound before the new copy.
        if let Some(old) = mirror.as_mut() {
            old.background = None;
        }
        *mirror = Some(receive_copy(io, context.0, context.1, context.3)?);
    }
    let mirror = mirror.as_mut().ok_or("the shared copy is missing")?;
    // A link resumed without a copy starts with the host's state since our revision.
    exchange(io, mirror, progress, (resumed && !copy, resumed), context)
}
/// Receives the whole shared project and tells the view.
fn receive_copy(
    io: &mut impl Link,
    storage: Option<&std::path::Path>,
    stop: &Arc<AtomicBool>,
    events: &SyncSender<Event>,
) -> Result<Mirror, String> {
    io.set_deadline(Instant::now() + Duration::from_secs(30));
    let offer: ServerMessage = protocol::read(io)?;
    let (document, mut revision) = crate::transfer::receive(io, offer, stop, |done, total| {
        let _ = events.try_send(Event::Receiving(done, total));
    })?;
    let project = crate::midi::decode_portable(&document)?;
    io.set_deadline(Instant::now() + Duration::from_secs(180));
    let mut snapshot = assets::receive_initial(io, project, revision, storage, stop, |done, total| {
        let _ = events.try_send(Event::Receiving(done, total));
    })?;
    // Sounds of a heavy initial copy that arrive after the project opened.
    let background = snapshot.take_background();
    let mut known: assets::Known = snapshot.metas.iter().map(|a| (a.path(), a.clone())).collect();
    let mut session = suhina_session::Session::new(snapshot.project.clone());
    io.renew();
    let first: ServerMessage = protocol::read(io)?;
    let ServerMessage::State {
        protocol: PROTOCOL,
        controls,
        operations,
        guests,
        accepted: None,
        error: None,
        host_ms: _,
        transport: _,
    } = first
    else {
        return Err("invalid host response".into());
    };
    protocol::validate_guest_count(guests)?;
    let controls = *controls;
    protocol::apply_operations(&mut session, &mut revision, &controls, &operations)?;
    if let Some(audio) = receive_update(io, session.project(), revision, storage, &mut known, stop, events)? {
        snapshot.add_audio(audio);
    }
    snapshot.project = session.project().clone();
    snapshot.revision = revision;
    let mirror = Mirror {
        session,
        revision,
        known,
        previous: controls.clone(),
        previous_guests: guests,
        dirty: false,
        background,
    };
    events.try_send(Event::Connected(controls, Box::new(snapshot), guests)).map_err(|_| "multiplayer view closed")?;
    Ok(mirror)
}
/// Polls, edits and publications over one link until it fails, the guest stops or the host ends.
/// `start.0`: the first message is a state the host sends unasked (a resume);
/// `start.1`: tell the view once the first state is through (after a reconnection).
fn exchange(
    io: &mut impl Link,
    mirror: &mut Mirror,
    progress: &mut Progress,
    start: (bool, bool),
    context: GuestContext<'_>,
) -> Result<GuestEnd, String> {
    let (storage, stop, inputs, events) = context;
    let (mut unasked, mut announce) = start;
    while !stop.load(Ordering::Relaxed) {
        io.renew();
        if !unasked && progress.flight.is_none() {
            let input = if progress.pending.is_none() { inputs.try_recv().ok() } else { None };
            match input {
                Some(Input::Publish { base, track, start, audio }) => {
                    let root = storage.map(std::path::Path::to_path_buf);
                    progress.pending = Some(prepare_take(base, track, start, audio, root, Arc::clone(stop))?);
                }
                Some(Input::Edit { base, edit }) => progress.flight = Some(Flight::Edit { base, edit }),
                Some(Input::Transport(request)) => progress.flight = Some(Flight::Transport(request)),
                Some(Input::RenewInvitation) => return Err("only the host can renew invitations".into()),
                None => {}
            }
            match progress.pending.as_ref().map(|p| p.ready.try_recv()) {
                Some(Ok(Ok(take))) => {
                    progress.pending = None;
                    progress.flight = Some(Flight::Publish(take));
                }
                Some(Ok(Err(error))) => {
                    progress.pending = None;
                    events.try_send(Event::Upload(None)).map_err(|_| "multiplayer view closed")?;
                    events.try_send(Event::Rejected(error)).map_err(|_| "multiplayer view closed")?;
                }
                Some(Err(mpsc::TryRecvError::Disconnected)) => return Err("take preparation stopped".into()),
                _ => {}
            }
        }
        let seen_revision = mirror.revision;
        // Background sounds take the place of an idle poll, never of a request.
        let fetch = match mirror.background.as_mut() {
            Some(sounds) if !unasked && progress.flight.is_none() => sounds.request(seen_revision, stop)?,
            _ => None,
        };
        let fetching = fetch.is_some();
        let mut arrived = Vec::new();
        let sent_ms = progress.local.now_ms();
        let response = if unasked {
            protocol::read(io)?
        } else {
            let request = match &progress.flight {
                Some(Flight::Publish(take)) => ClientMessage::Publish {
                    protocol: PROTOCOL,
                    sequence: progress.sequence,
                    base_revision: take.base,
                    seen_revision,
                    track: take.track,
                    start: take.start,
                    asset: take.asset.meta.clone(),
                },
                Some(Flight::Edit { base, edit }) => ClientMessage::Edit {
                    protocol: PROTOCOL,
                    sequence: progress.sequence,
                    base_revision: *base,
                    seen_revision,
                    edit: edit.clone(),
                },
                Some(Flight::Transport(request)) => {
                    ClientMessage::Transport { protocol: PROTOCOL, seen_revision, request: *request }
                }
                None => fetch.unwrap_or(ClientMessage::Poll { protocol: PROTOCOL, seen_revision }),
            };
            protocol::write(io, &request)?;
            if let Some(Flight::Publish(take)) = &progress.flight {
                io.set_deadline(Instant::now() + Duration::from_secs(180));
                assets::send_take(io, &take.asset, stop, |done, total| {
                    let _ = events.try_send(Event::Upload(Some((done, total))));
                })?
            } else {
                if fetching
                    && let Some(sounds) = mirror.background.as_mut()
                    && let Some(sound) = sounds.read(io, stop)?
                {
                    arrived.push(sound);
                }
                protocol::read(io)?
            }
        };
        let received_ms = progress.local.now_ms();
        if matches!(response, ServerMessage::Ended { protocol: PROTOCOL }) {
            return Ok(GuestEnd::Ended);
        }
        let ServerMessage::State {
            protocol: PROTOCOL,
            controls,
            operations,
            guests,
            accepted,
            error,
            host_ms,
            transport,
        } = response
        else {
            return Err("invalid host response".into());
        };
        protocol::validate_guest_count(guests)?;
        if let Some(shared) = &transport {
            shared.validate()?;
        }
        // The answer settles the request: count an accepted edit and show a refusal now, so a
        // link lost while the update arrives cannot repeat or hide either.
        let flight = if unasked { None } else { progress.flight.take() };
        let mut publication = None;
        match flight {
            Some(Flight::Edit { .. } | Flight::Publish(_)) if accepted == Some(true) => {
                progress.sequence = progress.sequence.checked_add(1).ok_or("operation counter exhausted")?;
                if let Some(Flight::Publish(take)) = flight {
                    publication = Some(take);
                }
            }
            Some(Flight::Transport(_)) if accepted == Some(true) => {}
            Some(flight) => {
                let fallback = match flight {
                    Flight::Transport(_) => "the host refused this playback change",
                    Flight::Edit { .. } | Flight::Publish(_) => "the host refused this edit",
                };
                if accepted.is_none() {
                    return Err("host did not acknowledge the request".into());
                }
                let publication = matches!(flight, Flight::Publish(_));
                // A refused take's local preparation is cleaned up before the view hears of it.
                drop(flight);
                if publication {
                    events.try_send(Event::Upload(None)).map_err(|_| "multiplayer view closed")?;
                }
                events
                    .try_send(Event::Rejected(error.unwrap_or_else(|| fallback.into())))
                    .map_err(|_| "multiplayer view closed")?;
            }
            None => {}
        }
        // An upload's round trip includes the sound transfer and says nothing about the clock.
        if !unasked && publication.is_none() && !fetching {
            progress.estimate.add(sent_ms, received_ms, host_ms);
        }
        let controls = *controls;
        let mut audio = arrived;
        mirror.dirty = true;
        if let Some(take) = &publication {
            let expected = take_edit((take.track, take.start), &take.asset.meta);
            let confirmed =
                operations.iter().any(|op| Some(op.revision) == take.base.checked_add(1) && op.edit == expected);
            // A take sent again after a resume may already be in the copy this guest holds.
            let held = take.base < mirror.revision;
            if !confirmed && !held {
                return Err("host did not confirm the offered take operation".into());
            }
        }
        if let Some(take) = publication {
            if assets::remember_take(&mut mirror.known, &take.asset.meta)? {
                audio.push(take.audio);
            }
            events.try_send(Event::Upload(None)).map_err(|_| "multiplayer view closed")?;
        }
        protocol::apply_operations(&mut mirror.session, &mut mirror.revision, &controls, &operations)?;
        if let Some(addition) =
            receive_update(io, mirror.session.project(), mirror.revision, storage, &mut mirror.known, stop, events)?
        {
            audio.push(addition);
        }
        let previous = &mirror.previous;
        if controls.revision < previous.revision || controls.revision == previous.revision && controls != *previous {
            return Err("host control revision mismatch".into());
        }
        if let Some(offset) = progress.estimate.offset() {
            // A better estimate moves the schedule; a millisecond or two is not worth an event.
            let changed =
                progress.shown.is_none_or(|(old, old_offset)| old != transport || old_offset.abs_diff(offset) > 2);
            if changed
                && events
                    .try_send(Event::Transport(transport, LocalClock { epoch: progress.local.epoch(), offset }))
                    .is_ok()
            {
                progress.shown = Some((transport, offset));
            }
        }
        if controls != mirror.previous || guests != mirror.previous_guests || !audio.is_empty() {
            events
                .try_send(Event::State(controls.clone(), operations, guests, audio))
                .map_err(|_| "multiplayer view is not keeping up")?;
            mirror.previous = controls;
            mirror.previous_guests = guests;
        }
        mirror.dirty = false;
        unasked = false;
        if announce && events.try_send(Event::Resumed).is_ok() {
            announce = false;
        }
        // A State clears the view's progress; background sounds report theirs after it.
        match &mirror.background {
            Some(sounds) if sounds.is_empty() => mirror.background = None,
            Some(sounds) => {
                let (done, total) = sounds.progress();
                let _ = events.try_send(Event::Receiving(done, total));
            }
            None => {}
        }
        if mirror.background.is_none() {
            std::thread::sleep(POLL);
        }
    }
    Ok(GuestEnd::Stopped)
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
