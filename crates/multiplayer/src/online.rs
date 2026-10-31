// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Internet sessions: authenticated peer-to-peer QUIC through iroh, reachable by an invitation code.
//! No account and no server of ours: iroh punches through home routers and falls back to public
//! relays that only forward end-to-end encrypted traffic.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::address_lookup::MemoryLookup;
use iroh::endpoint::{Connection, Incoming, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::invitation::Invite;
use crate::protocol::PROTOCOL;
use crate::transport::{Link, TIMEOUT};

#[path = "short_code.rs"]
mod short_code;
pub(crate) use short_code::ShortCode;

const PREFIX: &str = "suhina-net://";
const ALPN: &[u8] = b"suhina/multiplayer/18";
/// The lobby that trades a short code for the full invitation speaks only this protocol.
const LOBBY_ALPN: &[u8] = b"suhina/lobby/1";
/// A lobby lives as long as the invitation it hands out.
const LOBBY_LIFE: Duration = crate::transport::CODE_LIFE;
const LOBBY_SLOTS: usize = 8;
const MAX_INVITATION: usize = 8192;
/// Hole punching and relay lookup can take several seconds across the Internet.
const CONNECT: Duration = Duration::from_secs(20);
const ONLINE: Duration = Duration::from_secs(10);
/// A lobby's address record can take a few seconds to appear in the public address lookup.
const LOOKUP_RETRY: Duration = Duration::from_secs(2);
/// How long a closing stream waits for its last bytes to be acknowledged.
const FAREWELL: Duration = Duration::from_millis(500);

/// Where the endpoint looks for peers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    /// Internet: public relays and address lookup, through home routers.
    Internet,
    /// This computer only, without relays or lookup; for tests and two instances side by side.
    Loopback,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OnlineInvitation {
    pub protocol: u16,
    pub endpoint: EndpointAddr,
    pub secret: String,
}
impl std::fmt::Debug for OnlineInvitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OnlineInvitation { redacted }")
    }
}
impl OnlineInvitation {
    pub fn decode(text: &str) -> Result<Self, String> {
        if text.len() > 8192 {
            return Err("invitation is too long".into());
        }
        let body = text.trim().strip_prefix(PREFIX).ok_or("not a Suhina online invitation")?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| "invalid invitation encoding")?;
        crate::protocol::invitation_protocol(&bytes)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| "invalid invitation")?;
        if value.protocol != PROTOCOL
            || value.secret.len() != 64
            || !value.secret.bytes().all(|b| b.is_ascii_hexdigit())
            || value.endpoint.addrs.len() > 32
        {
            return Err("unsupported online invitation".into());
        }
        Ok(value)
    }
    pub fn is_online(text: &str) -> bool {
        text.trim().starts_with(PREFIX)
    }
}
impl Invite for OnlineInvitation {
    fn secret(&self) -> &str {
        &self.secret
    }
    fn set_secret(&mut self, secret: String) {
        self.secret = secret;
    }
    fn encode(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self).map_err(|_| "could not encode invitation")?;
        Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)))
    }
}

/// Where loopback endpoints find each other by id alone: this process only, never published.
/// It stands in for the public address lookup that Internet endpoints use.
fn directory() -> &'static MemoryLookup {
    static DIRECTORY: OnceLock<MemoryLookup> = OnceLock::new();
    DIRECTORY.get_or_init(MemoryLookup::new)
}

/// Opens an endpoint under `key` that accepts `alpn`. On the Internet it publishes its address
/// through iroh's public lookup (pkarr and DNS), so it can be dialled by its id alone.
async fn endpoint(reach: Reach, key: SecretKey, alpn: &[u8]) -> Result<Endpoint, String> {
    let builder = match reach {
        Reach::Loopback => Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .clear_address_lookup()
            .address_lookup(directory().clone())
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .map_err(|e| e.to_string())?,
        Reach::Internet => Endpoint::builder(presets::N0),
    };
    let endpoint = builder
        .secret_key(key)
        .alpns(vec![alpn.to_vec()])
        .bind()
        .await
        .map_err(|e| format!("could not open the network connection: {e}"))?;
    if reach == Reach::Internet {
        // Wait for a home relay so the invitation can be reached from behind any router.
        // A computer that gets none still invites with its direct addresses.
        let _ = tokio::time::timeout(ONLINE, endpoint.online()).await;
    }
    Ok(endpoint)
}

