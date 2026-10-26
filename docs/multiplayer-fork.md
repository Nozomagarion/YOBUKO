---
description: Develop and validate the multiplayer fork while keeping main identical to upstream, and try the restricted LAN prototype
read_when: multiplayer fork development, upstream synchronization, run collaboration experiment, fork CI
type: howto
status: draft
last_verified: 2026-10-25
covers:
  - crates/collaboration
  - crates/multiplayer
  - crates/ui/src/app/multiplayer.rs
  - .github/workflows/multiplayer-ci.yml
---

# Multiplayer fork

## Scope and status

This fork is an exploratory implementation requested by its owner. It does not claim an official
upstream issue or represent accepted upstream collaboration architecture. It begins with an
offline host/replica core and now has restricted desktop LAN starter and current-MIDI-project sessions.
The [core specification](spec/session-sharing.md) and [LAN contract](spec/lan-starter-session.md)
state the supported subset and missing features. Physical two-computer acceptance remains pending.
No comments, claims or pull requests have been sent to upstream on the owner's behalf.

Issue #568 is complete; reuse the stable object identities. Issue #606 remains the dependency
for a portable project/manifest and media exchange. Recording already exists in the engine and
session; the historical recording draft's claim that it is absent is stale. Do not duplicate it.
Before contributing a feature upstream, follow AGENTS/CONTRIBUTING, discuss the extension to
coediting and independent transport, inspect overlapping PRs and claim an approved, free issue.
No broad refactor is part of this first experiment.

## Run the experiment

From the repository root with the normal upstream build prerequisites:

```sh
cargo test --locked -p suhina-collaboration
cargo run --locked -p suhina-collaboration --example two_peers
```

The example creates two sessions with separate real renderers, changes shared track parameters,
then prints their different playhead positions. It opens no window, socket or audio device and
writes no project or audio file. It is a headless test of the first building block.


## Try the desktop LAN prototype

Use the same combined build commit on both computers. These labels and preservation checks are
covered by automated UI tests; the physical two-computer procedure below is an acceptance test
still to perform. No personal project or audio file is needed.

1. Open Suhina. Click **Multiplayer** at the top, beside **Studio** and **DJ**; it is also in
   **View → Multiplayer LAN**. You should see **Create LAN session** and **Join LAN session**.
2. On the host, **Host address** should contain its private local IPv4 address followed by `:0`.
   Click **Create LAN session**. A new **LAN starter session** tab opens, preserving other tabs.
   Four generated MIDI tracks appear; **Copy invitation** becomes available. `127.0.0.1:0` is
   useful only for two app instances on the same computer, not for a second computer.
3. Click **Copy invitation** and send it privately to the guest. On the guest computer, open
   **Multiplayer**, paste it into **Invitation from the host**, and click **Join LAN session**.
   Both should show four MIDI tracks and two return tracks; the host should say **Guest connected**. Allow local-network
   access in the OS if requested. A failed connection must display an error, not a joined state.
4. On the host, change **Pulse** to **Shared pulse**, then click its **Rename** button. The guest
   should receive the new name and confirmed revision. On the guest, change that track's
   **Volume (dB)** to `-12`, then click **Set volume**. The host should receive it. Pan controls
   move sound between left and right. Concurrent stale edits should display a refusal; use
   **Refresh controls** if a newer revision arrived while you were typing, then submit again.
5. Click **Play locally** on both computers. The starter should sound on each one's configured
   audio output. On one, click **Listen from beat 16**, then **Stop locally**. The other should
   continue from its own position. Transport is deliberately private.
6. Click **Leave session**. The starter project remains open and can be edited normally. Other
   projects remain intact. A reused invitation must be refused. Create a fresh host session for
   another invitation; it expires after five minutes and admits one guest only.
7. To check disconnect recovery, create/join again, change a name, then quit the host. The guest
   should report disconnection within the exchange deadline and retain the last confirmed local
   starter. Saving it is local; your files and microphone recordings were never shared.

