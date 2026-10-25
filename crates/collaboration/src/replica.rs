// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use serde_json::Value;
use suhina_model::Project;
use suhina_session::{Player, Session};

use crate::{Accepted, Error, Snapshot, policy};

/// One participant's copy, retaining its existing player and private transport across edits.
pub struct Replica {
    origin: Arc<()>,
    session: Session,
    revision: u64,
    last: Option<Accepted>,
    recovery_required: bool,
}

impl Replica {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            origin: snapshot.origin,
            session: Session::new(snapshot.project),
            revision: snapshot.revision,
            last: None,
            recovery_required: false,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn project(&self) -> &Project {
        self.session.project()
    }

    pub fn recovery_required(&self) -> bool {
        self.recovery_required
    }

    pub fn attach_player(&mut self, player: Player) {
        self.session.attach_player(player);
    }

    pub fn audition(&mut self, command: &str, params: &Value) -> Result<Value, Error> {
        policy::audition(&mut self.session, command, params)
    }

    /// `true` applied an edit; `false` is an identical retry of the last confirmation.
    pub fn apply(&mut self, accepted: &Accepted) -> Result<bool, Error> {
        if !Arc::ptr_eq(&self.origin, &accepted.origin) {
            return Err(Error::WrongSession);
        }
        if self.recovery_required {
            return Err(Error::RecoveryRequired);
        }
        if self.last.as_ref() == Some(accepted) {
            return Ok(false);
        }
        let next = self.revision.checked_add(1).ok_or(Error::CounterExhausted)?;
        if accepted.revision != next {
            return Err(Error::StaleRevision { expected: next, received: accepted.revision });
        }
        if self.session.execute(accepted.request.edit.command, &accepted.request.edit.params).is_err() {
            // A player notification can fail after the session model changed. Never quietly
            // retry that as though nothing happened; require explicit reconstruction instead.
            self.recovery_required = true;
            return Err(Error::RecoveryRequired);
        }
        self.revision = next;
        self.last = Some(accepted.clone());
        Ok(true)
    }
}
