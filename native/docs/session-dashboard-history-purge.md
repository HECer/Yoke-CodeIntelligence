# Explicit physical purge of one document history archive

The committed-frame purge has source-scoped Windows CLI/Session acceptance,
recorded below. Actual interrupted purge, cancellation/recovery and unverified
unlink attribution remain separate qualification gates. This contract does not
claim release certification or full STATUS completion. It reuses the existing
history archive workflow; that workflow's acceptance gate remains independent.

The standalone host CLI/API can review and remove exactly one explicitly selected
**committed** archive frame. It retains the exact archive manifest and operation
header, original archive event chain and completion marker, binding migration
evidence, and new purge receipts. Configuration/source bytes and the persistent
owner record are not purged. Cancelled or unfinished archive operations are
ineligible. This adds no dashboard mutation, automatic maintenance or background
execution.

## Concrete review and separate permission

Both complete host document selections, their independently constructed write
authorities and a separately selected existing private archive root are required.
Serialized plans are inert data. Archive approval cannot authorize purge: completed
archival has already removed the originals that archive approval checks.

```text
yoke host-documents --from-selection old.json --to-selection new.json history-purge-plan --archive-plan committed-history.json --archive-root C:\HostState\archive
yoke host-documents --from-selection old.json --to-selection new.json history-purge-apply --plan reviewed-purge.json --archive-root C:\HostState\archive --confirmed-plan EXACT_PURGE_PLAN_ID --allow-host-writes --allow-state-deletion
yoke host-documents --from-selection old.json --to-selection new.json history-purge-inspect --plan reviewed-purge.json --archive-root C:\HostState\archive
yoke host-documents --from-selection old.json --to-selection new.json history-purge-recover --plan reviewed-purge.json --archive-root C:\HostState\archive --expected-event CURRENT_PURGE_EVENT_ID --direction resume --allow-host-writes --allow-state-deletion
```

Save the plan's exact UTF-8 JSON and review its archive selection, frame physical
identity, security fingerprint, SHA-256 and length, fresh source/owner witnesses,
both current closed-journal revisions, and individually identified retained
receipt witnesses. Applying requires exact plan-ID confirmation, independent host
write authority and separate explicit state-deletion permission. The library uses
opaque approval and deletion-permission objects that cannot be deserialized.
The independent `--history-record-budget` selection still precedes the subcommand
and must match the selected archive package (default 512, allowed 32–512).

The purge plan captures **current** source bytes/revision and owner witnesses.
A legitimate destination save after archival can therefore receive a fresh purge
review. Later source, owner, destination generation, journal, root, receipt or
frame changes invalidate that review. New purge requires the active exact
destination owner of the completed migration; a retired/different owner cannot
authorize it. Historical receipt inspection does not grant current owner authority.

The existing schema has no archive creation timestamp, retention duration or
expiry. No retention-age policy is inferred. There is no document archive
consumer/pin registry. Dependency checks cover the supplied complete selections,
their admitted journal namespaces and the recorded migration/archive relationships;
they do not claim discovery of every possible external consumer.

## Guarded mutation and finite recovery

Existing canonical frame admission checks the entire immutable frame, exact
lengths/hashes, original canonical journal plans and complete event chains, terminal
phases and selected lineage. No live source can replace missing record evidence.
Original journal records must remain absent. Unknown same-binding journal records,
unfinished selected journals, occupied backup/output roles and pending binding,
archive or purge maintenance refuse. Sibling packages remain independent.
On Windows, reserved journal and purge namespaces are identified with ASCII case
insensitivity and then require exact canonical spelling. Case-variant orphans
cannot disappear from dependency review; distinct Unicode names are not folded.

The original disjoint-root and physical ancestor checks, held analyzed-root/target
bindings, deterministically ordered journal locks and `.yoke-history.lock` remain
in force. The frame is acquired with its reviewed physical identity through
`read_owned_file(..., delete_access=true)` and checked for exact bytes/security
while held. Existing readers prevent acquiring DELETE access. The only content
deletion uses that held frame's `delete_owned` primitive. Foreign replacement
occupants, reparse points and hard links refuse and remain untouched.

