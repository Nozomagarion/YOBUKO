// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The portable project document: offered with its size and SHA-256, then streamed after one
//! request as chunk headers each followed by the chunk's raw bytes.

use crate::protocol::{self, ClientMessage, PROTOCOL, ServerMessage};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

const CHUNK: usize = 32 * 1024;
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
    match protocol::read::<ClientMessage>(io)? {
        ClientMessage::Download { protocol: PROTOCOL, offset: 0 } => {}
        _ => return Err("invalid MIDI download request".into()),
    }
    for (index, chunk) in bytes.chunks(CHUNK).enumerate() {
        if stop.load(Ordering::Relaxed) {
            return Err("session stopped".into());
        }
        let offset = index.checked_mul(CHUNK).ok_or("invalid transfer offset")?;
        protocol::write(io, &ServerMessage::Chunk { protocol: PROTOCOL, offset, len: chunk.len() })?;
        io.write_all(chunk).and_then(|_| io.flush()).map_err(|e| format!("connection write failed: {e}"))?;
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
    protocol::write(io, &ClientMessage::Download { protocol: PROTOCOL, offset: 0 })?;
    let mut document = vec![0; bytes];
    let mut offset = 0;
    while offset < bytes {
        if stop.load(Ordering::Relaxed) {
            return Err("session stopped".into());
        }
        let ServerMessage::Chunk { protocol: PROTOCOL, offset: received, len } = protocol::read::<ServerMessage>(io)?
        else {
            return Err("invalid MIDI document chunk".into());
        };
        if received != offset || len != CHUNK.min(bytes - offset) {
            return Err("invalid MIDI chunk offset or size".into());
        }
        let part = document.get_mut(offset..offset + len).ok_or("invalid MIDI chunk offset or size")?;
        io.read_exact(part).map_err(|e| format!("incomplete MIDI chunk: {e}"))?;
        offset += len;
        if offset.is_multiple_of(256 * 1024) || offset == bytes {
            progress(offset, bytes);
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
