// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Catalog, MAX_PLAYLIST_ENTRIES, MAX_PLAYLISTS, MAX_TRACKS};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Playlist {
    pub id: u32,
    pub name: String,
    pub tracks: Vec<u32>,
}

#[derive(Debug)]
pub enum PlaylistEdit {
    Create(String),
    Rename { playlist: u32, name: String },
    Delete(u32),
    Add { playlist: u32, track: u32 },
    Remove { playlist: u32, track: u32 },
    Move { playlist: u32, track: u32, position: usize },
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.trim() == name && !name.chars().any(char::is_control)
}

impl Catalog {
    pub(super) fn validate_playlists(&self, tracks: &HashSet<u32>) -> Result<(), String> {
        let mut ids = HashSet::new();
        let mut count = 0usize;
        if self.playlists.len() > MAX_PLAYLISTS
            || self.next_playlist_id == 0
            || self.next_playlist_id > u64::from(u32::MAX) + 1
        {
            return Err("invalid DJ playlist limit or id counter".into());
        }
        for p in &self.playlists {
            count = count.checked_add(p.tracks.len()).ok_or("DJ playlist entry limit")?;
            let mut entries = HashSet::new();
            if p.id == 0
                || u64::from(p.id) >= self.next_playlist_id
                || !ids.insert(p.id)
                || !valid_name(&p.name)
                || p.tracks.len() > MAX_TRACKS
                || count > MAX_PLAYLIST_ENTRIES
                || p.tracks.iter().any(|id| !tracks.contains(id) || !entries.insert(id))
            {
                return Err("invalid DJ playlist name, id or track references".into());
            }
        }
        Ok(())
    }

    /// Mutate a candidate only: callers publish it after a successful checked save.
    pub fn edit_playlist(&mut self, revision: u64, edit: PlaylistEdit) -> Result<Option<u32>, String> {
        if revision != self.revision {
            return Err("the DJ library changed; refresh before editing".into());
        }
        let input_name = match &edit {
            PlaylistEdit::Create(name) | PlaylistEdit::Rename { name, .. } => Some(name),
            _ => None,
        };
        if input_name.is_some_and(|name| name.len() > 128 || name.chars().any(char::is_control)) {
            return Err("invalid DJ playlist name".into());
        }
        let mut next = self.clone();
        let mut created = None;
        match edit {
            PlaylistEdit::Create(name) => {
                if next.playlists.len() >= MAX_PLAYLISTS {
                    return Err("the DJ playlist limit was reached".into());
                }
                let id = u32::try_from(next.next_playlist_id).map_err(|_| "DJ playlist ids exhausted")?;
                next.next_playlist_id = next.next_playlist_id.checked_add(1).ok_or("DJ playlist ids exhausted")?;
                next.playlists.push(Playlist { id, name: name.trim().into(), tracks: Vec::new() });
                created = Some(id);
            }
            PlaylistEdit::Delete(id) => {
                let index = next.playlists.iter().position(|p| p.id == id).ok_or("unknown DJ playlist")?;
                next.playlists.remove(index);
            }
            edit => {
                let id = match &edit {
                    PlaylistEdit::Rename { playlist, .. }
                    | PlaylistEdit::Add { playlist, .. }
                    | PlaylistEdit::Remove { playlist, .. }
                    | PlaylistEdit::Move { playlist, .. } => *playlist,
                    _ => return Err("invalid DJ playlist edit".into()),
                };
                let p = next.playlists.iter_mut().find(|p| p.id == id).ok_or("unknown DJ playlist")?;
                match edit {
                    PlaylistEdit::Rename { name, .. } => p.name = name.trim().into(),
                    PlaylistEdit::Add { track, .. } => {
                        if p.tracks.len() >= MAX_TRACKS || p.tracks.contains(&track) {
                            return Err("track already in this playlist or playlist full".into());
                        }
                        p.tracks.push(track);
                    }
                    PlaylistEdit::Remove { track, .. } => {
                        let index =
                            p.tracks.iter().position(|id| *id == track).ok_or("track is not in this playlist")?;
                        p.tracks.remove(index);
                    }
                    PlaylistEdit::Move { track, position, .. } => {
                        if position == 0 || position > p.tracks.len() {
                            return Err("choose a final position between 1 and the playlist length".into());
                        }
                        let index =
                            p.tracks.iter().position(|id| *id == track).ok_or("track is not in this playlist")?;
                        p.tracks.remove(index);
                        p.tracks.insert(position - 1, track);
                    }
                    _ => return Err("invalid DJ playlist edit".into()),
                }
            }
        }
        next.revision = next.revision.checked_add(1).ok_or("DJ catalog revision exhausted")?;
        next.validate()?;
        *self = next;
        Ok(created)
    }
}
