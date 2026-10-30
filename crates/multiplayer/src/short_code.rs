// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Short invitation codes such as `7-piano-tigre-lune`, read out loud in a voice chat.
//!
//! A code is a number from 1 to 999 and three words from a self-authored list of 2048: about
//! 43 bits. Number 0 marks a lobby on this computer only (`online-loopback`). Nothing is sent in clear: both sides stretch the code with Argon2id into a lobby key
//! and a lobby proof, so a code is worth only what a guesser can try online before it expires
//! (five minutes, one guest), and a published lobby id cannot be traced back to its code offline.

use argon2::{Algorithm, Argon2, Params, Version};
use iroh::SecretKey;
use std::sync::OnceLock;

const WORD_FILE: &str = include_str!("short_code_words.txt");
/// The list's length: eleven bits per word.
pub(crate) const WORD_COUNT: usize = 2048;
const WORDS_PER_CODE: usize = 3;
const MAX_NUMBER: u16 = 999;
/// Longest text accepted as a short code, before any parsing.
const MAX_TEXT: usize = 128;
/// Argon2id cost: 32 MiB and three passes: about 70 ms on a recent computer, once per code and side.
const MEMORY_KIB: u32 = 32 * 1024;
const PASSES: u32 = 3;
const SALT: &[u8] = b"suhina/online/short-code/v1";
const KEY_CONTEXT: &str = "suhina 2026-10-30 online short code lobby endpoint key v1";
const PROOF_CONTEXT: &str = "suhina 2026-10-30 online short code lobby proof v1";

/// The word list, or `None` when the embedded file is not the expected 2048 sorted entries.
pub(crate) fn words() -> Option<&'static [&'static str]> {
    static WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    let words = WORDS.get_or_init(|| {
        WORD_FILE.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).collect()
    });
    (words.len() == WORD_COUNT && words.windows(2).all(|pair| pair[0] < pair[1])).then_some(words.as_slice())
}
fn damaged() -> String {
    "the short-code word list is damaged; use the full invitation".into()
}

/// A short code: its number and the positions of its words in the list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShortCode {
    number: u16,
    words: [u16; WORDS_PER_CODE],
}
impl std::fmt::Debug for ShortCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShortCode { redacted }")
    }
}
/// What both sides derive from a code: the lobby's endpoint key and the proof a guest sends.
pub(crate) struct LobbyKeys {
    pub key: SecretKey,
    pub proof: [u8; 32],
}
impl std::fmt::Debug for LobbyKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LobbyKeys { redacted }")
    }
}

impl ShortCode {
    /// A fresh code from the operating system's random source, uniform over every code.
    pub fn generate() -> Result<Self, String> {
        words().ok_or_else(damaged)?;
        let number = loop {
            // 64 935 = 65 × 999: rejecting the rest keeps every number equally likely.
            let value = random_u16()?;
            if value < 64_935 {
                break value % MAX_NUMBER + 1;
            }
        };
        let mut words = [0u16; WORDS_PER_CODE];
        for word in &mut words {
            // 2048 divides 65 536, so the low eleven bits are uniform.
            *word = random_u16()? & 0x07ff;
        }
        Ok(Self { number, words })
    }

    /// A fresh code for a lobby on this computer only: number 0, which Internet codes never use.
    pub fn local() -> Result<Self, String> {
        Ok(Self { number: 0, ..Self::generate()? })
    }
    /// Whether the code names a lobby on this computer only (number 0).
    pub fn is_local(&self) -> bool {
        self.number == 0
    }

    /// Whether the text is meant as a short code: it starts with a digit, unlike both invitations.
    pub fn looks_like(text: &str) -> bool {
        text.trim_start().starts_with(|c: char| c.is_ascii_digit())
    }

    /// Reads a code typed or pasted by a person: any case, accents, and spaces, dots, commas,
    /// underscores or hyphens between the parts. Nothing touches the network.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_TEXT {
            return Err("this code is too long; a short code looks like 7-piano-tigre-lune".into());
        }
        let list = words().ok_or_else(damaged)?;
        let folded = fold(text);
        let mut parts = folded.split(|c: char| !c.is_ascii_alphanumeric()).filter(|part| !part.is_empty());
        let number = parts
            .next()
            .filter(|part| part.len() <= 3 && part.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|part| part.parse::<u16>().ok())
            .filter(|n| *n <= MAX_NUMBER)
            .ok_or("a short code starts with a number from 1 to 999, like 7-piano-tigre-lune")?;
        let mut words = [0u16; WORDS_PER_CODE];
        for word in &mut words {
            let part = parts.next().ok_or("a short code has a number and three words, like 7-piano-tigre-lune")?;
            let index = list
                .binary_search(&part)
                .map_err(|_| format!("\u{201c}{part}\u{201d} is not a code word; check the spelling"))?;
            *word = u16::try_from(index).map_err(|_| damaged())?;
        }
        if parts.next().is_some() {
            return Err("a short code has a number and exactly three words, like 7-piano-tigre-lune".into());
        }
        Ok(Self { number, words })
    }

    /// The code as shown and read out: `7-piano-tigre-lune`.
    pub fn text(&self) -> Result<String, String> {
        let list = words().ok_or_else(damaged)?;
        let mut text = self.number.to_string();
        for index in self.words {
            text.push('-');
            text.push_str(list.get(usize::from(index)).ok_or_else(damaged)?);
        }
        Ok(text)
    }

    /// Stretches the code into the lobby's key and proof. Takes tens of milliseconds and
    /// 32 MiB; call it on a worker thread, never on the UI or audio threads.
    pub fn keys(&self) -> Result<LobbyKeys, String> {
        let canonical = self.text()?;
        let params = Params::new(MEMORY_KIB, PASSES, 1, Some(32)).map_err(|_| "invalid short-code cost")?;
        let mut seed = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(canonical.as_bytes(), SALT, &mut seed)
            .map_err(|_| "could not derive the short-code keys")?;
        let key = SecretKey::from_bytes(&blake3::derive_key(KEY_CONTEXT, &seed));
        let proof = blake3::derive_key(PROOF_CONTEXT, &seed);
        Ok(LobbyKeys { key, proof })
    }
}

fn random_u16() -> Result<u16, String> {
    let mut bytes = [0u8; 2];
    getrandom::fill(&mut bytes).map_err(|_| "the OS could not generate a short code")?;
    Ok(u16::from_le_bytes(bytes))
}

/// Lowercase ASCII, with the accents of French and other Latin letters removed.
fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => out.push('a'),
            'ç' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'ñ' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            // Combining accents left by a decomposed paste.
            '\u{0300}'..='\u{036f}' => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
#[path = "short_code_tests.rs"]
mod tests;
