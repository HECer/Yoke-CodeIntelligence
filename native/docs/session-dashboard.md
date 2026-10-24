# Native session dashboard

The native dashboard is an explicitly enabled session view, read only by default. The host
selects `"dashboard": {}` in its JSON HostProfile, then calls `open_dashboard`,
`cfg.open_dashboard()` or native session operation `openDashboard`. Existing
tool/API exclusions and optional-tool selection remain independent. Project
YAML, repository text, request arguments and another project query cannot grant
dashboard authority. No listener or browser starts for absent/null configuration.

A second host opt-in enables current-session management:

```json
{
  "notesPolicy": { "writable": true },
  "dashboard": {
    "management": {
      "sessionControls": true,
      "memories": true,
      "operationTimeoutMs": 30000
    }
  }
}
```

Management switches the active host-registered project, selects known modes or
restores layered host modes. It also lists, reads, creates, replaces, renames and
deletes current-project named memories. These use the native named-note store;
central AgentMemory coordination is separate. Global memory scope, arbitrary
paths, new registrations, commands, trust and source-write approval cannot be
selected through the browser. `notesPolicy.writable` still defaults false.
Current raw tool/API exclusions, ignored/read-only patterns, `no-memories`,
registered-project restrictions and independent execution/write grants apply.

An additional `management.languageServers` host allowlist enables project-local
provider selection and restoration of host defaults. Backend order remains
significant; an empty legacy selection disables semantic providers explicitly.
Registration alone grants no activation, and selection starts no process.
See the [selection contract and scoped acceptance](session-dashboard-language-servers.md).

A separate `management.documents` selection adds explicit UTF-8 file reads and
reviewed global host-file writes with durable conditional recovery. Independently
[granted project-file writes](session-dashboard-project-files.md) use native plans,
separate host approval/executed verification and original workspace journals.
The Windows guarded backend permanently binds each written global
target to its original selection/journal; migration and retirement are unavailable.
Saving bytes does not reload the running configuration. See the
[file contract](session-dashboard-documents.md).

A separate `management.rawLogs` selection enables explicit finite reads from
private host log files outside every analyzed root. No capture or polling is
started. Exact Base64 bytes, decimal offsets and opened-generation identity
remain distinct from a lossy UTF-8 preview. Concurrent files are not immutable
snapshots. See the [raw-log contract](session-dashboard-raw-logs.md).

A separate `management.shutdown: true` host grant and trusted built-in transport
binding enable [ending only the owning MCP Session](session-dashboard-shutdown.md).
Confirmation is explicit; acknowledgement does not claim exit. Other HTTP Sessions
and the service remain active. Library management alone cannot install a stop target.

The CLI `serve-session` host installs the owner-thread driver when management is
explicitly selected. Its native/Serena profiles expose opening operations.
MCP-HTTP installs its actor driver for Session-backed principals; the pure Engine
`native` HTTP profile refuses management. A library host must enable management
before opening and regularly call `Session::poll_dashboard_management`, including
while protocol input is idle. JSON cannot install a driver. The non-Send Session
stays on its owning thread; only bounded inert requests cross the mailbox.

Private `POST /api/manage` requires exact Host and same-origin Origin, Bearer
capability, JSON content type and canonical bounded Content-Length. Its closed
envelope contains a 64-hex request ID, current `sessionStateSha256`, current project
and a typed action. Unknown fields, duplicate JSON keys, global scope and
unsupported authority refuse. The owning thread rechecks session/project/root
binding, current policy, original deadline and revocation before execution.

Creation requires absence inside the existing immediate SQLite transaction.
Replacement, deletion and rename require the loaded note revision. Conflicts
cannot overwrite a newer head; immutable history, anchors and native reference
updates retain their existing policy. Expired queued tickets cannot execute later.
When execution might have started, timeout/disconnection is an unconfirmed outcome;
refresh saved state before retrying. The UI retains drafts on conflicts and
project/session changes and never silently rebinds them. Reload/discard and
destructive operations are explicit. Unchanged content is not rewritten, uniform
CRLF survives edits, and mixed line endings display read only with the native-API
alternative disclosed. Names and content render as inert text.

The result contains a private IPv4-loopback URL with a short-lived capability
in its fragment. The client opens that URL; `browserOpened` is false and
`browserOpening` is `client_managed`. The native process does not launch a browser
command. Do not publish or share the URL. The page removes the fragment from
history and retains the capability in that browser tab's session storage.

The dashboard shows current session/project/context/modes, host-registered project
access, the selected public compatibility tool catalog and language/workspace
selections. Registrations do not certify provider versions or semantic quality.
Native protocol catalogs and per-operation runtime/write grants remain separate.
It also shows bounded structured activity for native session operations observed
after opening, including pending/completed/failed state and timing. It does not
record arguments, results, source bytes, command lines or error messages. Unknown
operation text is replaced by a fixed label. This activity is independent of the
separately selected raw-log reader and does not count every public protocol
request. Evicted activity rows are disclosed.

Dashboard workspace recovery, global binding migration, tray integration and
automatic browser opening remain unavailable in this build. Their absence is disclosed rather than
as empty successful results. Source-write approval and host execution grants are
not changed through management. Existing guarded edit/approval/verification and
recovery workflows retain their independent authority.

