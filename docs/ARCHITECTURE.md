# Architecture and design decisions

## Native Rust ownership

`native/src` is an independent runtime with its own `.yoke` SQLite/CAS state. `capture`, `paths` and `store` own strict saved-source acquisition, immutable bytes, FTS and client/writer locking. `analysis` and `contracts` produce structural and authored observations; `query` owns retrieval, plans and directed analytics. `lsp` owns the host-selected semantic protocol, with exact source anchors and bounded excerpts. `changes`, `verify` and `process` own reviewed plans, copied execution, conditional original writes and recovery.

`editor` derives versioned immutable overlays; `editor_http` exposes authenticated loopback read operations. The VS Code client selects executables from user settings and queues document versions through startup/shutdown. `knowledge` validates authored assertions, sparse/external vectors and imported observations. `media` binds assets and supplied text; PDF parsing uses a resource-limited child rather than uninterruptible parsing in the main process. `federation` and `maintenance` have separately scoped integration and acceptance tests.

TypeScript directories and ADRs below describe the retained advanced runtime. They are not an assertion that every operation has migrated to Rust. Snapshot identity and serialized state are runtime-specific; no implicit conversion occurs. [The native guide](../native/README.md) and [replacement criteria](RUST_REPLACEMENT.md) describe current boundaries.

## Boundaries

```text
CLI / MCP stdio / read-only local explorer / library
                        |
                 CodeIntelligence.invoke
          validation, scope, shared deadline, budget
                        |
    Snapshot/CAS -> native analyzers -> canonical evidence graph
                        |                         |
              exact-source query          optional adapters
                        |
            prepare -> verify -> approve -> apply
                        |
                 durable journal / recovery
```

The default pipeline performs no model calls and does not execute indexed source. External provider and verification processes are opt-in trusted actions, not extensions of source-file authority.

## Module map

| Directory | Responsibility |
|---|---|
| `src/core` | Paths, saved-file capture, content hashes, UTF-8 positions, deadlines, response budgets and spend ledger |
| `src/store` | SQLite WAL, FTS5, immutable blobs, graphs, artifacts, plans, journals, memories and vectors |
| `src/analysis` | TypeScript compiler, Python/Go workers, document and contract hints, queries, analytics and knowledge imports |
| `src/adapters` | Strict upstream graph contracts, LSP framing, Serena MCP and explicitly trusted provider bridges |
| `src/changes` | Patch preparation, permissions, conditional writes, verification and crash recovery |
| `src/protocol` | Eight-tool schema validation, stdio MCP and read-only authenticated loopback HTTP |
| `workers` | Independently written language/process bridges and static explorer assets |

## ADR 001 — Capability fusion, not application concatenation

The specification proposed Graft for structural orientation, Serena/SolidLSP for semantic operations and Graphify for broader relationships. This implementation retains that capability split but uses a native TypeScript compiler and standard-library AST workers by default. It does **not** launch three applications for every question. This makes a runnable local path possible without upstream installs, API keys or copying GPL application code. Optional adapters import additional evidence rather than overwriting all native facts.

Tradeoff: this is not complete upstream feature parity. SolidLSP is not bundled; the included LSP client is independently implemented. All-language semantics, the newer Serena REPL and Graphify's cloud/media features remain outside the shipped scope.

## ADR 002 — The graph is a snapshot-bound index, not source truth

Source bytes are content-addressed with SHA-256. A snapshot records paths, hashes, modes, omissions, workspace identity and analysis configuration. `Entity` IDs are snapshot-scoped; moving/renaming a symbol does not pretend to preserve a proven global identity.

Each directed `Observation` has a relation, resolution, producer and evidence IDs. `Evidence` records the source hash, byte and line ranges, and anchor granularity. Parallel relationships survive. An unresolved target remains null with its target label; a search-result sequence is never converted into a path.

Resolution categories: syntactic, exact binding, possible dispatch, heuristic, inferred concept, observed execution, authored contract and unresolved. These are not calibrated numeric confidence probabilities.

Graphify imports preserve `_src`/`_tgt` direction markers and raw arc order even when a persisted graph is marked undirected. Raw extraction is preferred; relations already removed by an upstream projection cannot be reconstructed. Graft's symbol-level edge evidence is recorded as an enclosing-symbol anchor, not a fabricated precise call site.

## ADR 003 — Explicit freshness

Capture enumerates saved files, reads/hash-checks contents and repeats file-set/stat checks. Stable deletions, creations, ordinary concurrent changes and ignored files are handled explicitly. A new graph is published only after successful capture and bounded analysis. Immutable snapshot queries remain valid after later disk changes.

This is a stable double scan, not an OS-atomic snapshot against a hostile writer. Unsaved buffers are unknown. Fresh captures remain O(source bytes); the in-memory/cached explicit-snapshot query path is faster. Parse caches are keyed by content/provider/version; project-level resolution is recomputed for each new graph, including unchanged consumers after dependency/export changes.

