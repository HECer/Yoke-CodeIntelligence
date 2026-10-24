# Host-selected dashboard configuration files

This native 0.5.0 feature reads explicitly selected UTF-8 files and edits selected
global host files through reviewed proposals and conditional recovery. Selected
[project-file writes](session-dashboard-project-files.md) additionally require
host workspace writes and native exact approval/executed verification; they use
the original workspace writer and journals, never the global-file authority.
Saving file bytes does not reload or execute the file or change the running
Session's configuration. No configuration-file schema validation is performed.

## Independent host selection

The trusted host profile can select documents under `dashboard.management`:

```json
{
  "dashboard": {
    "management": {
      "documents": {
        "documents": [
          {
            "id": "host-settings",
            "label": "Host settings",
            "path": "C:\\HostState\\settings.yaml",
            "scope": {"kind": "global"},
            "writable": true
          },
          {
            "id": "project-settings",
            "label": "Project settings",
            "path": "C:\\Projects\\Example\\project.yaml",
            "scope": {"kind": "project", "project": "default"},
            "writable": false
          }
        ],
        "journalRoot": "C:\\HostState\\private-document-journals",
        "maxDocumentBytes": 64000
      }
    }
  }
}
```

Directories must already exist. Global targets and the journal directory must be
outside every registered analyzed root. Project targets must belong to exactly
the named registered root and are delivered only while that project is active.
File selection cannot be supplied by a dashboard request. `writable` defaults to
false; it is independent of memory writes and session controls. A writable global
file also needs the selected journal root and the current `ci_session` policy.
It does not grant workspace writes. A library host must install and poll the
management driver on the original Session owner thread; shipped stdio and
Session-backed MCP-HTTP drivers do so while idle.

Absent/null `documents` preserves preceding serialization and authority. An empty
explicit selection is invalid. This selected-document backend is Windows-only;
an explicit selection on another platform refuses during Session opening.

## Reading and reviewing

The Files tab lists selected global and active-project documents. Loading is an
explicit read. Snapshots carry exact UTF-8 text, byte length, content hash,
physical-file/security-bound revision and declared scope. Missing, non-UTF-8,
oversized, linked, replaced or inaccessible files refuse instead of returning an
empty complete document. A missing target can remain selected for recovery.

The editor preserves BOM and consistent LF/CRLF bytes. Mixed line endings remain
read only in the browser. The core proposal API accepts bounded exact UTF-8 text;
it does not normalize Unicode or evaluate file contents. Script-like text stays
inert. The original and proposed text and before/after hashes are shown for
review before the separate Save action.

Proposals remain in the original Session's memory for at most ten minutes, with
one latest proposal per document and at most sixteen retained proposals. They
bind the project, Session-state hash, file revision and exact after-bytes.
Submitting a fresh envelope cannot rebind an old proposal to a changed Session.
Another HTTP Session has its own proposals, even when the host deliberately
shares a global file. File revision checks prevent competing writes from
silently overwriting each other. Drafts remain visible but locked after Session
changes, unconfirmed writes or an observed pending recovery.

## Publication and recovery

A global save needs the separate host write selection and a concrete server-held
proposal. The writer holds physical directory/file bindings, retains security
metadata, reserves bounded journal capacity and writes an atomic plan/genesis
record. Prepared output is durably registered before retention or human bytes.
Publication uses no-replace moves through an original-file backup. Recovery
rechecks identities, hashes, security and the current event before any change.
Foreign occupants are preserved. Rollback requires the original physical file;
missing backups are never reconstructed from guessed content.

Inspect recovery displays recorded proposals and offers explicit resume or
rollback. A chosen direction cannot be reversed. A stale event review refuses.
Completed recovery is idempotent and reports no new file or journal changes.
Timeouts and interrupted responses require inspection/reload before another save.
Neither source files nor the running configuration are rewritten by this global
path. Journal payloads contain proposed configuration text, so the selected root
must be private host storage. Arguments and results do not enter structured
dashboard activity.

## Persistent write binding and limits

Before the first changing save, an opaque
`.yoke-doc-owner-<target-hash>.json` binding is created beside the global target.
It binds the physical parent/leaf, exact document selection and journal identity.
It remains after completion. Changing IDs, labels, limits, registered roots or
the journal selection does not silently migrate that authority or bypass older
journals. Restore the selection/root that owns the file to resolve unfinished
work. [Explicit host migration](session-dashboard-binding-migration.md) can review
both complete selections and retire the old write binding of one global target.
Automatic migration, standalone binding deletion and journal-history removal
remain unavailable. Foreign metadata refuses and is never silently adopted.

A different binding is reported explicitly as `blockedByDifferentAuthority`, not
as an available empty recovery list. The browser retains the draft and explains
the required current authority. Blocking migration metadata is separately
reported as `blockedByBindingMigration` with `complete:false`. Host-only reviewed
recovery resolves it; this page cannot migrate grants or reload its profile.
Removing/replacing host state externally is not a supported migration procedure.

Selections are limited to sixteen files and 64,000 bytes of serialized selection
configuration. File content is limited to 64,000 UTF-8 bytes, or a smaller host
limit. The existing 128,000-byte management request limit also applies to encoded
JSON. Plans are bounded to 512,000 serialized bytes. Journals use bounded entry,
byte, event and allocation-attempt limits and refuse when capacity is exhausted;
there is no automatic deletion. No model, provider, browser launch, privilege
change or analyzed-source execution is introduced by selecting or editing files.

## Private management messages

The existing authenticated envelope keeps `id`, `sessionStateSha256`, `project`
and `action`. File actions have `action.kind = "document"` and a closed `request`:

| request.kind | Required fields |
| --- | --- |
| `read` | `documentId` |
| `plan` | `documentId`, `expectedRevision`, `content` |
| `apply` | `documentId`, `planId` |
| `inspect` (project proposal only) | `documentId`, `planId`; reads existing host approval/verification |
| `pending` | `documentId`; optional SHA-256 `cursor` for project-journal pages only |
| `recover` | `documentId`, `journalId`, `expectedEvent`, `direction` (`resume` or `rollback`) |

Requests cannot provide paths, commands, grants or serialized approval objects.
Reading/planning is distinct from saving. Unconfirmed write outcomes are not
reported as unapplied. Capability responses distinguish configuration reading,
selected global editing and independently host-granted project editing. Existing default-mode
capability bytes remain unchanged when no document selection is present.

The [scoped acceptance](../../docs/validation/integration-2026-10-24/dashboard-documents/README.md) records 733 native passes, current CLI transports, actual native browser workflows and separately identified pixel rerenders. This is not a full dashboard, Windows baseline, cross-platform or upstream runtime certificate. At that checkpoint, document actions used the fixed `other_operation` fallback. The later [raw-log acceptance](../../docs/validation/integration-2026-10-24/dashboard-raw-logs/README.md) adds fixed allowlisted document/language/log labels; arguments/results remain excluded.

The later [project-file acceptance](../../docs/validation/integration-2026-10-24/dashboard-project-files/README.md)
connects exact selected captured project files to native plans and independent
host proofs. The UI shows both proposal and workspace IDs, requires a status
check and confirmation, and preserves drafts on refusal. [Project-journal recovery](session-dashboard-project-recovery.md) now permits
bounded exact-selection inspection and confirmed conditional rollback. Project
journals accept only `rollback`; other native/older unbound journals retain the
original host workflow. Empty pages never claim complete workspace recovery.
