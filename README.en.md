<p align="center">
  <img src="assets/fork/banner.svg" alt="Suhina Multiplayer + DJ — Create together. Prepare the set. Take it live." width="100%">
</p>

<p align="center">
  <a href="https://github.com/Nozomagarion/suhina-daw/tree/feature/multiplayer"><img src="https://img.shields.io/badge/status-experimental%20prototype-ffb18d" alt="Experimental prototype"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--or--later-63e5c1" alt="GPL 3.0 or later"></a>
  <a href="https://github.com/Nozomagarion/suhina-daw/actions/workflows/multiplayer-ci.yml?query=branch%3Afeature%2Fmultiplayer"><img src="https://github.com/Nozomagarion/suhina-daw/actions/workflows/multiplayer-ci.yml/badge.svg?branch=feature%2Fmultiplayer" alt="Fork validation"></a>
</p>

<p align="center"><a href="README.md">Français / product vision</a> · <a href="#what-you-can-try">Try the prototype</a> · <a href="#progress-you-can-audit">Progress</a> · <a href="#standing-on-the-work-of-others">Credits</a> · <a href="docs/fork-distribution.md">Updates</a></p>

**One place to make music together, prepare a set and perform it.** That is the ambition of
this independent fork. It combines Suhina's native music production foundation with a custom
collaboration layer and DJ preparation using selected rbxport code.

The destination is a complete production-to-live workflow. The current build is an experimental
prototype: LAN sessions share a bounded copy of an open MIDI project or a four-track starter, and the DJ workspace provides a
local audio catalog, analysis and two original-speed A/B decks. Full project coediting, direct shared-tab recording, automatic tempo sync
and USB export are future work. There is no common public release channel yet.

## The workflow we are building

| Create | Prepare | Perform |
|---|---|---|
| Produce with Suhina's engine, instruments and clip workflow. | Render a project into the DJ library; analyse tempo and key with rbxport code. | Build toward set preparation and live performance in the same app. |
| Collaborate on a common project while listening independently. | Keep the connection between your production and its rendered audio. | Keep everyone on the same tested combined version. |
| **Today:** restricted LAN MIDI notes, arrangement clips and mixer controls. | **Today:** native catalog, import, analysis and render bridge. | **Today:** two independent A/B decks, levels and crossfader; hardware export remains planned. |

The 29 October Suhina update integrates merged main 7753ffa3: 56 further commits covering
task-based settings, automation display fixes, style caching and shared DSP blocks. Old studio projects remain readable; new saves use native v4. LAN uses
protocol 13, portable format 3 and matching compiled Suhina identities. The shared device
subset stays FM Synth/Utility. [Compatibility](docs/spec/lan-upstream-20261028.md).

## What you can try

- **Studio / Multiplayer / DJ** entry points in the native app.
- **Create LAN session / Join LAN session:** separate single-use invitations connect one host and up to three guests.
  Share the current project (up to 64 tracks, FM Synth/Utility parameters only) or use the
  generated starter. Clips arrive in the verified initial copy; single-note add/update/delete, track names, volume
  and pan are live shared edits. The **MIDI notes** editor rejects stale drafts before indices shift. Play, stop and listening position stay local to each computer.
- **Launcher (protocol 13):** organize scenes and slots together, copy arrangement clips into empty
  compatible slots with independent contents, move/swap slots, and launch privately on each computer.
  Scene structural changes invalidate hidden selections; stale drafts require explicit refresh.
- **Devices:** add/remove top-level FM Synth and Utility without loading presets or replacing an
  existing instrument; chain edits discard old device selections. Edit parameters using generated SDK controls. Draft values
  are submitted explicitly, validated by the host and rejected after a newer revision until refreshed.
- **New invitation:** invite another guest while existing connections stay, up to three guests;
  rotating replaces the previous outstanding invitation. Guests: N / 3 shows occupancy. Renew admission without replacing the hosted
  music/player. Rejoining downloads the current verified copy into a new tab; the previous local
  copy and solo edits are retained, without merging. No automatic resume or Internet routing.
- **DJ library:** import audio, keep a persistent local catalog, analyse tempo/key and render a
  Studio project into the library. Create/rename local playlists, add/remove tracks, change their
  order and recover the sets after restarting. Audio/source files stay in place; deletion is
  confirmed. Revision checks refuse stale writes, and **Refresh DJ library** reloads another
  instance’s changes. Legacy format 1/2/3 catalogs are backed up before their first format 4 write.
- **Prepare:** audition a track from a chosen position through the existing preview/headphone output;
  add, rename, move and delete named cue points. Cues belong to the track across its playlists
  and survive restart. Prepare also loads a stereo waveform with cue markers and positions;
  click or drag to choose the next start without playing. Reload waveform retries a changed
  or missing file. Stop and leaving DJ cancel pending playback.
- **Beat grid:** edit a constant 20–400 BPM grid with a first-beat anchor and four beats per bar.
  Preview drafts before saving; revision checks protect stale edits and deletion is confirmed.
  Manual BPM stays separate from analysis and survives reimport. A/B performance decks now play original-speed tracks with pause/seek and a crossfader;
  deck headphone cues require a separate output pair. Sync, variable tempo and hardware export remain pending.
