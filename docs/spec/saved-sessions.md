---
description: Multiplayer sessions have a name and an owner, are saved on every participant's computer and can be opened again later, from the main version or a backup copy
read_when: saved multiplayer session, resume a session later, session name, main version, backup copy, who owns the session, Your sessions
type: spec
status: draft
last_verified: 2026-11-12
covers:
  - crates/multiplayer/src/saved.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
---

# Saved sessions

Self-authored, on top of the [shared studio](shared-studio.md) (protocol 19).

## What a person does

1. In the lobby's **Open a room** card, the host types a **Session name** and **Your name**
   (kept for next time), then creates the session or shares a project.
2. Everyone in the session sees its name and who keeps the main version, in the strip above
   the Studio and on the Multiplayer page.
3. Every participant's computer saves the session as it changes, at most every five seconds and
   once more when the session ends. Nothing is lost when someone leaves or the link drops.
4. Later, the lobby's **Your sessions** card lists them, newest first. Each says **Main version ·
   yours** or **Backup copy · main version: <owner>**, and when it was saved:
   - **Open for friends** (the main version) hosts it again with the same name; friends join
     with the new code.
   - **Open copy for friends** (a backup copy, when the owner is away) hosts the copy. Everyone
     who joins sees **Backup copy · the main version belongs to <owner>**.
   - **Open alone** opens it in the Studio as an ordinary project.

## The session's name

`SessionInfo` is a 32-hex-digit id (random, made when the session is created and kept when it
is opened again), a name and an owner of 1 to 64 bytes without control characters, and `main`:
whether the host plays the main version. The host sends it in the track controls of every state
(`Controls::session`, optional); a guest checks it like the rest of the controls. A session
hosted without a name (a test) has none and is not saved.

## On disk

Under the multiplayer storage root (`<data folder>/Shared projects`), `Sessions/<folder>/` holds
`session.json` (the `SessionInfo`, whether this computer keeps the main version, when it was
saved; written beside and renamed over, read only up to 64 KiB) and `session.suhina` (the
project, written with `project.save` and `copy`, so the shared tab keeps no file of its own).
`Sessions/me.json` keeps the name typed under **Your name**.

The folder is the session's id. A computer keeps the main version when it hosted the session
with `main`; every guest and the host of a backup copy keep a copy. When the computer that keeps
the main version joins a friend's backup copy, it saves that copy in `<id>-copy`, so the main
version is never overwritten by it. Opening a session for friends opens its file in a new tab
and hosts it once its sounds are loaded (at most two minutes), with the saved id and name, and
`main` when this computer keeps the main version.

## Not yet

Merging a backup copy's changes back into the main version; renaming or deleting a saved
session from the lobby (its folder can be deleted by hand).
