# End only the owning MCP connection

The optional HostProfile setting `dashboard.management.shutdown: true` enables
an explicit request to end the owning MCP connection. It defaults false; absent
and false values preserve the previous serialized configuration. Null, objects
and unknown authority fields refuse. No project file or request can install a
stop target.

The trusted built-in managed stdio or HTTP Session actor must independently
install its owner signal before opening the dashboard. JSON alone is
insufficient. A library host using only the management mailbox has no stop
driver: its shutdown capability remains false and requests refuse explicitly.
Pure Engine HTTP profiles retain their existing management refusal.

The closed action is `{ "kind": "shutdown" }`, inside the existing envelope
with request ID, current project and Session-state SHA. No path, command,
process ID, Session ID, service selector or serialized approval is accepted.
The original Session owner rechecks root/state/project, current `ci_session`
policy, revocation and the original deadline. The host shutdown grant remains
independent of session controls, memory, documents, logs and source writes.

After acceptance the owner quiesces this connection. Queued/new operations for
it cannot execute, including already prefetched stdio frames. The dashboard
listener retains an opaque response-completion value across bounded encoding
and its response write. Only after the write attempt returns may the owner
close normally. Moving a Reply through the mailbox does not count as that
attempt. If delivery is abandoned after acceptance, dropping the completion
value also releases the owner; a disconnected client cannot keep an accepted
shutdown pending indefinitely. In that case the client has an unconfirmed
outcome. Client receipt of an acknowledgement is never guaranteed.

The response acknowledges the request, not completed process exit:
`shutdownAccepted` is true, while `processExitConfirmed` and
`connectionCloseConfirmed` are false. Its scope is installed by the driver:

| Driver | Scope | Effect |
| --- | --- | --- |
| Managed stdio | `owning_stdio_mcp_connection` | Leave this connection's loop, then perform the caller's normal checked runtime cleanup. |
| Session-backed HTTP | `owning_http_mcp_session` | Revoke this actor's transport Session and use its existing owned-resource cleanup. |

`serviceShutdown` is always false. The HTTP service and other Sessions keep
their own authority and runtime state. No global stop flag, arbitrary process
kill or immediate process exit is added. Existing cleanup-unproven handling
remains authoritative; acceptance does not promise successful resource cleanup.
Session records, named memories, source bytes, write approvals and recovery
journals are not deleted by this action.

The Configuration page shows the control only when the grant and owning driver
are present. An explicit confirmation explains loss of this connection's runtime
variables and that browser drafts are not saved. Cancellation submits nothing.
After an accepted or unconfirmed request the page stops polling and disables
management; retained draft text can still be copied. It reports closing or
unconfirmed state without claiming exit. Closing one connection does not close
the browser tab automatically.

Only the fixed `dashboard.shutdown` name, status and duration enter activity.
Arguments, results, private capabilities and runtime variables are not recorded.
Implementation, whole-suite tests, actual transport cleanup and browser visual
acceptance are separate checkpoints; none is implied by this contract alone.

The [scoped acceptance](../../docs/validation/integration-2026-10-24/dashboard-owned-shutdown/README.md)
records 752 native passes, all-target lint/format/typecheck/npm, three actual
stdio variants, HTTP isolation and captured runtime-exit handles before Job
cleanup. Actual native browser cancellation/confirmation and draft retention
are separate from explicitly labeled synthetic/captured-DOM pixel evidence.
Broader dashboard, platform, upstream and release gates remain open.