To use your own music, first open a MIDI project in Studio. Choose **Multiplayer → Share current
MIDI project**, then use the same invitation flow. It creates a new musical copy, retaining the
source tab. FM Synth and Utility parameters are supported; audio, other instruments, plugins
and opaque device states are refused with an error. Names/volume/pan and single notes are live shared edits. In **MIDI notes**, choose a clip,
select a note and use **Apply note** or **Delete note**; **New note draft → Add note** adds one.
A newer shared revision makes the draft stale; **Refresh note draft** clears selection so a
shifted index cannot silently target another note. In **Clips**, create an empty MIDI clip or select an arrangement clip to move it, trim its
edges, duplicate, silence/enable or delete it. Move/trim/duplicate preserve overlapping neighbours.
The destination track selector applies to Move; Trim follows native edge/offset semantics.
A newer revision disables a pending clip draft; refresh or select the current clip again.
A deleted note editor selection cannot silently switch to another clip. Local metadata, hardware
input settings and launch mappings are omitted. See the [copy contract](spec/lan-midi-copy.md).
Both participants need the new protocol-5 build; old demo invitations no longer work.

To include loaded audio, use **Share project + audio**. Configure the application data folder
when using a custom host: collected copies stay in its **Shared projects** subfolder. Only sounds
already loaded in Studio are copied; missing audio must first be loaded/relinked. They become
portable float32 WAV copies, without changing the originals, up to 64 files, 64 MiB each and
256 MiB total. Wait for the complete transfer before a shared tab appears. FM Synth/Utility
parameters remain supported; other devices/plugins and streamed stems are refused. Existing
audio clips can be moved/trimmed/duplicated with the same Clips controls. New audio clips are
empty; importing a new file or publishing a new recording during connection is not implemented.
See [the audio contract](spec/lan-audio-assets.md).

Network and UI regression checks:

```sh
cargo test --locked -p suhina-multiplayer
cargo test --locked -p suhina-ui multiplayer --lib
```

## Git invariants

`origin` is the owner's fork; `upstream` is `suhina-daw/suhina-daw`. `main` has no fork commits;
`feature/multiplayer` contains experiments and receives upstream merges. Never merge the feature
into the fork's `main`. Upstream contributions use small branches from `upstream/main`.

With a clean working tree, fetch both remotes, then fast-forward `main` to `upstream/main` and
check the exact SHA before pushing `origin main`. A divergent main is an error, not a reason for
an automatic reset or force push. Prepare an upstream merge on a temporary integration branch
from `feature/multiplayer`; resolve conflicts semantically and run the checks. Preserve the merge
ancestry when integrating the result: do not squash/rebase that integration commit.

```sh
git fetch origin --prune
git fetch upstream --prune
git rev-parse main upstream/main origin/main
git merge-base --is-ancestor upstream/main feature/multiplayer
git diff --stat upstream/main...feature/multiplayer
```

The owner selected `feature/multiplayer` as the repository default branch on 2026-10-26, so
the repository home shows the combined application and its README. `main` remains the unchanged
upstream mirror. Schedules run on the default branch: any later reviewed fork scheduler can live
on feature/multiplayer without modifying main. Synchronization is still manual; no periodic
integration is activated merely by changing the repository default.

The fork updater now uses a dedicated Multiplayer + DJ channel and rejects packages without
the matching distribution marker and checksum. Official binaries cannot replace this fork.
The shared release channel still needs a reviewed publication before friends can use it.
[Source integration and distribution](fork-distribution.md) gives the source pins, isolated
candidate script and packaging workflow. [DJ workspace](spec/dj-workspace.md) describes the
first native DJ library and render bridge, separate from the LAN adapter.

## Validation and builds

Run the upstream quality gates: fmt, workspace clippy/tests, docs checks, licence/rename checks,
and cargo-deny when Cargo.lock changes. Existing documentation structure warnings belong to the
upstream cleanup; do not hide or mass-edit them. Add a `Parity: no change` explanation for this
restricted LAN addition: no existing production-parity chapter is covered; full project coediting
is still absent and DSP is unchanged.

The fork's CI workflow has read-only permissions, no secrets, and targets only the multiplayer
and integration branches. It tests and builds on Linux, macOS and Windows. It produces development
artifacts containing the app, CLI, plugin host and required notices; it does not publish releases.
A compiled target is not proof of physical audio, microphone, firewall or NAT behavior.

Inherited workflows must be disabled/audited in fork settings before Actions is enabled. In
particular, upstream publishing, Discord, board and generated-main writers are not appropriate
for the fork. Keep only explicitly reviewed fork checks active. Never place write credentials in
a job running Rust build scripts or a pull request's code. New workflows belong only on the
multiplayer branch; main stays byte- and history-identical to upstream.
