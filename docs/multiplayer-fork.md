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

## Shared Launcher (protocol 11)

Open **Multiplayer → Launcher** in a hosted/joined session. Click an empty slot, choose
**Source clip**, then **Copy clip into shared slot**; the original arrangement stays intact.
Alternatively choose **Create empty shared slot**, then use **MIDI notes** to add its notes.
Click a scene row to rename, insert before, duplicate or delete it; the last row cannot be
deleted. **Append shared scene** adds an empty row. All confirmed edits appear in both copies.
Click a filled slot to clear it or choose a destination for **Move or swap shared slot**;
occupied compatible targets swap. Copying into an occupied slot fails without replacement.
Explicitly routed multi-track patterns cannot be copied into slots in this milestone.
**Launch slot locally** / **Launch scene locally** act only on this computer, as does Stop locally.
New shared edits make drafts stale; **Refresh launcher draft** clears them. Structural scene
changes clear selections even on hidden pages, requiring a fresh selection. Both applications
must use protocol 11; old invitations are refused. [Contract](spec/lan-launcher-editing.md).
Physical two-computer tests and cross-platform acceptance remain separate from local tests.

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
   projects remain intact. A reused invitation must be refused. If only the guest leaves, the
   host keeps its project and can use **New invitation → Copy invitation** to invite again;
   the new secret expires after five minutes and admits one guest only. The returning guest
   opens the current host version in a new tab, retaining the previous solo copy and its edits.
   The button refuses a connected guest. If the host leaves, its session ends.
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
Both participants need the new protocol-10 build; old demo invitations no longer work.

To include loaded audio, use **Share project + audio**. Configure the application data folder
when using a custom host: collected copies stay in its **Shared projects** subfolder. Only sounds
already loaded in Studio are copied; missing audio must first be loaded/relinked. They become
portable float32 WAV copies, without changing the originals, up to 64 files, 64 MiB each and
256 MiB total. Wait for the complete transfer before a shared tab appears. FM Synth/Utility
parameters remain supported; other devices/plugins and streamed stems are refused. Existing
audio clips can be moved/trimmed/duplicated with the same Clips controls. New audio clips are
empty. To publish a finished take while connected, either participant records/finishes it in an ordinary
local Studio tab, keeps Multiplayer open, switches to that tab and selects **Publish take**.
Choose its loaded sound, an existing shared audio-only track and the arrangement start beat,
then **Publish loaded take**. The whole sound is inserted unwarped with default gain; source
trims, loops and effects are not baked in. New verified audio arrives before the guest sees the
clip, preserving both independent players. The guest prepares a local copy in the background while still polling, then the host pulls and
verifies its exact chunks before ordering insertion. Direct shared-tab recording remains unsupported. Save the shared copy explicitly to retain the new arrangement on reopening.
See [the audio contract](spec/lan-audio-assets.md).

In **Tracks**, enter a name, choose **Audio track** or **MIDI track · FM Synth**, and click
**Add shared track**. Both participants can do this while connected. A new audio track appears
immediately in **Publish take** destinations; use **Clips** then **MIDI notes** to compose on a
new MIDI track. Refresh a stale track draft explicitly; the name is cleared. Inputs stay off,
existing clips and readers are preserved, and the 64-track cap includes returns/groups.
General track.add, arbitrary devices and track deletion/grouping remain closed over LAN.
See the [shared-track contract](spec/lan-track-creation.md).

**Publish take** is available to the invited editor too. The host checks its draft revision,
sequence, target, metadata and quotas before requesting bytes, then checks again after verified
collection. A changed project can refuse the now-stale take: refresh the shared state before
trying again. Failed preparations/transfers preserve the original local recording. Progress
shows preparing/sending; playback remains local. Upload chunks share the existing TLS stream,
so shared network edits wait for that exchange to finish. See the [guest-take contract](spec/lan-guest-takes.md).

In **Devices**, choose an existing FM Synth or Utility and a parameter. **Find parameter**
filters its generated SDK list. Edit **Draft value**, then **Apply shared parameter**. The
confirmed value and both copies should change, while listening positions stay independent.
A newer shared revision disables Apply: **Refresh device draft** reloads the current value
before making a new edit. Switching to another target explicitly starts its new draft. A
switched-off device stores values without enabling it; manual changes override that parameter's
existing automation as ordinary native edits do. No new device/preset/plugin is loaded.
To start from an empty app, choose **Create LAN session → Devices**, select **Pulse** in
**Device track**, then **Add FM Synth** or **Add Utility**. Refresh a stale chain draft first.
Utility is appended; FM Synth is only available for a MIDI track without an instrument.
**Shared device → Remove selected device** removes its settings/automation and keeps the music.
After every confirmed addition/removal, explicitly select Shared device again: old indices and
parameter drafts are discarded, including on hidden pages and repeated same-kind devices.
Then choose a parameter and apply it as above. The second instance joins its private invitation to check both copies.
See the [shared-device contract](spec/lan-device-parameters.md).

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
