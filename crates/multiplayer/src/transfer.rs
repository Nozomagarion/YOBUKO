// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::protocol::{self, ClientMessage, PROTOCOL, ServerMessage};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

const CHUNK: usize = 4096;
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn send(io: &mut (impl Read + Write), bytes: &[u8], revision: u64, stop: &AtomicBool) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > crate::midi::MAX_DOCUMENT {
        return Err("invalid MIDI document size".into());
    }
    protocol::write(
        io,
        &ServerMessage::Project {
            protocol: PROTOCOL,
            format: crate::protocol::COPY_FORMAT,
            revision,
            bytes: bytes.len(),
            sha256: hash(bytes),
        },
    )?;
    for (index, chunk) in bytes.chunks(CHUNK).enumerate() {
        if stop.load(Ordering::Relaxed) {
            return Err("session stopped".into());
        }
        let offset = index.checked_mul(CHUNK).ok_or("invalid transfer offset")?;
        match protocol::read::<ClientMessage>(io)? {
            ClientMessage::Download { protocol: PROTOCOL, offset: requested } if requested == offset => {}
            _ => return Err("invalid MIDI download request".into()),
        }
        protocol::write(io, &ServerMessage::Chunk { protocol: PROTOCOL, offset, data: STANDARD.encode(chunk) })?;
    }
    Ok(())
}

pub fn receive(
    io: &mut (impl Read + Write),
    header: ServerMessage,
    stop: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<(Vec<u8>, u64), String> {
    let ServerMessage::Project { protocol: PROTOCOL, format: crate::protocol::COPY_FORMAT, revision, bytes, sha256 } =
        header
    else {
        return Err("invitation refused or incompatible MIDI format".into());
    };
    if bytes == 0
        || bytes > crate::midi::MAX_DOCUMENT
        || sha256.len() != 64
        || !sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid MIDI document offer".into());
    }
    let mut document = Vec::with_capacity(bytes);
    while document.len() < bytes {
        if stop.load(Ordering::Relaxed) {
            return Err("session stopped".into());
        }
        let offset = document.len();
        protocol::write(io, &ClientMessage::Download { protocol: PROTOCOL, offset })?;
        let ServerMessage::Chunk { protocol: PROTOCOL, offset: received, data } = protocol::read::<ServerMessage>(io)?
        else {
            return Err("invalid MIDI document chunk".into());
        };
        if received != offset || data.len() > CHUNK.div_ceil(3) * 4 {
            return Err("invalid MIDI chunk offset or size".into());
        }
        let chunk = STANDARD.decode(data).map_err(|_| "invalid MIDI chunk encoding")?;
        let expected = CHUNK.min(bytes - offset);
        if chunk.len() != expected {
            return Err("incomplete or oversized MIDI chunk".into());
        }
        document.extend_from_slice(&chunk);
        if document.len().is_multiple_of(256 * 1024) || document.len() == bytes {
            progress(document.len(), bytes);
        }
    }
    if hash(&document) != sha256 {
        return Err("MIDI document checksum mismatch".into());
    }
    Ok((document, revision))
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