Workspaces have separate state and randomly initialized identities. Identical bytes can be reused within their CAS. This is not a federated multi-repository database or multi-machine synchronization system. SQLite WAL permits concurrent readers but serializes writers; do not put the live database on a network share.

## ADR 004 — Permission outside repository text

Repository JSON can tune limits and describe verification profiles. It cannot authorize profile execution, workspace writes or a custom interpreter. Those capabilities come from trusted host options. MCP schemas reject undeclared permission fields.

Approval records are local workflow gates, not a privilege boundary against an agent already able to run arbitrary host shell commands or edit the state database. Secure agent deployment still needs host/container permissions.

## ADR 005 — Reviewable changes, honest atomicity

Prepare validates expected hashes, byte boundaries and non-overlapping edits, produces a full unified diff and a derived immutable snapshot. Compiler renames are available for native TS/JS symbols. Isolated apply materializes a copied tree; it does not mutate the original workspace.

Workspace apply requires host permission, exact plan approval, successful bounded verification unless explicitly overridden, current base snapshot and a writer lease. Durable intent records and per-file before/after hashes support recovery. Rollback only restores bytes still matching this transaction's output, preserving concurrent foreign edits. The full multi-file update is not atomically visible to external readers. The copied tree is not an OS sandbox or a Git worktree.

## ADR 006 — Bounded retrieval and usage

Eight tools limit schema clutter. Read-only plans compose find/filter/references/read/select/group/limit without executing a generated program. One request deadline is shared; synchronous compiler calls are checked cooperatively and cannot always be preempted mid-call.

Result packing preserves whole records and evidence links. Complete overflow results are local immutable artifacts. Byte counts describe the structured envelope; token counts are explicitly estimated. No upstream savings percentage is repeated as a measured result.

The spend ledger reserves costs before calls, distinguishes sent from unsent attempts, keeps unknown settlement reserved, and prevents duplicate settlement. The native engine spends no model tokens. Full agent-harness usage requires the harness to report it and is not fabricated by the core.

## ADR 007 — Optional enrichment remains evidence-scoped

Memories distinguish confirmed findings, hypotheses, dead ends and superseded records. Current-file hashes determine staleness. Imported vectors carry model/dimension/hash identity; there is no built-in embedding model. LCOV is aggregate coverage, not a precise per-test map. Runtime imports prove only the supplied run's observations. SARIF findings are external analyzer data, not automatically security proofs.

Graph analytics include degree, weak components, import cycles and snapshot differences. Weak components are not Leiden communities. Framework hints are syntax-derived candidates and can be incomplete. There is no general taint analysis or formal correctness verifier.

## ADR 008 — Compilation context is part of a fact

`typescript-host.ts` owns an immutable filesystem and a bounded graph of configured projects. `TypeScriptProject` orchestrates one language service/program per configuration and merges source entities without flattening context-specific observations or diagnostics. Config roots and inherited relative paths are delegated to the pinned compiler. Source-reference navigation uses a tested compiler-host redirect, not generated build output. Same-source conflicting rename bindings fail closed.

No compiler host fallback reads arbitrary live disk files: only immutable captured source and trusted pinned standard-library files are readable. Files not covered by any configured program remain searchable but explicitly lack semantic coverage. The default trace can be a union of configurations; request `analysisContexts` to restrict it.

## ADR 009 — External protocol results are not automatic source authority

LSP parsing validates per-message limits, UTF-8 JSON, JSON-RPC result/error shape, negotiated UTF-16 positions, URI scope, LocationLinks, edit versions and overlaps. Unsupported callbacks receive MethodNotFound and workspace/applyEdit is refused. Read results are locally source-bound, not upstream correctness/coverage attestations.

The host-authorized `runLsp`/CLI operation materializes a snapshot, shares one deadline, invokes the provider, requests shutdown, verifies copied source hashes and returns bounded locations or a persisted change plan. Original source is never implicitly edited. Unknown external billing/network activity is reported; a copied tree is not an OS security boundary. No arbitrary repository-declared server command is auto-started.

## 0.3 extensions

`EditorBuffers` derives versioned overlays from strict disk snapshots; `serveEditor` and the VS Code client pass only explicit local read privileges. Federation signs immutable source envelopes and stores remote snapshots separately; it never transfers a live WAL database or grants remote source writes.

Readonly dependency declarations extend the compiler filesystem, while the TypeScript solution builder emits to a memory overlay only. Semantic extraction cache keys include program/config/reference/source dependencies. Metadata-assisted capture is a separately labeled weaker read mode, never source-write authority.

Advanced operations remain modules around the existing snapshot/provenance/change core: semantic providers, source-bound model enrichment, community projection, contract hints, bounded dataflow, test maps and media imports. Their command dispatch is in `cli-advanced.ts`. Source-changing provider operations prepare a ChangeManager plan rather than bypassing its preconditions.

Retention scans explicit roots under an exclusive-client transaction, quarantines unreferenced blobs and only purges earlier unreferenced trash. Failures/unknown usage survive model errors. Optional Docker is containment policy, not a silent fallback around arbitrary process execution. See the operation-specific guides for residual limits.
