# Host-configured semantic navigation

Native `ci_find` supports six read-only LSP operations through an explicitly
registered host provider: `definition`, `declaration`, `references`,
`implementation`, `hover`, and document `symbols`. This extends the existing
eight-tool interface; it does not add a ninth tool or execute a server by default.

The trusted operator registers an `lsp::Server` through `Engine::register_server`
or starts the existing CLI with a reviewed server configuration:

```powershell
yoke --root C:/project --language-server-config C:/host/servers.json --trust-servers serve
```

For example, the host configuration can contain:

```json
[{"language":"rust","command":"rust-analyzer","args":[]}]
```

The registration is host authority. An MCP caller, repository file, source comment
or imported record cannot select a command, supply arguments, register a provider,
or grant trust. A missing configured provider returns an explicit error. The
captured file's language determines provider selection. Exact registrations take
priority; TSX may use a registered TypeScript provider and JSX may use a registered
JavaScript provider. The LSP client sends the appropriate per-document language
IDs and opens captured siblings from the same provider family.

An MCP `tools/call` uses the existing `ci_find` name and this argument shape:

```json
{
  "snapshotId":"<64-character immutable snapshot ID>",
  "languageServer":{"operation":"references","path":"src/lib.rs","offset":137},
  "timeoutMs":15000,
  "maxBytes":32000
}
```

All three `languageServer` fields are required. `offset` is a UTF-8 byte offset
within the captured file and must be on a character boundary. Use zero for a
document-symbol query. Paths must be canonical relative captured-source paths.
The request rejects additional nested fields, mutating operations, and mixtures
with ordinary `query`, `kind`, `model`, `vector`, `path`, `regex`, `limit`, or search
`offset` fields. Common snapshot, byte/token budget, timeout and task fields remain
available. Ordinary search requires `query`; the existing explicit semantic-vector
branch can instead supply its indexed `model`, `kind: "semantic"`, and `vector`.
The advertised MCP schema expresses these alternatives and keeps unknown fields
forbidden.

An explicit `snapshotId` selects those exact immutable bytes even after saved
files change. Without an ID, the engine captures saved source first. The LSP client
materializes the selected snapshot in a temporary directory, shares the request
deadline, validates source-preserving completion, and never writes original source.
Definition/declaration/reference/implementation locations retain checked source
hashes and exact UTF-8 ranges; large bodies use bounded excerpts or immutable
`ci_read` instructions. Hover and document-symbol responses remain provider data.
The returned request metadata includes the operation, path, offset, source hash
and snapshot. No language-server result is promoted into a fabricated native graph
or an exhaustive coverage assertion: `complete` remains false.

Response metrics describe this invocation:

| Metric | Meaning |
|---|---|
| `languageServerAttempts` | Zero for validation, source/precondition or missing-provider denial; one when the host-authorized LSP entrypoint is attempted. It is not a count of protocol messages or a claim that process startup succeeded. Existing semantic rename uses the same counter. |
| `localOnly` | True for local retrieval or denial before dispatch; false for an authorized provider attempt. |
| `externalUsage` | `none` before external dispatch; `unknown` after an attempt because provider billing and network activity cannot be established by this interface. |
| `modelCalls` | Zero: this interface does not invoke a model. |

Failed attempts retain these metrics and return an error, without a structural
fallback masquerading as semantic completion. `ci_find` and semantic change
preparation advertise `openWorldHint: true` because a trusted provider may access
external systems; this hint does not grant permission. A copied snapshot is not
an OS sandbox. The provider runs with host privileges, so operators must review
the executable and use appropriate outer isolation for untrusted programs.

`native/tests/semantic_mcp.rs` exercises actual MCP calls and framed Node stdio
processes labeled as protocol fixtures. Tests cover all six reads, requested old
snapshots after disk changes, Unicode byte/UTF-16 positions, source preservation,
provider denial, strict authority/mutation/mixed-input rejection, TSX routing and
existing explicit vector retrieval. These fixtures validate the interface and
source binding; they do not prove compiler correctness. Actual rust-analyzer and
TypeScript server gates are separate evidence, and other servers remain unverified.
The actual rust-analyzer MCP gate in `native/tests/live.rs` checks reference
locations and exact source hashes against an old requested snapshot after the
original saved file changes. Stored snapshot, workspace and graph-member identity
mismatches fail before a query can attribute those bytes to another snapshot.
