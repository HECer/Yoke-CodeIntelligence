# Native project sessions and source retrieval

The Windows-first parity implementation provides persistent project steering,
authored application notes, source queries, guarded edit plans, and an optional
trusted Python REPL. This document describes implemented interfaces; it does not
certify complete upstream parity or every language-server installation.

`yoke session --id work --host-profile host.json --requests requests.json`
executes a bounded sequence of session operations. The sequence is not atomic.
Each mutation has its own transaction and revision guards. A subsequent command
with the same ID and host profile resumes project selection and modes.

Example host profile:

```json
{
  "projects": [
    {"name": "library", "root": "C:/work/library", "readOnly": true},
    {"name": "reference", "root": "C:/work/reference", "queryOnly": true}
  ],
  "context": "agent",
  "modes": ["interactive", "editing"],
  "notesPolicy": {"writable": true, "readOnlyPatterns": ["policy/**"]}
}
```

Project roots and an optional `globalNotesRoot` must be existing absolute host
directories. Their canonical identities are bound to the session. Registered
projects inherit the host's capture options and registered semantic providers.
Query-only projects cannot be activated or edited. Source writing remains a
separate host permission; modes and remote tool arguments cannot grant it.
Global application notes use a separately selected host store. These notes are
distinct from the coding agent's central AgentMemory service.

For root-based read queries, a registered project's exact host-selected spelling
also selects its canonical root. This includes a host-selected Windows short
path. These aliases belong to the current invocation: session records do not
grant them, and reopening with only the canonical root drops old spellings.
Lookup does not resolve arbitrary caller paths against the filesystem. Removing
the registered project also disables its alias. Windows comparisons fold ASCII
case only; distinct opaque Unicode spellings are not merged by Unicode lowercase
expansion. Named project selectors remain available.
POSIX selectors retain literal backslashes, prefix text and case. Windows path
syntax does not make a different POSIX spelling into a registered selector.

Example requests:

```json
[
  {"operation": "activate", "args": {"project": "library"}},
  {"operation": "fs.list", "args": {"relativePath": "."}},
  {"operation": "notes.write", "args": {"name": "design/choice", "content": "Use immutable source snapshots."}},
  {"operation": "configuration", "args": {}}
]
```

`yoke serve-session --id work --host-profile host.json` exposes session steering
through `ci_session` over the existing bounded newline JSON-RPC stdio transport.
It also exposes read-only native tools, `ci_retrieve`, and `ci_graph`. Direct source prepare
and apply tools are omitted from this profile; guarded preparation goes through
the session's mode and project checks. Native `yoke serve` retains its eight-tool
contract. Errors from nested native queries remain MCP tool errors.

`yoke serve-session --profile serena --id work --host-profile host.json`
offers the 53 pinned public Serena tool names with independently generated
strict argument schemas. This inventory is not a statement of complete behavior
parity: unavailable operations fail explicitly. Source changes remain guarded
plans, and responses use native JSON rather than upstream formatted text.
`activate_project` and `serena_repl` accept only the host-owned session ID.
The independent schema generator reads public signature facts from the pinned
mapping; no upstream application implementation is embedded.

Adding `--python C:/Python/python.exe --trust-python` authorizes one persistent
Python worker for the stdio server. `serena_repl` cells retain their namespace;
declaration regex adapters use that worker, and an optional explicit
`--shell-command` selects the shell adapter. Without this configuration, Python
and shell operations fail. Ordinary MCP arguments cannot select executables,
trusted roots, or another session. Python cell errors are MCP tool errors;
ordinary cell exceptions retain the namespace while fatal output/deadline
failures discard the worker. The arbitrary-host-execution boundary below also
applies to this profile.

Capture configuration can explicitly include `readOnlyDependencyDirs` containing
`node_modules` and/or `target`. Immutable captured membership and original path
components authorize reads; source writes still reject those protected paths.
Git ignores, secret/state protections, links, binary content, and capture budgets
remain enforced. An ignored dependency is reported as omitted, not silently
treated as captured. Analyzer epoch 7 distinguishes the expanded parser and
capture semantics; authenticated historical epoch 4/6 snapshots keep their
recorded languages and bytes.

`yoke retrieve "payment processing" --options retrieval.json` combines
source-body BM25 with personalized graph ranking. Optional `graphWeight` is in
`[0,1]`; zero selects lexical ranking. `perFileLimit` diversifies results, and
`includeBody` returns a bounded excerpt with exact UTF-8 byte/hash bindings.
The ranking treats captured observations as bidirectional relevance links and
preserves parallel arcs. Scores express heuristic relevance, not binding proof.
Source and processed-document budgets, omitted scopes, and body clipping are
reported explicitly. `ci_retrieve` supports an immutable `snapshotId`.

`yoke freshness` compares disk capture against the last completed disk index.
It reports content/language/mode changes, additions, deletions, and omission
changes without publishing a graph or moving the latest snapshot. Prepared
branches do not redefine that baseline. A changed host capture scope requires
an explicit new index. An unindexed workspace is reported as unindexed.

