---
description: Develop and validate the multiplayer fork while keeping main identical to upstream, and run the offline experiment
read_when: multiplayer fork development, upstream synchronization, run collaboration experiment, fork CI
type: howto
status: draft
last_verified: 2026-10-25
covers:
  - crates/collaboration
  - .github/workflows/multiplayer-ci.yml
---

# Multiplayer fork

## Scope and status

This fork is an exploratory implementation requested by its owner. It does not claim an official
upstream issue or represent accepted upstream collaboration architecture. It begins with an
offline host/replica experiment; it does not yet provide usable multiplayer in the desktop app.
The [specification](spec/session-sharing.md) states the supported subset and missing features.
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

No scheduled workflow is added to main. Schedules run on the default branch; writing fork-only
automation there would violate the mirror. Start with manual synchronization. Any later scheduler
must live elsewhere or use an explicitly chosen different default branch, after reviewing effects.

The fork updater now uses a dedicated Multiplayer + DJ channel and rejects packages without
the matching distribution marker and checksum. Official binaries cannot replace this fork.
The shared release channel still needs a reviewed publication before friends can use it.
[Source integration and distribution](fork-distribution.md) gives the source pins, isolated
candidate script and packaging workflow. [DJ workspace](spec/dj-workspace.md) describes the
first native DJ library and render bridge; it does not add remote multiplayer.

## Validation and builds

Run the upstream quality gates: fmt, workspace clippy/tests, docs checks, licence/rename checks,
and cargo-deny when Cargo.lock changes. Existing documentation structure warnings belong to the
upstream cleanup; do not hide or mass-edit them. Add a `Parity: no change` explanation for this
unexposed experiment: it adds no desktop/CLI/MCP capability and changes no DSP.

The fork's CI workflow has read-only permissions, no secrets, and targets only the multiplayer
and integration branches. It tests and builds on Linux, macOS and Windows. It produces development
artifacts containing the app, CLI, plugin host and required notices; it does not publish releases.
A compiled target is not proof of physical audio, microphone, firewall or NAT behavior.

Inherited workflows must be disabled/audited in fork settings before Actions is enabled. In
particular, upstream publishing, Discord, board and generated-main writers are not appropriate
for the fork. Keep only explicitly reviewed fork checks active. Never place write credentials in
a job running Rust build scripts or a pull request's code. New workflows belong only on the
multiplayer branch; main stays byte- and history-identical to upstream.
