// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Internet sessions: authenticated peer-to-peer QUIC through iroh, reachable by an invitation code.
//! No account and no server of ours: iroh punches through home routers and falls back to public
//! relays that only forward end-to-end encrypted traffic.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::invitation::Invite;
use crate::protocol::PROTOCOL;
use crate::transport::Link;

const PREFIX: &str = "suhina-net://";
const ALPN: &[u8] = b"suhina/multiplayer/15";
/// Hole punching and relay lookup can take several seconds across the Internet.
const CONNECT: Duration = Duration::from_secs(20);
const ONLINE: Duration = Duration::from_secs(10);

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

/// One iroh endpoint and the runtime driving it. Owned by a plain (non-runtime) thread.
pub(crate) struct Net {
    runtime: Runtime,
    endpoint: Endpoint,
}
impl Net {
    pub fn bind(reach: Reach) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("suhina-online")
            .enable_all()
            .build()
            .map_err(|e| format!("could not start the network runtime: {e}"))?;
        let endpoint = runtime.block_on(async {
            let builder = match reach {
                Reach::Loopback => Endpoint::builder(presets::Minimal)
                    .relay_mode(RelayMode::Disabled)
                    .clear_address_lookup()
                    .clear_ip_transports()
                    .bind_addr("127.0.0.1:0")
                    .map_err(|e| e.to_string())?,
                Reach::Internet => Endpoint::builder(presets::N0),
            };
            let endpoint = builder
                .secret_key(SecretKey::generate())
                .alpns(vec![ALPN.to_vec()])
                .bind()
                .await
                .map_err(|e| format!("could not open the network connection: {e}"))?;
            if reach == Reach::Internet {
                // Wait for a home relay so the invitation can be reached from behind any router.
                // A computer that gets none still invites with its direct addresses.
                let _ = tokio::time::timeout(ONLINE, endpoint.online()).await;
            }
            Ok::<_, String>(endpoint)
        })?;
        Ok(Self { runtime, endpoint })
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
}
impl Drop for Net {
    fn drop(&mut self) {
        let endpoint = self.endpoint.clone();
        let _ = self.runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), endpoint.close()).await });
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
        self.connection.close(0u32.into(), b"session ended");
    }
}

#[cfg(test)]
#[path = "online_tests.rs"]
mod tests;