/// One iroh endpoint and the runtime driving it. Owned by a plain (non-runtime) thread.
pub(crate) struct Net {
    runtime: Runtime,
    endpoint: Endpoint,
    reach: Reach,
}
impl std::fmt::Debug for Net {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Net").field("reach", &self.reach).finish_non_exhaustive()
    }
}
impl Net {
    pub fn bind(reach: Reach) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("suhina-online")
            .enable_all()
            .build()
            .map_err(|e| format!("could not start the network runtime: {e}"))?;
        let endpoint = runtime.block_on(endpoint(reach, SecretKey::generate(), ALPN))?;
        Ok(Self { runtime, endpoint, reach })
    }
    pub fn address(&self) -> EndpointAddr {
        self.endpoint.addr()
    }
    /// Accepts at most `slots` connections at once; each arrives once the guest opens its stream.
    pub fn listen(&self, slots: usize) -> mpsc::Receiver<Stream> {
        let (send, incoming) = mpsc::sync_channel(slots);
        let endpoint = self.endpoint.clone();
        let handle = self.runtime.handle().clone();
        let slots = Arc::new(Semaphore::new(slots));
        self.runtime.spawn(async move {
            while let Some(connecting) = endpoint.accept().await {
                let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
                    connecting.refuse();
                    continue;
                };
                let (send, handle) = (send.clone(), handle.clone());
                tokio::spawn(async move {
                    let Ok(Ok(connection)) = tokio::time::timeout(CONNECT, connecting).await else {
                        return;
                    };
                    let Ok(Ok((out, into))) = tokio::time::timeout(CONNECT, connection.accept_bi()).await else {
                        return;
                    };
                    let _ = send.try_send(Stream::new(handle, connection, out, into, Some(permit)));
                });
            }
        });
        incoming
    }
    pub fn connect(&self, address: EndpointAddr) -> Result<Stream, String> {
        let expected = address.id;
        let (connection, out, into) = self.runtime.block_on(async {
            let connection = tokio::time::timeout(CONNECT, self.endpoint.connect(address, ALPN))
                .await
                .map_err(|_| "the host did not answer; check the invitation and try again")?
                .map_err(|e| format!("could not reach the host: {e}"))?;
            let (out, into) = connection.open_bi().await.map_err(|e| format!("could not open the session: {e}"))?;
            Ok::<_, String>((connection, out, into))
        })?;
        // The QUIC handshake authenticates this key; keep the check next to the invitation's pin.
        if connection.remote_id() != expected {
            return Err("reached a computer other than the invitation's host".into());
        }
        Ok(Stream::new(self.runtime.handle().clone(), connection, out, into, None))
    }
    /// Opens, in the background, a lobby that hands `invitation` to the first guest who proves a
    /// fresh short code. `ready` gets the code once the lobby can be reached, or why it cannot;
    /// it is never called after the returned lobby is dropped. Never blocks.
    pub fn open_lobby(&self, invitation: String, ready: impl FnOnce(Result<String, String>) + Send + 'static) -> Lobby {
        let shared = Arc::new(Mutex::new(LobbyState::default()));
        let (state, reach) = (Arc::clone(&shared), self.reach);
        self.runtime.spawn(async move {
            let (endpoint, proof) = match open_lobby(reach).await {
                Err(error) => {
                    if !state.lock().unwrap_or_else(PoisonError::into_inner).closed {
                        ready(Err(error));
                    }
                    return;
                }
                Ok((code, proof, endpoint)) => {
                    let live = {
                        let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
                        if !guard.closed {
                            if reach == Reach::Loopback {
                                directory().set_endpoint_info(endpoint.addr());
                            }
                            guard.endpoint = Some(endpoint.clone());
                            // Under the lock: a lobby dropped from now on never reaches `ready`.
                            ready(Ok(code));
                        }
                        !guard.closed
                    };
                    if !live {
                        endpoint.close().await;
                        return;
                    }
                    (endpoint, proof)
                }
            };
            // Spent or not, the lobby refuses everyone after its first guest, until it expires
            // or the host renews or stops; then nobody can reach it any more.
            let _ = tokio::time::timeout(LOBBY_LIFE, serve_lobby(&endpoint, proof, invitation.into())).await;
            let taken = {
                let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
                guard.closed = true;
                guard.endpoint.take()
            };
            if let Some(endpoint) = taken {
                retire(reach, &endpoint);
                endpoint.close().await;
            }
        });
        Lobby { shared, handle: self.runtime.handle().clone(), reach }
    }
}
impl Drop for Net {
    fn drop(&mut self) {
        let endpoint = self.endpoint.clone();
        let _ = self.runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), endpoint.close()).await });
    }
}

