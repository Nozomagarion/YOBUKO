---
description: Verified guest take uploads into a host-ordered LAN session, preserving source files and independent players
read_when: multiplayer guest recording upload, protocol 10, bidirectional take publication
type: spec
status: draft
last_verified: 2026-10-27
covers:
  - crates/collaboration/src/host.rs
  - crates/multiplayer
  - crates/ui/src/app/multiplayer.rs
---

# Publish a finished take from either participant

Self-authored fork contract. Protocol 8 extends the explicit Publish take workflow to the
admitted editor guest. Record/finish in an ordinary local Studio tab, choose a loaded sound,
an existing shared audio track and start beat, then Publish loaded take. It publishes the
whole decoded sound as an unwarped canonical WAV, without source trims/effects. Native file
version 3 and portable-copy format 2 remain. Direct shared-tab recording, microphone streaming,
automatic reconnection and additional guests remain separate work. Explicit host invitation renewal
is defined by the [LAN contract](lan-starter-session.md#explicit-invitation-renewal).

Guest preparation runs on one bounded background job while its network worker continues
polling. A selected AudioData handle is trusted local input, never a remotely chosen source
path. A private local copy is leased until confirmation. Only one local publication is pending;
cancellation drops unused copies and the source remains untouched.

After authentication, the guest offers exactly author sequence, draft base revision, delivery
cursor, track, start and strict audio metadata. The host assigns the author from its TLS
connection. Hash/format/size bounds, role, sequence/retry rules, revision, native placement and
portable project/asset quotas are checked before requesting bytes. A read-only trusted-core
preflight shares the same rules as commit and consumes no counters. The host reuses an exact
already verified hash/metadata entry; otherwise it pulls exact-offset 16 KiB base64 chunks.
No peer path, filename, supplied identity, arbitrary command or device enters this transfer.

Limits remain 64 unique session sounds, 64 MiB per WAV, 256 MiB total, 2 MiB document and all
musical quotas. Transfer I/O has a 180-second absolute deadline and does not hold the canonical
mutex. Create-new staging, exact sizes/offsets, BLAKE3, bounded RIFF parsing, metadata and exact
canonical float32 WAV re-encoding precede collection and commit. Malformed/incomplete transfers
disconnect and clean temporary files without adding a clip or consuming a revision/sequence.
The one-use invitation prevents retrying a malicious transfer through automatic reconnect.

After verification the host checks revision/quotas again and orders one clip.insertAudio under
the guest author. A concurrent edit can make the offer stale: refuse it and discard staged
copies rather than rebase or overwrite. The raw edit message still cannot submit audio insertion.
Exact accepted retries produce one clip and consume neither extra revision nor disk budget.

New decoded files and their operations reach the host UI in one bounded state event. Its queue
retains pending file leases when full; unconsumed/failed events clean up. The canonical asset
catalog makes verified sound available for later snapshots. Neither existing session nor player
is replaced. The guest validates that a successful acknowledgment contains its expected native
operation before retaining its local collected copy. The host marks that uploaded sound known
to its sender so it is not immediately downloaded again; normal missing-assets transfers still
precede all other incoming musical edits. Existing hashes are reused within cumulative limits.

UI progress distinguishes preparing/sending from receiving. Both people save their own shared
copy explicitly; the existing local collection mappings preserve paths for save/backup/reopen.
Tests cover actual TLS, both authors, native recording after join, exact renders, players,
save/reopen, dedup, stale races, replay, malformed offers/chunks/WAV/hash, cumulative quotas,
interruption, unclaimed-lease cleanup and native visible controls. Household two-PC testing
stays deferred; automated loopback is not that acceptance test.


## Amendment: several guest authors

Protocol 12 supports [up to three guests](lan-multiple-guests.md). Each connection keeps its own
verified-asset inventory and author sequence. Any guest can publish a finished take; all other
connections receive missing verified audio before its operation. Concurrent edits/uploads may
make a draft stale at the final recheck: the refused author retains its source and can resend
with refreshed revision. Global cumulative quotas remain; a departure does not reset them.


## Compatibility amendment: Suhina update of 28 October

[Protocol 13](lan-upstream-20261028.md) supersedes earlier version pins in this contract: native
project v4, portable-copy format 3 and matching pinned Suhina engine identities are required.
Ordinary old project files remain readable through the studio reader, then share as v4. The
musical edit permissions and quotas above remain unchanged; newly added studio effects are not
implicitly permitted in shared tabs. All participants need the updated combined application.


## Amendment: heavy sessions

Protocol 15 replaces the 16 KiB base64 upload chunks and the limits above with the streamed,
verified and resumable transfer of [heavy sessions](heavy-sessions.md); an upload the host's
sound cache already holds is not requested.