Before mutation, archive-root entry/byte capacity reserves the complete new header,
remaining events and publication temporary. Future event/completion outputs must
be absent. Mutation replies require at least 8192 bytes of output budget before
creating a purge header or unlinking. Existing bounds remain 16,000,000 frame
bytes, 256 archive-root entries and 64,000,000 archive-root bytes. Purge headers
retain the 512,000-byte journal record ceiling and explicit event headroom. The
active journal's 2,048-entry/16,000,000-byte limits are unchanged. Capacity refusal
never becomes empty successful maintenance; recovery reserves its remaining work.

A new canonical header and target-specific pending blocker are flushed, published
without replacement and read back through the original durable record primitive.
The blocker prevents ordinary writes/recovery and further binding maintenance.
The purge uses a separate, bounded hash chain, leaving the v1 archive chain intact:

```text
prepared -> deleting -> removed -> committed
    |
    +------> cancelled
```

`deleting` is durable deletion intent. A successful owned unlink followed by
durable `removed` progress records confirmed removed content bytes. If a process
stops after unlink but before durable progress, inspection and reviewed recovery
report **unverified removal bytes**. Mere absence never proves which process
deleted the frame or upgrades unverified removal into confirmed attribution.
Completion retains a per-operation marker by guarded rename of the blocker.
Header/event/completion evidence is never deleted by this workflow.

A completed cancellation receipt stays inspectable during a later purge of
the same reviewed frame. Its own confirmed/unverified removal totals remain
zero. A validated subsequent operation is identified separately by
`subsequentPurgePlanId` and `subsequentPurgeComplete`; another operation's pending
marker is not the cancelled operation's marker. A pending cancellation still
blocks overall archive completion until its own completion marker is retained.
Frame absence without an independently admitted subsequent purge still refuses.

A response lost after preparation requires inspection and exact current-event
review. Before deletion intent, explicit `--direction rollback` cancels while
retaining the exact frame. Afterwards only reviewed resume is allowed. Stale
events, missing post-intent blockers, changed live witnesses, occupied outputs
and damaged/foreign evidence refuse. Completed receipt replay retains historical
semantics and does not write source or revive ownership.

## Reading after purge

Archive inspection includes retained purge status and
`archivedRecordEvidenceAvailable:false` when admitted purge evidence explains
the missing frame. Missing frames without a valid selected purge remain errors.
Pending or unconfirmed purge completion is explicit. Exact record reads after
recorded purge fail with an explicit unavailable-evidence error. They never
return successful empty bytes, rebuild original records, substitute current
configuration or read a foreign frame occupant.

Purge inspection retains exact original archive/migration/completion witnesses
without requiring current live source/owner bytes merely to read a historical
receipt. Confirmed removed content bytes and unverified removal bytes are
separate. Measured filesystem space is unavailable and secure erasure is not
claimed. `fileChanged:false`/`sourceWritten:false` refer to the selected authored
document, whose bytes this maintenance operation never changes.

All mutation fixtures use owned temporary data. Real user-data deletion requires
its own concrete host review and explicit deletion permission. The source task
and this document grant no such runtime authorization.

## Source-scoped Windows qualification

At native source `704b647d`, two independent native/Serena fixtures each pass 95
actual CLI cases and 70 exact record readbacks. A fresh, separately authorized
purge removes one 28-record frame while preserving its 14-record sibling and all
retained receipts. Missing write/deletion grants, wrong confirmation and stale
review refuse with the intended raw exit/cause and no mutation. Confirmed removed
content is 31,666 frame bytes per fixture; unverified removal is zero, measured
filesystem space remains unavailable and secure erasure is not claimed.

After generated-owner retirement, applying the cached sibling purge plan refuses,
all sibling record reads remain exact, and historical purge/archive inspection
still succeeds. All four owning Sessions and their Job exit naturally before
forced cleanup. The separately reviewed consumer's 72 guards pass without skips;
guard counts are not runtime coverage. See [actual source-bound receipts](../../docs/validation/integration-2026-10-26/cross-maintenance-live/README.md).

These owned temporary fixtures do not qualify actual interruption, purge
cancellation/recovery, unknown unlink attribution, older binaries, browser/mobile,
all possible external consumers, a matching current release/VSIX or full project
completion. The executable retains source 704; relevant-source byte equality with
later documentation checkpoints does not relabel the build.
