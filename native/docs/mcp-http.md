# Native MCP Streamable HTTP

The Rust host API `mcp_http::serve` starts an explicitly configured listener.
Construct each `Principal` from a unique host identifier, a zeroizing 32-byte
service credential and a `Target`. The bearer wire value is the credential's
64-character lowercase hexadecimal encoding. Load credentials through the
existing Windows service credential flow; do not put tokens or private keys in
repository configuration, command arguments, logs or MCP tool arguments.

`Target` fixes the canonical project root, protocol profile, `HostProfile`,
capture options and registered language servers before listening. Write access
defaults to false and additionally requires an editing host mode. Changing a
principal through `Control::upsert`, or revoking it through `Control::revoke`,
invalidates its existing sessions. Queued dispatch rechecks current credentials
and generation. No MCP method configures these authorities.

Session targets may additionally provide `collection_grants`, a host-only map
from registered collection names to current import-grant maps. The CLI spelling
is `collectionGrants` inside each trusted service principal. Collections still
require a registered receiver project and their independent registration
permission. The actor borrows this map under the current policy read lock for
one call; grants never enter connection state or queued jobs. Replacing a
principal with a reduced grant map invalidates its old sessions. Clients cannot
send a grant or select a filesystem receiver root. Other protocol profiles
cannot acquire collection authority through this setting.

The default address is an ephemeral IPv4 loopback port, endpoint `/mcp`.
Non-loopback binding requires both explicit permission and host-provided TLS.
TLS accepts TLS 1.3 and HTTP/1.1, with explicitly selected server names. Every
request requires an exact permitted Host authority. An Origin header is allowed
only when it exactly matches a configured origin; no origins are allowed by
default. Browser OPTIONS exposes only fixed transport metadata, without source
access. Permitted origins receive scoped CORS headers on authentication errors.

The implementation follows the pinned MCP 2025-11-25 Streamable HTTP transport:
one bounded JSON-RPC request or notification per POST, with both JSON and event
stream types in Accept. Requests receive JSON responses; accepted notifications
receive empty 202 responses. GET returns 405: unsolicited SSE streams, event
replay and resumable streams are not implemented. DELETE closes a session.
Initialization returns a cryptographically random session identifier; subsequent
messages bind it to the same principal, immutable host generation and negotiated
protocol version. Supported versions are 2025-11-25, 2025-06-18 and 2025-03-26.
Client responses are refused because this transport sends no server requests.

Headers are limited to 16 KiB and 48 fields, bodies to at most 4 MB and serialized
responses to 16 MB. Duplicate headers/JSON keys, transfer encoding, pipelined
bytes, ambiguous framing and unsupported protocol versions are refused. Host
limits bound connections, fixed actor workers, total/per-principal sessions,
absolute session lifetime and requests per session. Each session stays on one
actor because its process resources cannot safely cross threads.

After sending a framing/admission refusal, the response side is half-closed.
Shutdown may discard up to 16 KiB of remaining TCP bytes for at most 50 ms,
within the original physical deadline and host cancellation. Discarded bytes
are never parsed or dispatched. This bounded tail cleanup avoids resetting an
already framed refusal merely because an oversized header left unread bytes;
it does not promise delivery for an unbounded sending peer.

The host request budget is 250–30000 ms. The final 100 ms is reserved inside
that budget for an expiry response. Nested `Deadline::new` calls inherit the
host ceiling and cancellation on the dispatch thread; passed clones retain
them across explicit worker boundaries. Unrelated threads do not inherit TLS
context. Cancellation is cooperative, not hard CPU preemption. Shutdown closes
network sockets and cancels active work, waits at most five seconds for cleanup,
and reports failure if cleanup cannot be proven. A non-cooperative native call
can outlive cancellation and requires host recovery; no hard-kill guarantee is
made.

Native, Session, Serena, Graphify, Trail and Graft profiles use the shared
initialized protocol dispatcher. For Serena only, the host can set
`Target.agent_repl: Some(ReplSpec)`. This type has no deserializer and tool
arguments cannot attach it. Preflight validates the explicit executable, Python
trust and output/shell settings without starting a process. Each successful
initialization then creates its own persistent worker on its session actor.
The selected profile and presence of runtime authority cannot change on an
existing protocol connection. Public `serena_repl` arguments must use the
host-owned session label returned by `get_current_config`, which differs from
the transport session secret.

The optional Python runtime is ordinary explicitly trusted host execution,
not an OS sandbox: direct Python host effects are unmonitored. Its facade still
uses the session's registered source and edit policies. The CLI requires
`--trust-python` independently of service trust; an optional shell facade also
requires `--trust-shell`. Credentials remain references, never inline tokens.
Workers are separate for every SID and principal generation. Cancellation or
loss of a runtime invalidates its SID rather than silently recreating its
namespace. Ordinary Python cell errors can be corrected in the same still-live
runtime.

DELETE, expiry, revocation, rotation and shutdown request checked worker cleanup.
`ManagedChild::terminate_checked` requests owned wrapper termination and polls
for process exit within a separate host cleanup budget; other process callers
retain their existing behavior. AgentRepl additionally retains and checks its
three pipe-worker joins. Failure to prove cleanup is surfaced by server shutdown
instead of reporting successful cleanup. Windows gates verify actual owned
Python PID exit, including a child-process descendant under the Windows job.
These checks do not turn Python into a sandbox or guarantee hard preemption of
arbitrary native calls. Windows TCP and TLS are the tested runtime scope; Linux
and macOS remain deferred.
