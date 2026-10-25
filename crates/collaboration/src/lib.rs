// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Trusted in-process experiment, not an authenticated network session.
//!
//! The [`Host`] orders a small subset of existing session commands. Each participant, including
//! the host's owner, uses a [`Replica`] with its own player. Nothing depends on this crate in
//! the desktop app, engine or session. Call these allocating APIs on a control thread only.

mod host;
mod policy;
mod replica;

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use suhina_model::Project;

pub use host::Host;
pub use policy::Edit;
pub use replica::Replica;

/// Includes the host's owner. Peer handles are local bookkeeping, never authentication tokens.
pub const MAX_PEERS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PeerId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Reader,
    Editor,
}

/// A participant's request; numbering begins at one and rejected requests consume no number.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub sequence: u64,
    pub base_revision: u64,
    pub edit: Edit,
}

/// Only the host can construct a confirmation. There is intentionally no wire deserializer.
#[derive(Debug, Clone)]
pub struct Accepted {
    pub(crate) origin: Arc<()>,
    pub(crate) revision: u64,
    pub(crate) author: PeerId,
    pub(crate) request: Request,
    pub(crate) result: Value,
}

impl PartialEq for Accepted {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.origin, &other.origin)
            && self.revision == other.revision
            && self.author == other.author
            && self.request == other.request
            && self.result == other.result
    }
}

impl Accepted {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn author(&self) -> PeerId {
        self.author
    }

    pub fn result(&self) -> &Value {
        &self.result
    }
}

/// An already trusted project clone, not an untrusted project import or a file transfer format.
#[derive(Clone)]
pub struct Snapshot {
    pub(crate) origin: Arc<()>,
    pub(crate) revision: u64,
    pub(crate) project: Project,
}

impl Snapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn project(&self) -> &Project {
        &self.project
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnknownPeer,
    WrongSession,
    ReadOnly,
    OwnerProtected,
    PeerLimit,
    UnsupportedCommand,
    InvalidParameters,
    SequenceReused,
    SequenceGap,
    ReplayTooOld,
    StaleRevision { expected: u64, received: u64 },
    CounterExhausted,
    RecoveryRequired,
    Command(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, received } => {
                write!(f, "revision {received} does not match the required revision {expected}")
            }
            Self::Command(reason) => write!(f, "session command failed: {reason}"),
            other => f.write_str(match other {
                Self::WrongSession => "this confirmation belongs to another local session",
                Self::UnknownPeer => "this peer is not a member of the session",
                Self::ReadOnly => "this peer may watch but cannot edit",
                Self::OwnerProtected => "the session owner's membership and role are protected",
                Self::PeerLimit => "the session's peer limit has been reached",
                Self::UnsupportedCommand => "this command is not permitted by the offline experiment",
                Self::InvalidParameters => "these parameters are not permitted by the offline experiment",
                Self::SequenceReused => "this operation number was already used for a different request",
                Self::SequenceGap => "operations must arrive in sequence, starting at one",
                Self::ReplayTooOld => "this operation is older than the retry cache; fetch a fresh snapshot",
                Self::CounterExhausted => "the session's operation, peer or revision counter is exhausted",
                Self::RecoveryRequired => "the replica needs an explicit fresh snapshot before further edits",
                Self::StaleRevision { .. } | Self::Command(_) => "session command failed",
            }),
        }
    }
}

impl std::error::Error for Error {}

impl From<suhina_session::CommandError> for Error {
    fn from(error: suhina_session::CommandError) -> Self {
        Self::Command(error.to_string())
    }
}
