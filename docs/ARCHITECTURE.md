# Architecture and design decisions

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