- **Settings → Agents and updates → GitHub updates…:** inspect the three GitHub source identities and check
  availability of a combined app update. An official standalone binary cannot overwrite the fork.

The network exchange is tested with real TLS loopback connections and two desktop project
instances. **A physical two-computer session has not yet been verified.** Loaded project audio can now be shared with **Share project + audio** as verified portable
float32 WAV copies (64 files, 64 MiB each, 256 MiB total). Original encodings/tags are not
archived. Either participant can publish a finished loaded take from another local Studio tab into an existing shared audio track using **Publish take**, without reconnecting. The whole sound is inserted unwarped; source trims/effects are not baked in. Guest uploads are pulled and verified by the host before publication. Recording directly into the shared tab and external plugins remain unsupported. MIDI notes and arrangement clips can be edited while connected. **Clips** supports create,
move, trim, duplicate, enable and delete with revision checks and stack-only placement.
**Tracks** lets either participant add an audio track or a MIDI track with default FM Synth while connected. All participants need protocol 13 and the same compiled Suhina revision.
[LAN contract and limits](docs/spec/lan-starter-session.md) · [Validation steps](docs/multiplayer-fork.md#try-the-desktop-lan-prototype)

## Progress you can audit

See the current [milestone table in the main overview](README.md#où-en-est-le-projet).

These bars count the explicitly listed criteria in [fork-progress.json](fork-progress.json).
They are **not percentages of engineering effort, stability or completion of the ultimate app**.
Implemented items link to code or validation evidence; pending items stay pending until their
acceptance gate is met. Rebuild the graphic with `python3 scripts/fork_readme.py`.

Portable media, shared notes/clips/tracks and verified take publication are covered by local
TLS, desktop and audio tests. Next development expands LAN participant capacity; automatic
resume and Internet sessions remain planned. Physical two-computer acceptance is deferred at
the owner's request and remains necessary before claiming a verified household session.

## One app, one update path

```text
Suhina upstream ─┐
                ├─ isolated source integration ─ checks ─ combined build ─ in-app update
rbxport upstream┘               ↑
                      custom multiplayer + DJ modules
```

Friends should install the same combined distribution. Source updates are integrated and tested
with the custom modules before publication. Installed apps download that tested package; they
do not compile random upstream changes or replace themselves with an official standalone app.

Already implemented: source pins, an isolated integration candidate tool, component checks in
Settings, package source identity and a guard before replacement. Still pending: publishing and
validating the common update channel, and activating scheduled integration. A GitHub source
change is information; it does not mean a compatible combined update has been released.
[Distribution and maintenance design](docs/fork-distribution.md)

## Standing on the work of others

This project exists because of substantial work by other open-source authors.

| Project | What comes from it | What this fork adds |
|---|---|---|
| **[Suhina](https://github.com/suhina-daw/suhina-daw)** | The DAW foundation: native interface, audio engine, project model, session commands, instruments, effects, plugin hosting, CLI and automation. | Restricted collaboration, the DJ workspace/bridge and combined distribution controls. |
| **[rbxport](https://github.com/chrisle/rbxport)** by **chrisle and contributors** | The pinned `rbl-analysis` sources used for offline tempo/key analysis. The vendored subset is recorded with its license and source hashes. | Integration into the local DJ catalog. The rest of rbxport is not yet integrated; this fork does not claim its full feature set. |

Read [Suhina's original README](UPSTREAM_README.md), [ATTRIBUTION.md](ATTRIBUTION.md),
[NOTICE](NOTICE), [rbxport provenance](third_party/rbxport-analysis/VENDORED.md) and the exact
[source pins](fork-sources.json). Original authors retain their copyright. This is an independent
experimental fork, not an official release or endorsement from either upstream project.
It contains no proprietary music-software code or content.

## Build and inspect

Use **`feature/multiplayer`** for the combined app. `main` deliberately stays an exact upstream
mirror, so the custom modules and this presentation live on the feature branch.

```sh
git clone --branch feature/multiplayer https://github.com/Nozomagarion/suhina-daw.git
cd suhina-daw
cargo test --locked -p suhina-collaboration -p suhina-multiplayer -p suhina-dj -p suhina-update
cargo build --locked -p suhina -p suhina-cli -p suhina-plugin-host
```

Follow [build instructions](UPSTREAM_README.md#building) for platform prerequisites. Development artifacts are available
from successful [fork workflow runs](https://github.com/Nozomagarion/suhina-daw/actions/workflows/multiplayer-ci.yml?query=branch%3Afeature%2Fmultiplayer).
Check the run's commit and platform results; a compiled app alone does not establish that every
validation passed. A stable installer, production support and signed/notarized distribution are
not yet offered by this fork.

## Help shape it

Useful next contributions are reproducible two-computer results, portable project/media design,
shared note/clip editing and DJ preparation workflows. Include OS, build commit, precise steps
and expected/observed behavior in reports. Keep invitations private; they grant access to the
session for their short lifetime.

Follow [CONTRIBUTING.md](CONTRIBUTING.md), the [fork guide](docs/multiplayer-fork.md) and
[security boundary](docs/security/session-sharing.md). Discuss fork work on this fork; the
upstream projects have their own direction and contribution rules.

**GPL-3.0-or-later.** Packages must include required notices and corresponding source. See
[LICENSE](LICENSE) and the [distribution contract](docs/fork-distribution.md).