/// A host's lobby for one short code. Dropping it closes the lobby and forgets its address.
pub(crate) struct Lobby {
    shared: Arc<Mutex<LobbyState>>,
    handle: Handle,
    reach: Reach,
}
#[derive(Default)]
struct LobbyState {
    closed: bool,
    endpoint: Option<Endpoint>,
}
impl Drop for Lobby {
    fn drop(&mut self) {
        let taken = {
            let mut guard = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
            guard.closed = true;
            guard.endpoint.take()
        };
        if let Some(endpoint) = taken {
            retire(self.reach, &endpoint);
            self.handle.spawn(async move { endpoint.close().await });
        }
    }
}
fn retire(reach: Reach, endpoint: &Endpoint) {
    if reach == Reach::Loopback {
        directory().remove_endpoint_info(endpoint.id());
    }
}

/// A fresh code and its lobby endpoint, bound under the key derived from the code.
async fn open_lobby(reach: Reach) -> Result<(String, [u8; 32], Endpoint), String> {
    let (code, keys) = tokio::task::spawn_blocking(move || {
        let code = if reach == Reach::Loopback { ShortCode::local()? } else { ShortCode::generate()? };
        Ok::<_, String>((code.text()?, code.keys()?))
    })
    .await
    .map_err(|_| "the short-code worker stopped".to_owned())??;
    let endpoint = endpoint(reach, keys.key, LOBBY_ALPN).await?;
    Ok((code, keys.proof, endpoint))
}
async fn serve_lobby(endpoint: &Endpoint, proof: [u8; 32], invitation: Arc<str>) {
    let slots = Arc::new(Semaphore::new(LOBBY_SLOTS));
    while let Some(incoming) = endpoint.accept().await {
        let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
            incoming.refuse();
            continue;
        };
        let invitation = Arc::clone(&invitation);
        tokio::spawn(async move {
            let _slot = permit;
            hand_over(incoming, &proof, &invitation).await;
        });
    }
}
/// One lobby exchange: 32 proof bytes in; a big-endian u16 length and the invitation out, or a
/// zero length when the proof is wrong. Every guest with the code gets the invitation; the
/// session itself admits them until it is full.
async fn hand_over(incoming: Incoming, proof: &[u8; 32], invitation: &str) {
    let Ok(Ok(connection)) = tokio::time::timeout(CONNECT, incoming).await else { return };
    let Ok(Ok((mut out, mut into))) = tokio::time::timeout(CONNECT, connection.accept_bi()).await else {
        return;
    };
    let mut claim = [0u8; 32];
    if !matches!(tokio::time::timeout(TIMEOUT, into.read_exact(&mut claim)).await, Ok(Ok(()))) {
        return;
    }
    // Constant time.
    let admitted = bool::from(claim.ct_eq(proof));
    let reply = if admitted { invitation.as_bytes() } else { &[] };
    let Ok(length) = u16::try_from(reply.len()) else { return };
    let _ = tokio::time::timeout(TIMEOUT, async {
        out.write_all(&length.to_be_bytes()).await?;
        out.write_all(reply).await
    })
    .await;
    let _ = out.finish();
    // Let the guest read everything and hang up first.
    let _ = tokio::time::timeout(TIMEOUT, connection.closed()).await;
}