`yoke agent-repl --python C:/Python/python.exe --trust-python --cells cells.json`
runs a JSON array of Python cells in one persistent namespace. The facade is
available as `s`; functions, classes, variables, source queries, notes, and
guarded plans can compose across cells. A separately selected `--shell-command`
and repeated `--shell-argument` enable the shell convenience adapter. Output and
execution share bounded deadlines; fatal timeout/output failures invalidate the
owned worker rather than silently reusing a damaged namespace.

Trusted Python is arbitrary host execution. The worker is not an OS sandbox;
its imports can have host filesystem and network effects. Facade write guards
do not limit arbitrary Python code. The CLI reports this execution boundary.
No Python or shell execution authority is accepted through ordinary session
tool arguments. Supported method names and proven method behaviors are tracked
separately in the parity evidence.

The native session operation `symbols.fileApi` accepts `file`, optional `limit`
(1–200), and `offset`. Exact paths take precedence; basename fallback requires a
unique captured file. Supported declaration headers preserve multiline source
bytes and exclude AST bodies. Unsupported headers are explicitly omitted rather
than copied from body text. Each row carries its original source hash and byte
range; pagination includes the total and next offset.

`yoke serve-session --id local --profile trail` (or `graft`) exposes the six
pinned query tools and accepts the legacy Graft aliases. `find_code(full=true)`
returns complete selected declaration bodies with source hashes and exact byte
ranges; the default returns at most eight lines around a query match. Selected
source and body output each have a 32 MiB budget. `find_all` returns up to
100000 occurrences within a 32 MiB row budget, with exact totals when the scan
finishes and explicit lower bounds/truncation otherwise. Dependency traces omit
containment edges and expand captured file selectors into their declarations.

Trail results exceeding the inline response budget are published as immutable
JSON artifacts, with their ID, byte count, item count, and resource URI. All
stdio profiles support paged reads using
`ci://artifact/<sha256>?cursor=0&maxBytes=32000&encoding=utf8`.
`encoding=base64` supports binary data; `maxBytes` is 1024–32000 and bounds the
serialized page JSON. Follow `nextCursor` until null. Each page verifies the
entire artifact hash, including bytes outside the requested chunk. Session
profiles may select an existing host-registered project using `project`; a URI
cannot register a project or grant source access.

The public Git workflow CLI is `yoke git analyze --trust-git`, with optional
`--git <absolute-host-executable>` and `--options <host-json>`. It reads verified
committed objects without changing the working tree or index baseline. Existing
reports can be read with `yoke git load <id>`, rendered with
`yoke git render <id> --format markdown` (also JSON, text, Mermaid, or HTML), and
queried using `yoke git digest <id> report|history|file --path <captured-path>`.
`yoke git seed --trust-git --parent-root <absolute-root> --parent-workspace <id>`
copies only revalidated compatible parser entries from the same Git worktree
family. Neither Git execution nor parent workspace access is granted by a tool
argument or repository file. See `git-workflows.md` for proof boundaries and
remaining workflow gaps.
# GitHub credentials and bounded resources

`yoke credentials github-import <name>` accepts a GitHub token only through a
private stdin pipe. It refuses an interactive terminal and never accepts the token
as a command-line argument. A single trailing LF or CRLF is permitted. The token
is stored create-only in Windows Credential Manager under
`yoke-code-intelligence/service/github/<name>`; output contains metadata only.
The host's GitHub `credentialRef` selects this name, not an arbitrary credential
target. Importing a credential does not enable network access or authorize any
repository. Those remain separate host settings.

Graphify resources up to 32,000 UTF-8 bytes retain their original inline MIME type.
Larger resources within the adapter's 256,000-byte capacity return a JSON descriptor
with `artifactId`, `resourceUri`, `totalBytes`, `originalMimeType`, and
`originalResourceUri`. Read `resourceUri` through the existing `resources/read`
pagination contract. Stored artifact bytes exactly preserve the original response;
the descriptor does not silently truncate it. Capacity limits still produce an
explicit error.

## Message-oriented protocol connections

Rust hosts may use `protocol::Connection` to deliver separate JSON-RPC objects
while retaining protocol initialization. Its constructors select a native engine
or an existing host-owned session with the session, Serena, Trail, Graft, or
Graphify surface. Messages cannot select a backend or grant host authority.
`message` admits one bounded JSON object and returns one response or no response
for an accepted notification. It rejects batches, concatenated objects, duplicate
keys, and oversized input. Stdio continues to use the same dispatcher and framing.
Version negotiation also accepts MCP `2025-11-25` while retaining older supported
versions and the existing fallback.

This API is transport preparation. An HTTP listener still needs authentication,
Origin validation, request/version/session handling, and project scope enforcement.
It does not itself claim Streamable HTTP certification. The pinned transport
contract is the [official MCP 2025-11-25 specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).

`serve-http --config <host.json> --trust-service` starts the explicit HTTP host.
Use `--duration-ms` for a bounded foreground run, or zero for an ongoing service.
The host file declares principals with `id`, `tokenCredentialName`, a profile and
optional absolute target root. Tokens and TLS keys are resolved from the existing
OS credential namespace; inline token/key fields are rejected. `tlsCredentialName`
selects an existing TLS identity. Non-loopback binding additionally requires
`--allow-remote-bind` and valid TLS. Configured language servers require the
existing global `--trust-servers`; source writes require global `--allow-writes`
and the target's explicit write setting. Request messages cannot grant these.

