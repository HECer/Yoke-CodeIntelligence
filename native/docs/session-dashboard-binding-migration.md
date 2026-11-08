# Explicit global document write-binding migration

Native 0.5.0 provides a trusted host CLI/library workflow for one selected global
file. The dashboard reports blockers but cannot supply selections or authority.
[Acceptance and limits](../../docs/validation/integration-2026-10-24/dashboard-binding-migration/README.md)
record the qualified implementation and actual host checks.

A host-reviewed migration holds complete explicit old and new document selections,
their independent write authorities and registered analyzed roots. Both targets
must resolve to the same physically guarded global file. Project files are
ineligible. Current document identity, exact bytes, security, owner metadata,
journal generations and immutable review remain preconditions. Both journal
namespaces must be bounded and free of unfinished work for the selected target.

The trusted selection JSON has `documents` containing the exact host profile's
`dashboard.management.documents` object, and `projects` containing the complete
registered root set in original order, including the primary project. For example:

```json
{
  "documents": {
    "documents": [{
      "id": "host-settings", "label": "Host settings",
      "path": "C:\\HostState\\settings.yaml",
      "scope": {"kind": "global"}, "writable": true
    }],
    "journalRoot": "C:\\HostState\\private-document-journals",
    "maxDocumentBytes": 64000
  },
  "projects": [{"name": "default", "root": "C:\\Projects\\Example"}]
}
```

Supply all configured documents and all registered projects (1–32), not a reduced
target-only selection. Absolute global files and existing private journal roots
must remain outside every analyzed root. A complete-selection hash change can
require a separate migration for each affected global target.

## CLI workflow

Place selection flags before the subcommand:

```text
yoke host-documents --from-selection old.json --to-selection new.json binding-plan --from-document host-settings --to-document host-settings
yoke host-documents --from-selection old.json --to-selection new.json binding-apply --plan plan.json --allow-host-writes
```

The first command emits the raw plan JSON. Save exact UTF-8 stdout as `plan.json`,
review it, then explicitly apply. Preserve both selections, plan and original
journal directories. Mutation permission is checked before reading inputs.
Finite input/output/deadline limits still apply. Both independent write
authorities and concrete approval are required; serialized fields grant nothing.

The changing operation retires the old selection by installing the new binding.
It changes only owned binding metadata. It never edits configuration bytes,
reloads services, moves/deletes journals, adopts an unknown binding or grants
authority from serialized data. Old/new selection files are trusted host input,
never paths or grants supplied by a dashboard request.

Before metadata mutation, durable intent records the reviewed old/new binding,
original metadata identity/security and exact bytes, source observation and
journal state. A fixed pending marker blocks ordinary writes/recovery until the
migration is explicitly resolved. Binding metadata is rewritten through its
original held file, without removing its directory entry or changing identity.
An empty or partial record therefore remains occupied and fails closed.

Successful migrations add a new generation to the owner format. Older binaries
that do not understand the format must refuse it. A later return to an earlier
selection still receives a distinct generation. The later proposal-generation
fix binds ordinary cached proposals to that generation; the original migration
acceptance alone did not cover their revival path.
Freshly created, never-migrated bindings retain their existing serialization.
The existing format is schema1; migrated schema2 includes the fresh generation.
Migration retires the old write selection. Independent read grants remain.
Explicitly open a new host using the new complete profile to adopt the new write
binding. Existing Sessions do not reload. A later save through that host is a
separate configuration-file write.

## Interrupted migration

Inspect the original journals, review the current event SHA, and explicitly
choose `resume` or `rollback`:

```text
yoke host-documents --from-selection old.json --to-selection new.json binding-inspect --plan plan.json
yoke host-documents --from-selection old.json --to-selection new.json binding-recover --plan plan.json --expected-event CURRENT_EVENT_SHA --direction resume --allow-host-writes
```

Explicit recovery independently requires both current host authorities and the
reviewed durable event. A chosen recovery direction is durable and cannot be
reversed. Only exact recorded metadata or admitted prefixes from the same owned
file may be completed/restored. Foreign bytes, changed identities/security,
occupied names, unavailable state or changed source refuse and remain preserved.
Terminal metadata/history is retained; there is no automatic retirement cleanup.
Events are hash chained, closed-schema and bounded to sixteen slots; retry reuses
fixed intent. Completion uses a create-only rename to retained terminal history.
Pending migration blocks ordinary binding, writes and recovery. Dashboard Pending
reports `blockedByBindingMigration: true` and `complete: false`, retaining a
copyable read-only draft without migration actions. After completion the old
selection reports `blockedByDifferentAuthority`. Never manually remove or
overwrite owner/journal metadata to bypass these checks.

This scope implements per-target migration and retirement of the preceding write
selection. Standalone owner deletion, journal-history removal, broader journal
UI, automatic cleanup and full platform/release acceptance remain open. No
default model call, project execution or system privilege change is introduced.

## Cached proposals across a return migration

The [current generation acceptance](../../docs/validation/integration-2026-10-24/dashboard-document-generations/README.md)
records an actual counterexample against the preceding CLI and corrected native,
Serena, persistent Python and browser checks. Global proposals now bind the
owner generation separately from file/Session revision and recheck it under the
write lock. Fresh explicit review is required after returning to a selection.
Old generationless journal recovery stays explicit and separate from ordinary
proposal approval. Never-migrated/project/absent-field serialization stays exact.