/// Trades a short code for the full invitation at the host's lobby, dialled by the id derived
/// from the code alone. Returns the guest's endpoint too, to reach the host with. Blocking:
/// plain threads only. Errors before any network use when the code cannot be stretched.
pub(crate) fn redeem(code: &ShortCode) -> Result<(Net, OnlineInvitation), String> {
    let keys = code.keys()?;
    let reach = if code.is_local() { Reach::Loopback } else { Reach::Internet };
    let net = Net::bind(reach)?;
    let retry = reach == Reach::Internet;
    let reply = net.runtime.block_on(ask_lobby(&net.endpoint, keys.key.public(), &keys.proof, retry))?;
    let text = String::from_utf8(reply).map_err(|_| "the session sent an invalid invitation")?;
    let invitation = OnlineInvitation::decode(&text)?;
    Ok((net, invitation))
}
/// `retry` keeps dialling for a while: public address records can lag a fresh lobby.
async fn ask_lobby(endpoint: &Endpoint, lobby: EndpointId, proof: &[u8; 32], retry: bool) -> Result<Vec<u8>, String> {
    const ABSENT: &str =
        "no session answers this code; check it, or ask the host for a new code or the full invitation";
    let deadline = tokio::time::Instant::now() + CONNECT;
    let connection = loop {
        match tokio::time::timeout_at(deadline, endpoint.connect(EndpointAddr::new(lobby), LOBBY_ALPN)).await {
            Ok(Ok(connection)) => break connection,
            // The host may have just read the code out before its address record propagated.
            Ok(Err(_)) if retry && tokio::time::Instant::now() + LOOKUP_RETRY < deadline => {
                tokio::time::sleep(LOOKUP_RETRY).await;
            }
            _ => return Err(ABSENT.into()),
        }
    };
    // The handshake proves the lobby holds the key derived from the code.
    if connection.remote_id() != lobby {
        return Err(ABSENT.into());
    }
    let exchange = async {
        let (mut out, mut into) = connection.open_bi().await.map_err(|e| e.to_string())?;
        out.write_all(proof).await.map_err(|e| e.to_string())?;
        let _ = out.finish();
        let mut length = [0u8; 2];
        into.read_exact(&mut length).await.map_err(|e| e.to_string())?;
        let length = usize::from(u16::from_be_bytes(length));
        if length == 0 {
            return Ok(None);
        }
        if length > MAX_INVITATION {
            return Err("the session sent an oversized invitation".to_owned());
        }
        let mut body = vec![0u8; length];
        into.read_exact(&mut body).await.map_err(|e| e.to_string())?;
        Ok(Some(body))
    };
    let result = tokio::time::timeout(TIMEOUT, exchange).await;
    connection.close(0u32.into(), b"invitation received");
    match result {
        Ok(Ok(Some(body))) => Ok(body),
        Ok(Ok(None)) => {
            Err("this code was refused: it is wrong, already used or expired; ask the host for a new one".into())
        }
        Ok(Err(error)) => Err(format!("the code exchange failed: {error}")),
        Err(_) => Err("the session did not answer the code in time".into()),
    }
}
/// A blocking view of one bidirectional QUIC stream, for plain threads only.
pub(crate) struct Stream {
    handle: Handle,
    connection: Connection,
    out: SendStream,
    into: RecvStream,
    deadline: Instant,
    _slot: Option<OwnedSemaphorePermit>,
}
impl Stream {
    fn new(
        handle: Handle,
        connection: Connection,
        out: SendStream,
        into: RecvStream,
        slot: Option<OwnedSemaphorePermit>,
    ) -> Self {
        Self { handle, connection, out, into, deadline: Instant::now() + crate::transport::TIMEOUT, _slot: slot }
    }
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "online exchange deadline"))
    }
}
impl Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let limit = self.remaining()?;
        let into = &mut self.into;
        // RecvStream::read is cancel-safe, so a timeout loses no data.
        match self.handle.block_on(async { tokio::time::timeout(limit, into.read(bytes)).await }) {
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "online exchange deadline")),
            Ok(Ok(Some(n))) => Ok(n),
            Ok(Ok(None)) => Ok(0),
            Ok(Err(e)) => Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, e)),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let limit = self.remaining()?;
        let out = &mut self.out;
        // SendStream::write is cancel-safe: it reports only bytes it accepted.
        match self.handle.block_on(async { tokio::time::timeout(limit, out.write(bytes)).await }) {
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "online exchange deadline")),
            Ok(Ok(n)) => Ok(n),
            Ok(Err(e)) => Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, e)),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Link for Stream {
    fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.out.finish();
        // Closing at once would discard a goodbye still in flight; wait briefly for its delivery.
        let stopped = self.out.stopped();
        let _ = self.handle.block_on(async { tokio::time::timeout(FAREWELL, stopped).await });
        self.connection.close(0u32.into(), b"session ended");
    }
}

#[cfg(test)]
#[path = "online_tests.rs"]
mod tests;
