// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};

const PREFIX: &str = "suhina-lan://";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Invitation {
    pub protocol: u16,
    pub endpoint: SocketAddr,
    pub certificate: String,
    pub secret: String,
}

impl std::fmt::Debug for Invitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invitation { redacted }")
    }
}
impl Invitation {
    pub fn encode(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self).map_err(|_| "could not encode invitation")?;
        Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)))
    }
    pub fn decode(text: &str) -> Result<Self, String> {
        if text.len() > 8192 {
            return Err("invitation is too long".into());
        }
        let body = text.trim().strip_prefix(PREFIX).ok_or("not a Suhina LAN invitation")?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| "invalid invitation encoding")?;
        crate::protocol::invitation_protocol(&bytes)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| "invalid invitation")?;
        if value.protocol != super::protocol::PROTOCOL
            || !private(value.endpoint.ip())
            || value.endpoint.port() == 0
            || value.secret.len() != 64
            || !value.secret.bytes().all(|b| b.is_ascii_hexdigit())
            || value.certificate.len() > 4096
        {
            return Err("unsupported LAN invitation".into());
        }
        value.der()?;
        Ok(value)
    }
    pub fn der(&self) -> Result<Vec<u8>, String> {
        let der = URL_SAFE_NO_PAD.decode(&self.certificate).map_err(|_| "invalid invitation certificate")?;
        if der.is_empty() || der.len() > 2048 {
            return Err("invalid invitation certificate size".into());
        }
        Ok(der)
    }
}
/// An invitation whose single-use secret the host rotates.
pub(crate) trait Invite {
    fn secret(&self) -> &str;
    fn set_secret(&mut self, secret: String);
    fn encode(&self) -> Result<String, String>;
}
impl Invite for Invitation {
    fn secret(&self) -> &str {
        &self.secret
    }
    fn set_secret(&mut self, secret: String) {
        self.secret = secret;
    }
    fn encode(&self) -> Result<String, String> {
        Invitation::encode(self)
    }
}
pub(crate) fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        IpAddr::V6(_) => false,
    }
}
pub(crate) fn new_secret() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "the OS could not generate an invitation secret")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(crate) fn secret_hash(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}
