// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex, PoisonError,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use suhina_collaboration::{Host, Request, Role};
use suhina_model::Project;
use suhina_session::{Agents, CommandError, CommandKind, CommandSpec, ParamType, Policy};

use crate::invitation::{Invitation, new_secret, private, secret_hash};
use crate::protocol::{self, ClientMessage, Controls, PROTOCOL, ServerMessage, WireEdit};

const TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);

struct Canonical {
    host: Host,
    secret: [u8; 32],
    expires: Instant,
    redeemed: bool,
    guest: bool,
}
impl Canonical {
    fn admit(&mut self, secret: &str, now: Instant) -> Result<suhina_collaboration::PeerId, String> {
        if secret.len() != 64
            || !bool::from(self.secret.ct_eq(&secret_hash(secret)))
            || self.redeemed
            || now >= self.expires
        {
            return Err("admission refused".into());
        }
        let peer = self.host.add_peer(Role::Editor).map_err(|e| e.to_string())?;
        self.redeemed = true;
        self.guest = true;
        Ok(peer)
    }
}
enum Event {
    Ready(String, Controls, Box<Project>),
    Connected(Controls, Box<Project>),
    Receiving(usize, usize),
    State(Controls, bool),
    Rejected(String),
    Failed(String),
    Closed,
}
enum Input {
    Edit { base: u64, edit: WireEdit },
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
    guest: bool,
    selected: Option<Project>,
    project: Option<Project>,
    receiving: Option<(usize, usize)>,
}

