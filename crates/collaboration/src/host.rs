// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::sync::Arc;

use suhina_model::Project;
use suhina_session::Session;

use crate::{Accepted, Error, MAX_PEERS, PeerId, Request, Role, Snapshot};

struct Peer {
    role: Role,
    next_sequence: u64,
    last: Option<Accepted>,
}

impl Peer {
    fn new(role: Role) -> Self {
        Self { role, next_sequence: 1, last: None }
    }
}

/// Single authoritative command owner. Playback belongs to the owner's separate replica too.
/// Membership methods are for the trusted local host caller, not remote guest commands.
pub struct Host {
    origin: Arc<()>,
    session: Session,
    revision: u64,
    peers: BTreeMap<PeerId, Peer>,
    next_peer: u64,
}

impl Host {
    pub fn new(project: Project) -> Result<Self, Error> {
        let session = Session::new(project);
        session.project().validate().map_err(|errors| Error::Command(format!("invalid project: {errors:?}")))?;
        Ok(Self {
            origin: Arc::new(()),
            session,
            revision: 0,
            peers: BTreeMap::from([(Self::owner(), Peer::new(Role::Editor))]),
            next_peer: 2,
        })
    }

    pub const fn owner() -> PeerId {
        PeerId(1)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn project(&self) -> &Project {
        self.session.project()
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot { origin: Arc::clone(&self.origin), revision: self.revision, project: self.session.project().clone() }
    }

    pub fn add_peer(&mut self, role: Role) -> Result<PeerId, Error> {
        if self.peers.len() >= MAX_PEERS {
            return Err(Error::PeerLimit);
        }
        let next = self.next_peer.checked_add(1).ok_or(Error::CounterExhausted)?;
        let peer = PeerId(self.next_peer);
        self.peers.insert(peer, Peer::new(role));
        self.next_peer = next;
        Ok(peer)
    }

    pub fn set_role(&mut self, peer: PeerId, role: Role) -> Result<(), Error> {
        if peer == Self::owner() {
            return Err(Error::OwnerProtected);
        }
        self.peers.get_mut(&peer).ok_or(Error::UnknownPeer)?.role = role;
        Ok(())
    }

    pub fn remove_peer(&mut self, peer: PeerId) -> Result<(), Error> {
        if peer == Self::owner() {
            return Err(Error::OwnerProtected);
        }
        self.peers.remove(&peer).map(|_| ()).ok_or(Error::UnknownPeer)
    }

    /// Check role, sequence/retry, revision and counter bounds before costly trusted preparation.
    /// Returns the cached confirmation for an exact retry; never mutates the project or counters.
    pub fn preflight(&self, author: PeerId, request: &Request) -> Result<Option<Accepted>, Error> {
        let peer = self.peers.get(&author).ok_or(Error::UnknownPeer)?;
        if peer.role != Role::Editor {
            return Err(Error::ReadOnly);
        }
        // Check the retry before base revision: a lost ACK necessarily has an old base.
        if request.sequence < peer.next_sequence {
            return match peer.last.as_ref() {
                Some(last) if last.request == *request => Ok(Some(last.clone())),
                Some(last) if last.request.sequence == request.sequence => Err(Error::SequenceReused),
                _ => Err(Error::ReplayTooOld),
            };
        }
        if request.sequence != peer.next_sequence {
            return Err(Error::SequenceGap);
        }
        if request.base_revision != self.revision {
            return Err(Error::StaleRevision { expected: self.revision, received: request.base_revision });
        }
        self.revision.checked_add(1).ok_or(Error::CounterExhausted)?;
        peer.next_sequence.checked_add(1).ok_or(Error::CounterExhausted)?;
        Ok(None)
    }

    pub fn submit(&mut self, author: PeerId, request: Request) -> Result<Accepted, Error> {
        if let Some(accepted) = self.preflight(author, &request)? {
            return Ok(accepted);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::CounterExhausted)?;
        let peer = self.peers.get_mut(&author).ok_or(Error::UnknownPeer)?;
        let next_sequence = peer.next_sequence.checked_add(1).ok_or(Error::CounterExhausted)?;
        // This canonical session has no player, plugins, backup path or remote file commands.
        let result = self.session.execute(request.edit.command, &request.edit.params)?;
        let accepted = Accepted { origin: Arc::clone(&self.origin), revision, author, request, result };
        self.revision = revision;
        peer.next_sequence = next_sequence;
        peer.last = Some(accepted.clone());
        Ok(accepted)
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
