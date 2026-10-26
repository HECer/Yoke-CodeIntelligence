# Explicit logical retirement of one global document owner

This native 0.5.0 source change adds a host-only library/CLI workflow for retiring
the write eligibility of one selected GLOBAL document. Windows terminal CLI and
owning-HTTP acceptance is recorded below; broader qualification remains open.
It does not prove that every live host released that document. The original
complete host selection, its registered analyzed roots and original physical
journal namespace, plus the existing `WriteAuthority`, authorize this explicit
logical change. Project documents and foreign or reduced selections are refused.
There is no second destination selection or snapshot-retention deletion grant.

The owner entry stays at `.yoke-doc-owner-<target-hash>.json`. Retirement rewrites
that same held file into a restrictive format-3 tombstone, preserving its physical
identity, security, original selection/journal binding and optional generation.
A fresh retirement nonce distinguishes the terminal record. Ordinary format-1/2
owner readers reject this schema; it cannot resemble never-owned absence.
No owner unlink, automatic adoption, reactivation or rebinding is provided.

Configuration bytes (including BOM, CRLF and Unicode), independently granted
reads, running Session configuration, source snapshots, journal records and
archive evidence remain unchanged. A valid owner created before any journal
header can be explicitly retired by its original authority under the same lock.
An empty journal alone grants nothing. Existing writers must release the original
journal lock before this operation can proceed; no inventory of other live hosts
or claim that all of them exited is inferred.

## Reviewed host workflow

Use the same complete selection JSON as the
[binding migration workflow](session-dashboard-binding-migration.md), including
every selected document and every registered root in original order. Pass the
original selection before the subcommand; standalone retirement accepts no
`--to-selection`:

```text
yoke host-documents --from-selection original.json owner-retirement-plan --document host-settings
yoke host-documents --from-selection original.json owner-retirement-apply --plan retirement.json --allow-host-writes
yoke host-documents --from-selection original.json owner-retirement-inspect --plan retirement.json
```

Save exact UTF-8 plan stdout as `retirement.json` and review it before applying.
The plan is inert data. It binds exact source bytes/revision, source and parent
identity, source security, owner bytes/identity/security, the optional generation,
original selection/root binding and bounded journal observations. Apply rechecks
those preconditions under physical guards and the original exclusive journal
lock. A deserialized plan, hash or confirmation flag cannot construct the typed
host authority or concrete approval.

Unfinished original document journals, orphan events, unknown document roles,
unattributed output/backup names, pending migration/archive/retirement markers,
occupied intent/terminal roles, missing evidence, changed roots/files/security,
links and capacity/deadline failures refuse while preserving occupants. All
original-selection journals are replayed. Other retained journal evidence is
bound by exact bytes, identity and security without being promoted to authority.
Parent scans conservatively refuse unrelated-looking document stage/backup
names whose target association cannot be established.

## Durable transition and recovery

Capacity is reserved before mutation. A create-only immutable intent records the
review; a fixed pending marker blocks ordinary writes/recovery/migration before
the owner rewrite starts. The owner name remains occupied even after an empty or
partial interrupted rewrite. Canonical, closed-schema, hash-chained events have
sixteen slots. Completion commits a terminal event and retains its marker using
a create-only rename. Original evidence is never deleted by retirement.

On any unconfirmed result, preserve the original selection, plan, fixed owner
entry and journal root. Inspect, review the current event SHA, then explicitly
choose the recovery direction:

```text
yoke host-documents --from-selection original.json owner-retirement-recover --plan retirement.json --expected-event CURRENT_EVENT_SHA --direction resume --allow-host-writes
```

`rollback` is available only before terminal commitment. Recovery requires the
original `WriteAuthority`, exact current event, source/owner/root preconditions,
unchanged retained evidence and enough capacity for its terminal event. Its
direction is recorded before metadata mutation and cannot reverse. Only exact
recorded before/after bytes, or their prefixes while writing is recorded in the
same owned file, are admitted. A direction-fixing event can precede an
interruption without replacing the preceding rewrite's partial bytes. Foreign
bytes, replaced files, missing markers after writing starts, conflicting roles
and uncertainty refuse.
Repeated recovery with a fixed direction reuses its recorded intent. A terminal
response loss is inspectable; retry only completes retained marker publication
and does not rewrite the owner. A committed retirement cannot roll back.

Inspection reports `complete: true` only with matching terminal owner bytes and
retained terminal marker evidence. `Access::owner_retirement_blocked` reports a
pending marker or a validated retired tombstone without manufacturing write
authority; unknown/partial metadata without a marker remains an error. It never
reports that every live host is unbound. An ordinary cached proposal, legacy/generation-bearing journal recovery
or migration cannot restore a retired owner. Completed archive inspection and
exact retained reads continue using their independent immutable evidence. Pending
archive mutation still needs the current active owner/generation. Physical archive
purge is a separate workflow and receives no permission from retirement.

The owning Session dashboard checks this blocker before ordinary owner lookup.
It returns `blockedByOwnerRetirement:true`, `complete:false` and an empty blocked
journal view, rather than claiming a complete empty recovery result. The client
retains the draft and disables document writes/recovery. It offers no retirement
or ownership-restoration action. Terminal owning-HTTP checks are recorded below;
actual desktop/mobile visual qualification remains pending.

## Qualification boundary

The implementation child's original checks covered source parsing/formatting and
diffs only. A later separate Windows live consumer now qualifies the terminal
single-selection CLI and owning HTTP behavior at native source `704b647d`: native
and Serena Sessions each pass eight fixed CLI cases, preserve seven original
journal records and exact configuration reads, inspect the committed schema-3
tombstone and exit naturally before forced cleanup. All 66 new protocol/helper
guards pass without skips. The owned foreign pending-marker test is presence-based
blocking, not actual interruption or retirement recovery qualification.
See [source-bound live evidence](../../docs/validation/integration-2026-10-26/owner-retirement-live/README.md).

Whole-library and CLI unit checks/Clippy retain their separately recorded source
scopes. Actual interrupted recovery, migrated owner generations, older-binary
refusal, retained archive reads after retirement and native desktop/mobile visual
qualification remain open. Tests include meaningful
temporary-fixture interruption, recovery, replay, archive-read preservation,
foreign protocol/path/Unicode/link and deadline/entry/byte/event exhaustion cases;
their presence is not a passing result. No real user-data maintenance was run.
Physical owner deletion, reactivation, all-host release proof and broader project,
platform or release acceptance remain open.
