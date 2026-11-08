# Selected private raw log windows

The native Windows dashboard can read explicitly host-selected private log files.
This is independent of structured activity, session controls, memories, language
selection and configuration documents. It neither creates a logger nor captures
tool arguments, command output, errors or results automatically.

An optional closed HostProfile selection is required:

```json
{
  "dashboard": {
    "management": {
      "rawLogs": {
        "logs": [
          {
            "id": "owned-session-log",
            "label": "Owned private session log",
            "path": "C:\\Owned host logs\\session.log"
          }
        ],
        "maxWindowBytes": 64000
      }
    }
  }
}
```

The host selects 1–16 absolute file paths, stable IDs and nonempty labels of at
most 200 UTF-8 bytes. The complete selection is at most 64,000 serialized bytes;
`maxWindowBytes` is 1–64,000 (default 64,000). Absent/null `rawLogs` preserves
previous serialized configuration and fingerprints. An explicit selection binds
the host fingerprint, physical parent identities and every registered analyzed
root. Only directory metadata is inspected during setup; a missing selected log
is allowed but remains unavailable until it exists. No log payload is loaded by
opening the Session or dashboard, fetching state, or refreshing the page.

The selected file must be outside all registered analyzed roots. This is checked
against held physical ancestor identities, including Unicode case aliases.
Reparse points, hardlinked files, traversal, streams, unsupported path namespaces
and replaced parent/root identities refuse. The guarded backend is Windows-only;
an explicit selection on another platform fails instead of claiming an empty
complete result. Existing Windows symlink privileges are not changed.

The separate management driver and private dashboard capability are also
required. The closed action is:

```json
{ "kind": "readLog", "logId": "owned-session-log", "maxBytes": 8192 }
```

`maxBytes` is optional and cannot exceed the host-selected window limit. No
request can supply a path, command, offset, registration or new grant. The
original owning Session checks its current project/state, root binding, current
`ci_session` policy, cancellation, revocation and original deadline before
payload admission, then checks current state again before returning the window.
Session controls and memories can both be disabled while this independent read
selection is enabled. Another HTTP principal or Session receives no authority
from an observed selection or a foreign dashboard capability. A host selecting
the same log for multiple principals explicitly shares that entire file; this
interface does not filter its contents by principal.

Each explicit read opens one file generation with read/write/delete sharing so
an active logger can continue writing or rotating. It observes the initial size,
seeks to `max(0, initialSize - maxBytes)` and reads at most the initially selected
range. It never follows later appends or switches to a replacement filename
during that observation. The observed range is returned with a fresh observation
ID, opened file identity, initial/final sizes, byte length, SHA-256 and exact
Base64 bytes. Sizes and exclusive `[startByte, endByte)` offsets are decimal
strings so JavaScript cannot round large integers.

Log directory handles allow read/write sharing, while withholding delete
sharing to pin the selected parent and ancestor names. This also avoids blocking
Win32 child-file rotation through a directory writer handle. Configuration-file
directory and file guards retain their stricter original sharing rules.

An active writer may truncate, overwrite or rotate the file during a read.
`immutableSnapshot` and `currentPathGenerationGuaranteed` are always false,
including when both observed sizes match. `shortRead` marks an early EOF relative
to the initially selected range. An empty partial range after truncation is not
a statement that the whole file is empty; a missing or unreadable file returns
an explicit refusal. Separate observations keep separate IDs even if their
bytes match. No whole-file hash, atomic snapshot, semantic interpretation or
completeness guarantee is inferred from the window.

The Logs tab requires an explicit **Read latest byte window** action. It shows
the range, generation, observation ID and raw-byte hash. Text is a lossy UTF-8
preview when the window contains invalid bytes or starts inside a character;
this is disclosed. Base64 remains authoritative, including BOM and line endings.
Log text is inert. Status refreshes never reread a log. A previous observation is
marked stale on a Session/project change; a response arriving under an obsolete
binding is discarded. No writes, rotations, retention changes, file removal or
automatic project execution are available through this grant.

Raw payloads never enter the Session state or structured activity. Only the
fixed `dashboard.logs.read` operation name, status and duration are recorded.
Configuration document and language management operations also have fixed
allowlisted labels; arbitrary caller text still becomes `other_operation`.

The [scoped acceptance](../../docs/validation/integration-2026-10-24/dashboard-raw-logs/README.md)
records 747 native passes, current CLI stdio/HTTP acceptance and actual browser
DOM workflows. Inspected synthetic fixtures and captured-native-DOM pixel
rerenders are explicitly distinguished from native screenshots. The earlier
rotation failure and fixture parser failure are preserved separately. Broader
dashboard, platform and provider parity gates remain independent.
