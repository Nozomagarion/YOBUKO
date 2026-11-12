---
description: The shared session shows in the ordinary Studio, and any studio edit anyone makes there reaches every participant, host-ordered
read_when: multiplayer studio, shared tab editing, collaborative arrangement or launcher, which edits a session shares, studio edit relay
type: spec
status: draft
last_verified: 2026-11-12
covers:
  - crates/session/src/shared_studio.rs
  - crates/collaboration/src/policy.rs
  - crates/collaboration/src/host.rs
  - crates/multiplayer/src/protocol.rs
  - crates/ui/src/app/multiplayer.rs
---

# Shared studio

Self-authored extension of the host-ordered session ([session sharing](session-sharing.md),
[online sessions](online-sessions.md)), protocol 19. Before it, the shared tab only took a short
list of typed edits from a dedicated form; the ordinary Studio refused to edit it.

## What a person does

1. The host shares a project (or creates a session); the guests join with the code.
2. On the Multiplayer page, **Open the shared session in Studio** brings the shared tab to the
   front in the Studio: arrangement, launcher, clip editor, devices and mixer, as in any project.
   A strip above the Studio names the session, its code and its guests, with **Session page**
   (the Multiplayer page, whose controls all stay) and **Leave session**.
3. Whatever anyone edits there (a MIDI clip drawn, notes moved, a track added, a fader moved)
   goes to the host, which runs it on its copy and sends it to every participant, who run it on
   theirs. Each computer still plays the project itself; linked playback is unchanged.

## Which edits are shared

A *studio edit* is any registry command on one project whose effect is an edit (one undo step),
except those that reach outside the project (`suhina_session::shared_studio_command`):

- the clipboard: every `*.paste`;
- files: `project.collect`, `project.relinkFiles`, `clip.addFiles`, `clip.consolidate` (it writes
  a rendered WAV), `device.addSd`, `device.updateSd`, `device.setNamFile`, `device.loadSample`;
- plug-ins, loaded from each computer's own installation: `device.addPlugin`, `clip.setAra`;
- this computer's hardware: `track.setAudioInput`, `track.setMidiInput`, `mapping.add`,
  `mapping.remove`.

Its parameters (`shared_studio_params`) are a JSON object of at most 2048 bytes, nested at most
four levels, with only the command's own parameter names, none of which may be of a file or
controller type (so `track.add` is shared without `preset`, `sd` or `sample`). The edits that draw
random numbers (`device.randomize`, `note.humanize`, `note.randomizeVelocity`) are shared only with
a `seed`; the Studio adds one, so every computer draws the same. The command checks everything
else when it runs, on the host first.

Undo and redo move through one computer's history and are not shared yet: the Studio says so.
Transport, launching and reads run on the local copy as before.

## Order and conflicts

A studio edit travels as `{"command":"studio","id":…,"params":…}`. The host checks it as it
checks every shared edit: on a throw-away copy of its project first, which must still make a
portable copy (`midi::encode_portable`), then for real, recording one revision. Unlike the typed
edits, a studio edit seen against an older revision is not refused: it runs on the latest one,
so whoever edits last wins, as in a shared document. Every participant runs the same commands in
the same order from the same copy, so the projects stay equal; each still checks the track
controls the host sends with every state.

The Studio queues its edits and hands them over in order; a drag's edits (one per frame, with
the same `gesture`) replace one another while they wait, so a drag sends its latest position.
At most 256 edits wait.

A network message may hold 256 KiB (64 KiB before), room for 32 operations of 2 KiB and the track
controls.

## Not yet

Shared undo, pasting, recording into the shared tab, adding plug-ins or files while connected,
saving the shared session under a name on every computer and resuming it later.
