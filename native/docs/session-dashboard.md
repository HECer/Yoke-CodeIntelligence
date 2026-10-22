# Native session dashboard

The native dashboard is an explicitly enabled, read-only session view. The host
selects `"dashboard": {}` in its JSON HostProfile, then calls `open_dashboard`,
`cfg.open_dashboard()` or native session operation `openDashboard`. Existing
tool/API exclusions and optional-tool selection remain independent. Project
YAML, repository text, request arguments and another project query cannot grant
dashboard authority. No listener or browser starts for absent/null configuration.

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
operation text is replaced by a fixed label. This is not a raw log viewer or a
count of every public protocol request. Evicted activity rows are disclosed.

Configuration/memory editing, raw log access, tray integration, automatic browser
opening and MCP shutdown are unavailable in this scope. Their capabilities are
reported false rather than as empty successful results. Source and host settings
cannot be changed through HTTP. Existing guarded edit/approval/verification and
recovery workflows retain their independent authority.

Only the static bootstrap is public. Session data requires exact Host, allowed
Origin and constant-time Bearer capability validation. HTTP framing, headers,
routes, response sizes and request lifetimes are bounded; mutations and request
pipelining refuse. Source/session text is rendered as inert text. No external
assets or network destinations are used. Each data request opens existing-only
read-only state and checks the exact workspace/session row against the view.
Concurrent changes refuse with 409 until the current host session supplies a
fresh view. Neither stored data nor the browser can select another root.

Host bounds (camelCase) are:

| Field | Default | Allowed |
| --- | --- | --- |
| `port` | `0` (OS-selected) | `0`–`65535`, IPv4 loopback only |
| `requestTimeoutMs` | `1000` | `10`–`5000` |
| `lifetimeMs` | `3600000` | `100`–`86400000` |
| `maxRequests` | `10000` | `1`–`100000` |
| `activityCapacity` | `200` | `1`–`1000` |

The listener starts only on the explicit open operation. Repeated opens reuse the
live instance; an expired instance is replaced with a fresh capability. Dropping
the owning session stops and joins its worker. A pending request can delay cleanup
only until its bounded request deadline. It never terminates a client/browser or
an unrelated process. No activity or capability is persisted to the application
database. SQLite may coordinate live WAL reads through its normal sidecars.
MCP-HTTP dashboards additionally share the transport session's revocation/expiry
signal, checked before data admission and response. Revoking that access closes
the listener independently of later actor/process cleanup.

Explicit configuration binds the host session fingerprint and stale admission.
Absent/null configuration preserves prior serialization, session fingerprints,
configuration replies and default tool-catalog bytes. The retained default
catalog description is upgraded only for explicitly dashboard-enabled sessions.
No default source execution, model call, external service or install is added.

This independent native view addresses part of the public dashboard surface;
the [public guide](https://oraios.github.io/serena/02-usage/060_dashboard.html)
also describes broader management and raw-log features not implemented here.
It is not a parity or cross-platform runtime certificate.
