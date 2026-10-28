---
description: Integrating pinned Suhina and rbxport sources while preserving fork modules and distributing one guarded update channel
read_when: update the multiplayer fork, rbxport source updates, common app distribution, release packages
type: howto
status: draft
last_verified: 2026-10-26
covers:
  - fork-sources.json
  - scripts/fork_sources.py
  - scripts/fork_package.py
  - scripts/test_fork_distribution.py
  - crates/update/src/components.rs
  - crates/update/src/distribution.rs
  - crates/update/src/distribution/tests.rs
  - .github/workflows/multiplayer-release.yml
---

# Fork distribution

Source integration and binary installation are separate. Friends install the same combined
distribution. Their apps never install an official Suhina or standalone rbxport binary over
the fork. A new upstream version first becomes a candidate with the custom modules retained,
then passes regression gates and platform checks before a combined package can be published.

## Pinned integration

`fork-sources.json` records both repositories and full commits, plus hashes of every imported
rbxport analysis source. The Suhina commit must be an ancestor of the fork; rbxport is vendored
as the analysis subset only. `crates/collaboration`, `crates/multiplayer` and `crates/dj` stay outside that tree.

```sh
python3 scripts/fork_sources.py check
python3 scripts/fork_sources.py prepare --directory ../suhina-integration-candidate
```

Prepare fetches both official sources and creates a new worktree/branch from
`feature/multiplayer`. It merges Suhina preserving ancestry, imports the pinned analysis tree
and updates source identities. It changes neither the caller's checkout nor main, and never
pushes or publishes. A conflict stops in that isolated candidate. Changed analysis dependencies
require review of the adapter. Commit the refreshed pins/vendor/lock after review, run all
quality gates, then merge that candidate into feature/multiplayer preserving ancestry.

`main` remains the upstream mirror. No scheduled workflow is silently installed on it.
**Periodic source integration is not activated.** The owner selected feature/multiplayer as
the repository default branch on 2026-10-26 so the combined app is visible on the repository home.
A future reviewed schedule can therefore live on that branch, or run the prepare command from
a dedicated maintenance host/repository. Changing the default alone does not schedule any work.
Failed candidates must never be promoted by the scheduler. Main is only advanced by verified
fast-forward, never force/reset.

## Binary channel

The updater only reads Nozomagarion/suhina-daw. The regular channel uses release tag
`multiplayer` and `suhina-multiplayer-<platform>.zip` or `.tar.gz` assets. The release title ends
in the exact binary version; target_commitish must be the full fork commit. Every download
requires GitHub's SHA-256 digest. Test artifacts are from the same fork and have the same
package requirements; legacy unmarked artifacts are not eligible.

Before any running file is replaced, the unpacked package must contain
`DISTRIBUTION.json` beside standalone executables, or in Contents/Resources in a Mac bundle.
It names this distribution, exact version and source commit, supported multiplayer API and DJ catalog versions, and full Suhina/rbxport source commits. The marker
prevents accidental cross-distribution replacement; it is not a cryptographic signature.
GitHub HTTPS/repository ownership and the archive checksum remain the trust boundary.

The check occurs once per start when enabled. Download/install and restart remain explicit
actions. It does not restart during a session. No combined public release exists merely because
the updater is configured. A missing/unpublished channel is a failed availability check and
does not fall back to official binaries. Local builds do not automatically offer a nightly.

## Candidate packages and activation

The reviewed `multiplayer-release.yml` workflow builds tagged candidates on Mac Universal,
Windows x64 and Linux x64. Use an explicit tag of the form
`multiplayer-v0.0.1-nightly.YYYYMMDDHHMMSS+COMMIT7`. All platforms share that version; the
14-digit UTC timestamp orders same-day builds. The tag's commit prefix must match COMMIT7.
Workspace fmt/clippy/tests, source verification, documentation and dependency checks are gates;
known baseline failures block distribution too. There is no implicit skip/override.

`fork_package.py` requires all three executables and adds the distribution marker, source pins
and rbxport notices. For a Mac app pass Contents/MacOS; it places metadata in Contents/Resources
before the final ad hoc signature. Non-code files must not sit in MacOS: their detached signature attributes would
be lost by the updater's archive extraction. Resource signatures survive ordinary ZIP/tar extraction.
Candidates contain corresponding source archives at the exact commit. The workflow uploads
candidate artifacts only; it has no write token and does not publish a release.

Activation requires enabling the reviewed fork workflow, testing candidate installation on
Mac and Windows, then publishing those exact assets on the fork's `multiplayer` release with
the exact version title/full target commit and corresponding source archive. Do not publish
development artifacts whose embedded version or distribution marker differ. This is the
remaining operational step before friends can receive combined in-app updates.

## Compatibility

The marker's multiplayer_api=1 preserves the trusted collaboration contract. The separate LAN
adapter has protocol 13 and MIDI-copy format 3, checked during invitation decoding, the TLS
hello and document offer. Protocol-1/2/3/4/5/6/7/8/9/10/11/12 invitations are refused. The bounded MIDI adapter is pinned
to native project format 4; the source update reviewed the Bezier representation and old-file
migration. Hello also verifies the exact compiled pinned Suhina source revision before admission. Future wire changes must change that handshake contract. Combined distribution updates
do not imply that full project/media transfer or broader remote collaboration exists.

## Visible GitHub update button

Settings > General starts with an Updates section and **GitHub updates…**. The button opens
and checks the combined release plus three source components: Suhina main, rbxport main and
the fork's feature/multiplayer branch. Each row compares the included commit with GitHub and
links to that project. Upstream differences say Integration needed; a newer fork source says
New development commit, which does not imply that a binary release exists. Failed source
checks say Unavailable independently of the others and of the combined release check.

These three additional anonymous branch requests occur only after an explicit action from
Settings or Check now. They run on a background thread with the inherited connection
and request timeouts. Local fork revisions come from the existing build identity; the two
upstream revisions come from the pinned source file. The install action always uses the
combined fork distribution. The dialog never installs an upstream component separately.

To check: open Options > Settings (Cmd+, on Mac), choose General, and click GitHub updates…
at the top. The dialog shows all three projects and a combined-release status. Check now
refreshes them. GitHub links open the matching repositories. If no combined release is
published the dialog says so; turning off startup checks leaves this button available.
