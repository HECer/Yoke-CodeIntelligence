# Explicit native federation transport

`federation_http::serve` exposes existing signed source federation through one
host-selected HTTP/1.1 listener. It creates no listener by default. The host
provides a literal socket address, fixed import/export grants, principals, and
optional TLS material in memory. A nonloopback address requires both TLS and
the separate `allow_remote_bind` permission. Source files, repository JSON,
MCP requests, and signed source envelopes cannot supply these permissions.

TLS uses rustls with TLS 1.3, a host certificate/key, and HTTP/1.1 ALPN.
`TlsMaterial` owns a zeroizing private-key buffer; export seeds and client
tokens also use zeroizing buffers. Private material and principals have no
serialization or debug representation. The native CLI loads secrets from
its OS credential provider using public credential names. Its service and
client profiles contain public configuration/certificates and credential
references, never private keys or bearer tokens. The library accepts RAM
material across platforms; availability of the CLI OS credential provider is
a separate platform capability.

`Principal::new` accepts exactly 32 token bytes and hashes them before storing
the principal. The HTTP wire uses `Authorization: Bearer ` followed by exactly
64 lowercase hexadecimal digits. Authentication compares token hashes with
constant-time equality. IDs and grant references are bounded and restricted
to the fixed host grant registry. Roles are `read`, `import`, and `admin`:

| Method and path | Required authority |
| --- | --- |
| `GET /v1/status` | A current principal |
| `GET /v1/sources` | Read role; only that principal's applicable import scopes appear |
| `GET /v1/read/{grant}/{remoteId}?path={encodedPath}` | Read role and the exact import grant |
| `GET /v1/graph/{grant}/{remoteId}` | Read role and the exact import grant |
| `GET /v1/export/{grant}/{localSnapshotId}` | Read role and the exact export grant |
| `POST /v1/import/{grant}` | Import role and the exact import grant; existing signed envelope validation |
| `POST /v1/admin/revoke` | Admin role; exact JSON `{ "principalId": "id" }` |

Remote admin can revoke a principal. It cannot add grants, keys, roles,
execution authority, roots, or source policy. The host handle can revoke or
replace principals in memory, but replacements still reference the startup
grant registry. Revocation is checked before reading an upload and again when
dispatching it. An operation already authorized under the policy lock may
finish before a waiting host revocation returns; later operations cannot use
the revoked token. Revocations are in-memory configuration changes, so the
host must update its public configuration to carry them across a restart.

Imported snapshots retain the existing separate federated namespace,
Ed25519 signature, receiving source policy, workspace/scope/key binding, and
replay rules. Import never updates the original source tree or local latest
snapshot. Export produces the existing source-only signed envelope. A source
listing validates applicable retained receipts against the current fixed
grant; invalid applicable data fails closed instead of disappearing from a
successful empty listing. Other tenants' receipts are not exposed. Listing
does not establish the peer's current state or completeness.

HTTP parsing allows one GET or POST per connection, with at most 32 headers
and 16,384 header bytes. It requires HTTP/1.1 origin-form targets, an exact
Host authority (bound address or an explicitly allowed TLS name plus port),
and bounded decimal Content-Length. It rejects all Origin headers, duplicate
headers, Transfer-Encoding, upgrades, Expect, ambiguous framing, fragments,
and buffered pipelining. Read requests accept exactly one UTF-8 `path` query
with strict percent encoding. POST requires `application/json`. Host, token,
role, and grant denials happen before reading a declared body. Every request
is authenticated again at dispatch. Error responses never assert completion.
After a denial response, the owned socket can drain at most 64 KiB of raw
incoming bytes for at most 50 ms to avoid discarding the response through an
OS reset. It performs no body parsing or source/state operation. A larger or
continuing denied upload can still end in an explicit transport failure;
neither the client nor service infers a successful operation from a reset.

Body and response limits are host-selected from 1 through 32,000,000 bytes.
The total request and response, including framing, is capped at 64,000,000
bytes. Request I/O has a host-selected 100–30,000 ms deadline. Each core
source/state operation has an additional deadline of at most **4,000 ms**,
including SQLite contention; increasing the I/O budget does not increase this
operation budget. One worker bounds concurrent native operations. Shutdown
sets its stop flag, closes its active socket, wakes its own listener, and
waits up to five seconds for the worker. Idle TLS/header/body I/O checks
cancellation with socket timeouts capped by the remaining deadline and 50 ms.
Both server and client enforce deadline/stop checks before and after every
physical read, write, vectored write, and flush inside Rustls's stream adapter.
Partial TLS records or handshake bytes arriving faster than the inactivity
timeout cannot extend the shared request deadline. Successful buffered
plaintext operations and final client JSON parsing also recheck the deadline.
A failure to join is reported as an
error and is never reported as successful cleanup. Drop performs the same
bounded cleanup and emits a fixed diagnostic if cleanup cannot be proven.
This does not terminate unrelated listeners or processes.

`federation_http::request` supplies the public client API. `ClientConfig`
selects a literal socket address, expected TLS server name, public CA DER
certificates, RAM token, budgets, and the explicit plaintext-loopback option.
TLS verifies both the configured CA and server name; its Host header uses
that verified name and the explicit endpoint port. There is no DNS endpoint
resolution, redirect following, subprocess, credential argument, or
source-selected destination. Empty CA configuration permits plaintext only
with the host's explicit option and a loopback address. Responses require
bounded JSON and exact Content-Length, reject ambiguous/truncated/trailing
data, and require a complete close boundary within the shared deadline.
An HTTP non-success status stays explicit in `ClientResponse` and cannot be
interpreted as a successful source receipt.

The CLI commands are `federation-service` and `federation-request`; they require
the separate host trust flags documented in `workflows.md`. Public profiles
reference OS credential names rather than accepting plaintext secrets. A
TLS certificate can be distributed as public PEM/DER, without its key.

The tests use ephemeral RAM certificates and temporary source/state fixtures.
They exercise actual localhost TLS through the public client for signed
import/read/export, tenant filtering, role/token/grant denial, revocation,
CA/name validation, and source preservation. Raw transport fixtures exercise
Host/Origin/framing rejection, truncation, response bounds, and deadlines.
Shutdown tests cover an idle TLS handshake and a blocked state writer. TLS
drip tests cover a peer's partial handshake record against the public client,
and an actual completed handshake followed by partial encrypted application
records against the server; an authorized later client remains available.
The client regression reproduced a 100 ms budget taking 1.21 seconds before
physical I/O enforcement. The server availability fixture already passed on
the tested Windows/Rustls build; it is not claimed as a reproduced server
failure. The
canonical-name client fixture represents a listener whose accepted TLS name
differs from its literal endpoint without opening a public wildcard listener.
The Windows CLI OS-credential → TLS service → verified client live gate is
maintained separately in `tests/workflow_cli.rs`.

This release provides fixed host-managed roles and source sharing, rather than
an OAuth identity service, full-state replication, public hosting deployment,
distributed availability service, or security certification. Certificate
rotation and restarting persistent public host policy remain host operations.
The RAM library cannot protect secrets from an equally privileged debugger,
host compromise, or OS/process dumps. Kernel/filesystem stalls outside normal
deadline-aware operations can prevent cleanup; any such failure requires host
recovery and cannot yield successful shutdown proof. No public deployment,
remote multi-host reliability, or cross-platform OS-keystore certification is
claimed by the localhost transport gates.