impl Service {
    /// Trusted local selection only; remote inputs must use the bounded MIDI decoder instead.
    pub fn select_project(&mut self, project: &Project) -> Result<(), CommandError> {
        if self.active() {
            return Err("leave the current session first".into());
        }
        self.selected = None;
        crate::midi::validate(project).map_err(CommandError::from)?;
        self.selected = Some(project.clone());
        Ok(())
    }
    /// The verified initial music, consumed once by the desktop when it creates a new tab.
    pub fn take_project(&mut self) -> Option<Project> {
        self.project.take()
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
                        self.receiving = None;
                        self.mode = "hosting".into();
                        self.invitation = Some(invitation);
                        self.controls = Some(controls);
                    }
                    Ok(Event::Connected(controls, project)) => {
                        self.project = Some(*project);
                        self.receiving = None;
                        self.mode = "connected".into();
                        self.controls = Some(controls);
                    }
                    Ok(Event::Receiving(done, total)) => self.receiving = Some((done, total)),
                    Ok(Event::State(controls, guest)) => {
                        self.controls = Some(controls);
                        self.guest = guest;
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
            self.running = None;
            self.invitation = None;
            self.guest = false;
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
        self.guest = false;
        self.project = None;
        self.receiving = None;
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
        label: "Host a MIDI copy of the selected local project",
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
        id: "multiplayer.edit",
        label: "Submit a starter track control",
        params: &[
            param("base_revision", ParamType::Integer, "last observed revision"),
            param("edit", ParamType::Text, "typed edit as JSON text"),
        ],
        kind: CommandKind::Projects,
        run: edit,
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
        json!({"mode":s.mode,"controls":s.controls,"invitation":s.invitation,"error":s.error,"guest_connected":s.guest,"receiving":s.receiving}),
    )
}
fn stop(s: &mut Service, _: &Value) -> Result<Value, CommandError> {
    s.running = None;
    s.invitation = None;
    s.mode = "idle".into();
    s.guest = false;
    s.selected = None;
    s.project = None;
    s.receiving = None;
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
    start_source(s, address(p)?, crate::starter_project())
}
fn start_selected_host(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let address = address(p)?;
    let source = s.selected.take().ok_or("select a local MIDI project first")?;
    start_source(s, address, source)
}
fn start_source(s: &mut Service, address: SocketAddr, source: Project) -> Result<Value, CommandError> {
    s.start(move |stop, sockets, inputs, events| {
        if let Err(e) = host(address, source, &stop, &sockets, inputs, &events) {
            let _ = events.try_send(Event::Failed(e));
        }
    })
}
fn start_guest(s: &mut Service, p: &Value) -> Result<Value, CommandError> {
    let invitation =
        Invitation::decode(p["invitation"].as_str().ok_or("invitation must be text")?).map_err(CommandError::from)?;
    s.start(move |stop, sockets, inputs, events| {
        if let Err(e) = guest(invitation, &stop, &sockets, inputs, &events)
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
    stop: &Arc<AtomicBool>,
    sockets: &Arc<Mutex<Vec<TcpStream>>>,
    inputs: Receiver<Input>,
    events: &SyncSender<Event>,
) -> Result<(), String> {
    let (source, document) = crate::midi::encode(&source)?;
    let document = Arc::new(document);
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
    let invite = Invitation {
        protocol: PROTOCOL,
        endpoint: listener.local_addr().map_err(|e| e.to_string())?,
        certificate: URL_SAFE_NO_PAD.encode(der.as_ref()),
        secret: secret.clone(),
    }
    .encode()?;
    let host = Host::new(source.clone()).map_err(|e| e.to_string())?;
    let initial = Controls::from_project(host.project(), 0)?;
    let canonical = Arc::new(Mutex::new(Canonical {
        host,
        secret: secret_hash(&secret),
        expires: Instant::now() + Duration::from_secs(300),
        redeemed: false,
        guest: false,
    }));
    events.try_send(Event::Ready(invite, initial, Box::new(source))).map_err(|_| "multiplayer view closed")?;
    let pending = Arc::new(AtomicUsize::new(0));
    let mut handlers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let mut sequence = 1u64;
    let mut last = (0u64, false);
    while !stop.load(Ordering::Relaxed) {
        while let Ok(Input::Edit { base, edit }) = inputs.try_recv() {
            let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
            let request = Request { sequence, base_revision: base, edit: edit.edit()? };
            match state.host.submit(Host::owner(), request) {
                Ok(_) => sequence = sequence.checked_add(1).ok_or("operation counter exhausted")?,
                Err(e) => {
                    let _ = events.try_send(Event::Rejected(e.to_string()));
                }
            }
        }
        {
            let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
            let current = (state.host.revision(), state.guest);
            if current != last
                && events
                    .try_send(Event::State(Controls::from_project(state.host.project(), current.0)?, current.1))
                    .is_ok()
            {
                last = current;
            }
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
                let document = Arc::clone(&document);
                pending.fetch_add(1, Ordering::Relaxed);
                match std::thread::Builder::new().name("suhina-lan-peer".into()).spawn(move || {
                    let _guard = ConnectionGuard { sockets: registry, peer: stream.peer_addr().ok() };
                    let _ = serve(stream, tls, &canonical, &stop, &document);
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

fn serve(
    stream: TcpStream,
    tls: Arc<ServerConfig>,
    canonical: &Mutex<Canonical>,
    stop: &AtomicBool,
    document: &[u8],
) -> Result<(), String> {
    let connection = ServerConnection::new(tls).map_err(|e| e.to_string())?;
    let mut io = StreamOwned::new(connection, DeadlineSocket::new(stream));
    let hello: ClientMessage = protocol::read(&mut io)?;
    let peer = {
        let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
        let result = match hello {
            ClientMessage::Hello { protocol: PROTOCOL, format: 1, secret } => state.admit(&secret, Instant::now()),
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
        crate::transfer::send(&mut io, document, stop)?;
        io.sock.renew();
        let initial = {
            let state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
            Controls::from_project(state.host.project(), state.host.revision())?
        };
        protocol::write(
            &mut io,
            &ServerMessage::State { protocol: PROTOCOL, controls: Box::new(initial), accepted: None, error: None },
        )?;
        while !stop.load(Ordering::Relaxed) {
            io.sock.renew();
            let message: ClientMessage = protocol::read(&mut io)?;
            let response = {
                let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
                let outcome = match message {
                    ClientMessage::Poll { protocol } if protocol == PROTOCOL => None,
                    ClientMessage::Edit { protocol, sequence, base_revision, edit }
                        if protocol == PROTOCOL && sequence > 0 =>
                    {
                        Some(state.host.submit(peer, Request { sequence, base_revision, edit: edit.edit()? }))
                    }
                    _ => return Err("unsupported LAN protocol message".into()),
                };
                let accepted = outcome.as_ref().map(Result::is_ok);
                let error = outcome.and_then(Result::err).map(|e| e.to_string());
                ServerMessage::State {
                    protocol: PROTOCOL,
                    controls: Box::new(Controls::from_project(state.host.project(), state.host.revision())?),
                    accepted,
                    error,
                }
            };
            protocol::write(&mut io, &response)?;
            // Rate bound also applies to a guest bypassing our client's polling cadence.
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    })();
    let mut state = canonical.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = state.host.remove_peer(peer);
    state.guest = false;
    result
}

fn guest(
    invitation: Invitation,
    stop: &AtomicBool,
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
    protocol::write(&mut io, &ClientMessage::Hello { protocol: PROTOCOL, format: 1, secret: invitation.secret })?;
    io.sock.deadline = Instant::now() + Duration::from_secs(30);
    let offer: ServerMessage = protocol::read(&mut io)?;
    let document = crate::transfer::receive(&mut io, offer, stop, |done, total| {
        let _ = events.try_send(Event::Receiving(done, total));
    })?;
    let project = crate::midi::decode(&document)?;
    io.sock.renew();
    let first: ServerMessage = protocol::read(&mut io)?;
    let ServerMessage::State { protocol: PROTOCOL, controls, .. } = first else {
        return Err("invitation refused, expired or already used".into());
    };
    let controls = *controls;
    controls.changes(&project)?;
    let mut previous = controls.clone();
    events.try_send(Event::Connected(controls, Box::new(project.clone()))).map_err(|_| "multiplayer view closed")?;
    let mut sequence = 1u64;
    while !stop.load(Ordering::Relaxed) {
        io.sock.renew();
        let input = inputs.try_recv().ok();
        let edited = input.is_some();
        let request = match input {
            Some(Input::Edit { base, edit }) => {
                ClientMessage::Edit { protocol: PROTOCOL, sequence, base_revision: base, edit }
            }
            None => ClientMessage::Poll { protocol: PROTOCOL },
        };
        protocol::write(&mut io, &request)?;
        let response: ServerMessage = protocol::read(&mut io)?;
        let ServerMessage::State { protocol: PROTOCOL, controls, accepted, error } = response else {
            return Err("invalid host response".into());
        };
        let controls = *controls;
        controls.changes(&project)?;
        if controls.revision < previous.revision || controls.revision == previous.revision && controls != previous {
            return Err("host control revision mismatch".into());
        }
        if edited {
            match accepted {
                Some(true) => sequence = sequence.checked_add(1).ok_or("operation counter exhausted")?,
                Some(false) => {
                    let _ =
                        events.try_send(Event::Rejected(error.unwrap_or_else(|| "the host refused this edit".into())));
                }
                None => return Err("host did not acknowledge the edit".into()),
            }
        }
        if controls != previous {
            events.try_send(Event::State(controls.clone(), true)).map_err(|_| "multiplayer view is not keeping up")?;
            previous = controls;
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
