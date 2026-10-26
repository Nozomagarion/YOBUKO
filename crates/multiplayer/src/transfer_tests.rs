// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::io::{Cursor, Result as IoResult};

struct Script {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}
impl Read for Script {
    fn read(&mut self, out: &mut [u8]) -> IoResult<usize> {
        self.input.read(out)
    }
}
impl Write for Script {
    fn write(&mut self, bytes: &[u8]) -> IoResult<usize> {
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}
fn header(bytes: usize, sha256: String) -> ServerMessage {
    ServerMessage::Project { protocol: PROTOCOL, format: 1, bytes, sha256 }
}

#[test]
fn offered_sizes_hashes_offsets_and_truncation_are_checked_before_opening_a_project() {
    let stop = AtomicBool::new(false);
    for size in [0, crate::midi::MAX_DOCUMENT + 1, usize::MAX] {
        let mut io = Script { input: Cursor::new(vec![]), output: vec![] };
        assert!(receive(&mut io, header(size, "a".repeat(64)), &stop, |_, _| {}).is_err());
        assert!(io.output.is_empty(), "an invalid offer must not request or allocate the document");
    }
    for (offset, data, checksum) in
        [(1, "abc".as_bytes(), hash(b"abc")), (0, "ab".as_bytes(), hash(b"abc")), (0, "abc".as_bytes(), "a".repeat(64))]
    {
        let mut input = vec![];
        protocol::write(&mut input, &ServerMessage::Chunk { protocol: PROTOCOL, offset, data: STANDARD.encode(data) })
            .unwrap();
        let mut io = Script { input: Cursor::new(input), output: vec![] };
        assert!(receive(&mut io, header(3, checksum), &stop, |_, _| {}).is_err());
    }
    let mut io = Script { input: Cursor::new(vec![]), output: vec![] };
    assert!(receive(&mut io, header(3, hash(b"abc")), &stop, |_, _| {}).is_err());
    assert!(receive(&mut io, header(3, hash(b"abc")), &AtomicBool::new(true), |_, _| {}).is_err());
}

#[test]
fn the_sender_refuses_a_guest_that_requests_a_different_offset() {
    let mut input = vec![];
    protocol::write(&mut input, &ClientMessage::Download { protocol: PROTOCOL, offset: 1 }).unwrap();
    let mut io = Script { input: Cursor::new(input), output: vec![] };
    assert!(send(&mut io, b"abc", &AtomicBool::new(false)).is_err());
}