Only the static bootstrap is public. Session data requires exact Host, allowed
Origin and constant-time Bearer capability validation. HTTP framing, headers,
routes, response sizes and request lifetimes are bounded. Mutations require the
separate management opt-in; request pipelining refuses. Source/session text is
rendered as inert text. No external
assets or network destinations are used. Each data request opens existing-only
read-only state and checks the exact workspace/session row against the view.
Concurrent changes refuse with 409 until the current host session supplies a
fresh view. Neither stored data nor the browser can select another root.

Accepted sockets explicitly use blocking reads/writes under the original bounded
deadline, including separately arriving headers and bodies. This avoids Winsock's
inherited listener nonblocking mode; see the
[accept contract](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept).
The accepted request is capped by remaining dashboard lifetime. Management queue
wait, execution and response share their original operation ceiling, also capped
by that lifetime; polling never restarts it.

Host bounds (camelCase) are:

| Field | Default | Allowed |
| --- | --- | --- |
| `port` | `0` (OS-selected) | `0`–`65535`, IPv4 loopback only |
| `requestTimeoutMs` | `1000` | `10`–`5000` |
| `lifetimeMs` | `3600000` | `100`–`86400000` |
| `maxRequests` | `10000` | `1`–`100000` |
| `activityCapacity` | `200` | `1`–`1000` |
| `management` | absent/null | explicit optional object |
| `management.sessionControls` | `true` when opted in | boolean |
| `management.memories` | `true` when opted in | boolean; write policy remains separate |
| `management.operationTimeoutMs` | `30000` | `100`–`60000` |
| `management.shutdown` | `false` | boolean; trusted owning transport binding also required |
| `management.documents` | absent/null | closed explicit selection; Windows guarded backend |
| `management.rawLogs` | absent/null | closed exact-file selection; explicit 1-64000-byte windows; Windows backend |

The listener starts only on the explicit open operation. Repeated opens reuse the
live instance; an expired instance is replaced with a fresh capability. Dropping
the owning session stops and joins its worker. A pending request can delay cleanup
only until its bounded request deadline. It never terminates a client/browser or
an unrelated process. No activity or capability is persisted to the application
database. SQLite may coordinate live WAL reads through its normal sidecars.
MCP-HTTP dashboards additionally share the transport session's revocation/expiry
signal, checked before data admission and response. Revoking that access closes
the listener independently of later actor/process cleanup.

Managed stdio also closes its listener on EOF. Actual catalog changes notify idle
stdio immediately; MCP-HTTP coalesces its notification into the next normal
response. Closing one HTTP session does not stop other sessions or the service.

Explicit configuration binds the host session fingerprint and stale admission.
Absent/null configuration preserves prior serialization, session fingerprints,
configuration replies and default tool-catalog bytes. The retained default
catalog description is upgraded only for explicitly dashboard-enabled sessions.
No default source execution, model call, external service or install is added.

Absent/null management preserves the preceding read-only configuration bytes,
fingerprints and catalogs. Explicit management binds a distinct host fingerprint.
The [2026-10-24 scoped evidence](../../docs/validation/integration-2026-10-24/dashboard-management/README.md)
records 677 native passes, real CLI transports and an actual native browser
workflow. Separately labeled exact-HTML fixtures provide inspected desktop/mobile
pixels; this is not full accessibility or all-browser certification.

This independent native view and management slice address part of the public dashboard surface;
the [public guide](https://oraios.github.io/serena/02-usage/060_dashboard.html)
also describes a broader management surface than this scoped implementation.
It is not a parity or cross-platform runtime certificate.

The later [selected-document/language acceptance](../../docs/validation/integration-2026-10-24/dashboard-documents/README.md)
records 733 scoped native passes and current CLI stdio/HTTP/native-browser flows.
Nine byte-identical integration groups are reused once after a controller cwd
failure before library execution. Native snapshot capture timed out: pixel
artifacts distinguish synthetic fixtures and static captured-native-DOM rerenders.
At that checkpoint file actions used the fixed `other_operation` fallback.
The later [private raw-log acceptance](../../docs/validation/integration-2026-10-24/dashboard-raw-logs/README.md)
records 747 native passes with no preceding-source group reuse, current CLI live
flows and actual native browser DOM checks. Fixed allowlisted document/language/
log labels retain status/timing while arguments/results remain excluded. Native
snapshot failures and the captured-DOM pixel rerenders are explicitly labeled.
Full dashboard/platform certification remains open.

The later [owning-shutdown acceptance](../../docs/validation/integration-2026-10-24/dashboard-owned-shutdown/README.md)
records 752 native passes on 1143 exact inputs, without group reuse, real stdio
exit before stdin EOF and HTTP Session/service isolation. Actual native browser
checks retain unsaved drafts and distinguish acknowledgement from proven process
exit; pixel evidence remains synthetic or a captured-DOM rerender.

The later [project-file acceptance](../../docs/validation/integration-2026-10-24/dashboard-project-files/README.md)
records 760 native passes on 1145 exact inputs, no group reuse and real original
host approval/executed verification through stdio, HTTP and native browser saves.
That checkpoint retained host-only workspace recovery. The later
[project-journal acceptance](../../docs/validation/integration-2026-10-24/dashboard-project-recovery/README.md)
adds bounded exact-selection inspection and confirmed conditional rollback,
with 788 scoped native passes on 1149 exact inputs and no group reuse. Other
native/older unbound journals use the host workflow. Drafts and IDs remain retained.
