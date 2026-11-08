# Native loopback editor transport

`editor_http::serve` binds exclusively to IPv4 `127.0.0.1`. Its public `Config` contains `port`, `request_timeout_ms`, and optional `max_requests` for a bounded listener lifetime. Port zero selects an ephemeral port. Timeout values are 10 through 600,000 milliseconds.

Startup writes exactly one newline-terminated JSON handshake containing `endpoint` and a 64-character hexadecimal bearer token. The token comes from 256 bits of operating-system randomness, remains in process memory, and is not logged or included in transport errors. The host/editor client must protect that startup channel. Restarting the service generates a new token.

Requests use `POST /editor HTTP/1.1`, `Content-Type: application/json`, an exact `Content-Length`, and `Authorization: Bearer TOKEN`. `Host` must equal the advertised `127.0.0.1:PORT`. An optional `Origin` must equal its exact `http://127.0.0.1:PORT` origin. Authorization comparison uses `subtle`'s constant-time byte equality. Unauthenticated requests are rejected before body reads or allocation.

| Operation | Fields | Result |
|---|---|---|
| `open` | None | New overlay session |
| `buffer` | `sessionId`, `buffer: {path, version, text, expectedVersion?}` | Versioned source-buffer metadata |
| `closeFile` | `sessionId`, `path`, `version?` | Close one buffer, with an optional version precondition |
| `close` | `sessionId` | Close the session |
| `status` | `sessionId` | Current session metadata |
| `capture` | `sessionId` | Immutable overlay snapshot |
| `tool` | `sessionId`, `tool`, `arguments?` | Native tool envelope bound to a newly captured overlay |

Successful operations return `{ "ok": true, "data": ... }`. Transport/operation failures return `{ "ok": false, "error": { "code": ..., "message": ... } }`. Invalid requests use HTTP 400; missing authentication 401; invalid host/origin or unavailable capabilities 403; stale editor versions/disk conflicts 409; elapsed deadlines 408. Malformed JSON and duplicate typed request fields are rejected. Unknown operation fields cannot grant authority.

Only `ci_status`, `ci_find`, `ci_read`, `ci_trace`, and `ci_context` are exposed. Their arguments pass the ordinary strict native schema before capture. The session's overlay snapshot replaces any supplied snapshot ID; its remaining deadline caps the tool deadline. `ci_status` with `mode: index` is refused: use `capture` for overlay indexing. Native tool envelopes retain their own error/partial status and response budget.

The transport forces `Engine.allow_writes` false while serving, even if its library caller had enabled writes. It restores the caller's setting on ordinary service completion. It cannot invoke change, verification, approval, command, provider-registration or arbitrary host operations. Buffer text enters immutable local storage and never writes original source. Existing overlay policies still reject protected/ignored paths, obvious secrets, NULs, oversized buffers, stale versions and independent disk conflicts.

HTTP handling is deliberately serial. It rejects transfer encoding, `Expect`, duplicate headers, malformed CRLF framing, excessive headers and a second already-buffered request. Every response declares `Connection: close`; later requests on that socket are never dispatched. There is no chunked transfer, keepalive or pipelined processing.

The limits are 16 KiB of request headers, 64 parsed headers, 24 MiB of JSON body, and 32 MiB of serialized response. The body limit accommodates a valid 4,000,000-byte source buffer even when every byte expands to six JSON escape characters. The existing 4,000,000-byte source limit remains separate. Response serialization stops at its limit rather than building an unlimited response first.

One absolute deadline starts when a connection is accepted and covers headers, body, operation, serialization and writes. Partial reads/writes use the remaining duration; progress cannot reset the timer. Before response headers are sent, a timed-out client gets a small best-effort nonblocking 408 response. If a response transfer has already started, the socket closes with an incomplete body that the client must reject. Neither case extends the budget. Static analysis and SQLite still use cooperative shared deadlines; this is not an OS CPU scheduler quota. A serial service can delay queued clients up to the active connection's configured lifetime.

On Windows, closing rejected input with unread bytes can reset the socket before an error response reaches the client. After responding and shutting down writes, the service discards at most 64 KiB of already-available bytes without waiting or interpreting them. Oversized/malicious peers are not guaranteed a complete error response. This disposal never changes authorization or dispatches a request.

Validation in `native/tests/editor_http.rs` uses actual loopback sockets, complete HTTP request/response framing and real overlay/index storage. It covers source-preserving unsaved reads, authority rejection, pre-body authentication/origin/host checks, stale buffer/close conflicts, duplicate lengths/authorization, transfer encoding, invalid JSON, pipelining, header bounds, unknown fields, slow header/body progress, a maximally escaped valid buffer and continued service availability for a nonreading client. Module tests additionally verify that expired responses cannot claim success and that writes stop at their absolute deadline after a real bounded TCP queue reports `WouldBlock`. The latter observes actual backpressure instead of assuming a particular loopback buffer size. These checks are separate from running the extension inside a real VS Code host.
