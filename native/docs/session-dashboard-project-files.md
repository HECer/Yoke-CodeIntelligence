# Selected project files through native workspace gates

This route is separate from the selected global-file writer. A trusted host must
select an exact project document with `writable: true`, grant workspace writes
and enable editing for the active non-query, non-read-only project. Current
`ci_session`, `ci_change_prepare` and `ci_change_apply` policy still applies at
their respective operations. Source capture/exclusion policy remains unchanged.
An omitted or uncaptured configuration file is unavailable to this writer.
The selected-document backend remains Windows-only.

The private Files editor prepares a normal immutable native change plan affecting
exactly the selected existing file. The selected document's physical, security
and byte revision must match. Trusted physically checked roots derive its source
path; a request cannot supply a path, snapshot, command, write grant, approval or
verification record. The native capture must contain the exact before-bytes.
Native source line-ending policy can normalize the after-bytes; the returned
proposal contains those exact bytes for review.

Two IDs remain distinct: the dashboard's short-lived reviewed proposal and the
native workspace plan. The host reviews and approves the exact native plan with
the existing host approval command, and explicitly runs its independently
selected executed verification in the existing copied-tree workflow. Syntax-only
verification is insufficient. The browser's `inspect` request reads these records;
it cannot create approval or run verification. Checking status never saves.

After explicit confirmation, saving calls the original native workspace writer.
It rechecks host writes, exact plan approval, passed executed verification for the
result snapshot, source scope/line endings, plan expiry, full current base snapshot,
WriterLock, file preconditions and durable workspace journals. Changes to other
captured files therefore refuse even when the selected file is unchanged.
No project file is routed through global host-file approval or its journal.
Windows security metadata preservation is that of the existing workspace writer;
the stronger global host-file replacement contract is not claimed here.

Reviewed proposals remain bound to the original actor, project and Session SHA,
expire after ten minutes and share the sixteen-proposal capacity with global
documents. A refused preflight retains the proposal for checking new host
approval/verification. An operation that entered the writer but did not confirm
success is unconfirmed; its possible deterministic workspace journal ID and
native plan ID are returned without claiming that a journal was created.

Workspace journal inspection/recovery uses the original native host workflow.
The dashboard explicitly reports that it cannot list complete workspace recovery
state, resume or roll back those journals; it never returns an empty complete
recovery result. Global-file conditional recovery keeps its separate contract.
No source, journal or immutable snapshot is deleted by selecting this route.

Saving bytes does not reload configuration or automatically execute project code.
The dashboard never chooses a verification command or adds execution trust.
Only fixed action name, status and duration enter activity; no file body, command,
approval or result is logged there. Implementation, tests, actual transport and
browser qualification are separate from this contract.

The [scoped acceptance](../../docs/validation/integration-2026-10-24/dashboard-project-files/README.md)
records 760 native passes, all-target Clippy/368-root formatting/typecheck/npm,
three actual stdio variants, separate HTTP proposals/shared source bytes and
actual native browser confirmation/save with exact byte hashes. Host-authored
inert-byte assertions run through the real copied-tree verifier. Synthetic and
captured-native-DOM pixels are separately labeled from native screenshots.
Broader dashboard, platform, upstream and release gates remain open.
