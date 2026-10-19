# Architecture and design decisions

## Native Rust ownership

`postgres_catalog` validates received UTF-8 JSONL and reconstructs a standalone
typed catalog archive and directed graph. `postgres_catalog_client` supplies
fixed read-only SQL through a separately trusted/pinned host psql18 process with
explicit endpoint, network and credential authority. Capture claims remain
unverified data; replay never creates a code SourceSnapshot or triggers fetch.
OID identities, parallel constraints, ordered column metadata and raw record
byte evidence stay separate. See [PostgreSQL catalogs](../native/docs/postgres-catalog.md).

`git_workflows::owners` adds per-path authored rule interpretations over an
unchanged saved impact/area parent. An internal area callback holds the same
checked read transaction through captured rule reads and result construction;
public area bytes and legacy owner/report identities stay unchanged. Exact
rule/token byte evidence, base/head and unknown syntax/precedence remain
separate. See [captured owner candidates](../native/docs/git-owners.md).

`meaning_projection` consumes native group/join exports as bounded, untrusted
presentation data. It checks membership, policy identities, original directed
endpoints, evidence references and ordered hyperedge slots, then performs
storeless queries. Import never creates `meaning_join::JoinedGraph`; exported
replay and attestation claims stay nested data with top-level source authority
false. See [portable Meaning projections](../native/docs/meaning-projection.md).

`changes_plan` replays the released saved-plan identity and complete base/result
delta, then reconstructs exact diff bytes and warnings under checked source
policies. Approval, verification, new apply and recovery share this admission;
read transactions and SQL/deadline policy are scoped and restored. See
[saved plan integrity](../native/docs/plan-integrity.md).

`changes_directory` binds one complete captured text tree, including empty
directories, modes and physical directory identities. A matching standard-LSP
folder request contributes checked original-coordinate text repairs; the native
client owns the single directory resource effect. Copied verification checks
the declared shape, while approved workspace application and conditional
recovery use one filesystem rename. See [directory moves](../native/docs/directory-moves.md).

`semantic_file_delete` admits a finite captured ES module and checks graph and
literal usage candidates before source-bound definition/reference requests.
Bulk reads share one checked captured scope while retaining per-file policy,
hash and cancellation checks. Only after all checks does ordinary guarded
deletion preparation publish a plan. [File usage checks](../native/docs/file-usage.md)
describe supported declarations and unknown consumers.

`changes_symbol_delete` derives independently removable TypeScript/JavaScript,
Python and Rust declaration ranges from exact captured AST/entity bindings.
Export, ambient and decorator wrappers belong only to their owning declaration;
Rust reuses `refactor_contract`'s owned outer-attribute/documentation bundle.
Stored entity and usage-evidence coordinates retain their original ranges. See
[symbol deletion boundaries](../native/docs/symbol-deletion.md).

`semantic_propagated_delete` binds one finite ES function module to captured
AST/entity ranges and exact host-provider definitions/references. Removed
source regions seed a directed dependency closure; surviving reported or
possible consumers retain candidates and their dependencies. One guarded plan
contains nonoverlapping original-coordinate removals after captured physical
sources are rechecked. [Function propagation](../native/docs/propagated-delete.md)
records capacities and unknown consumers.

`native/src` is an independent runtime with its own `.yoke` SQLite/CAS state. `capture`, `paths` and `store` own strict saved-source acquisition, immutable bytes, FTS and client/writer locking. `analysis` and `contracts` produce structural and authored observations; `query` owns retrieval, plans and directed analytics. `lsp` owns the host-selected semantic protocol, with exact source anchors and bounded excerpts. `changes`, `verify` and `process` own reviewed plans, copied execution, conditional original writes and recovery.

Checked Store lock waits retain the original absolute deadline and host
cancellation flags in connection-owned state. Record mutations recheck those
conditions after serialization; ledger transaction admission shares the same
policy. [SQLite deadline ownership and boundaries](../native/docs/sqlite-deadlines.md)
describe callback lifetime, recovery and the independent raw-connection limits.
Store opening shares its setup deadline across client locks and schema/identity
work, inherits enclosing host cancellation and clears that policy on success.

`editor` derives versioned immutable overlays; `editor_http` exposes authenticated loopback read operations. The VS Code client selects executables from user settings and queues document versions through startup/shutdown. `knowledge` validates authored assertions, sparse/external vectors and imported observations. `media` binds assets and supplied text; PDF parsing uses a resource-limited child rather than uninterruptible parsing in the main process. `federation` and `maintenance` have separately scoped integration and acceptance tests.

Rust 0.5 adds `projects` and its own compiler worker, `dataflow`, `test_map`, `providers`/`ledger`/`ollama`, `runtime` and its own Python worker, `container`, `retention`, `federation_http` and native OS credential modules. External toolchains are explicit host inputs. Model calls and execution remain outside default retrieval. Compiler/library hashes enter context identity; model attempts reserve before sending; retirement and purge recheck every durable authority root.

`graph_view` composes explicitly selected structural and semantic overlays while preserving their source bindings and precision. Ordinary retrieval can select a saved View, and `graph_extensions` prepares and admits selected-View reports before publishing their artifacts. Canonical graphs and existing canonical report identities remain separate from these projections.

`meaning_graph` validates typed concepts, directed relations and role-bearing hyperedges against exact submitted UTF-8 source ranges. `meaning_build` reserves all admitted attempts before sending, accounts for reported costs before rejecting invalid domain output, and publishes the completed graph, cache pointers and immutable receipt references together. Cache identity includes the source unit, extraction contract, proven analyzer epoch, provider inputs, generation and currency. `meaning_cli` separates explicit trusted builds from provider-free load/query/status operations. `retention` validates the typed job, cache, graph and report dependencies before retiring history or purging CAS content.

`meaning_aliases` replays a published meaning graph before proposing bounded
lexical matches between its concept labels and declared aliases. Comparison
keeps packet-qualified concept references and exact original evidence, performs
no model calls or persistence, and never grants semantic identity to a label or
similarity score. The read-only `meaning aliases` command applies independent
pair, character-work, candidate and output limits before publishing stdout.

`meaning_groups` projects explicitly host-selected, disjoint concept groups over
one replayed meaning graph. Every original concept, directed parallel relation,
hyperedge slot and evidence record is preserved. Group-induced selfloops and
repeated projected participants remain visible in a separate projection schema.
The read-only `meaning group --review HOST_JSON` command admits strict bounded
host assertions before opening state; no provider, capture or persistence is
introduced. Host assertions do not establish semantic identity or authenticated
human review. See the [group projection contract](../native/docs/meaning-groups.md).

`git_workflows::conflicts` reads the explicit host worktree's unmerged index
through the existing guarded Git process boundary. Independent host flags grant
index metadata and stage-body reads; checked native capture lineage limits each
quoted path. Matching pre/post index observations, verified raw object hashes and
stage-qualified UTF-8 pages stay separate from canonical saved sources, graphs
and current working files. One retained read transaction pins scope metadata;
no report, source, index or latest-pointer write occurs. See
[index conflict triage](../native/docs/git-conflicts.md).

`git_workflows::areas` replays an unchanged sealed impact-detail parent and
optionally selected saved native Meaning graphs inside one read transaction.
Snapshot/graph/file bindings constrain concept memberships; packet/position
references, original quotes and every tied smallest file-set claim stay visible.
Directory fallback is labeled a path fact. No provider, capture or publication
is introduced; the [area contract](../native/docs/git-areas.md) records admission,
resource bounds and the limits of these interpretations.

TypeScript directories and ADRs below describe the retained runtime. Snapshot identity and serialized state are runtime-specific; no implicit conversion occurs. [The native guide](../native/README.md) and [replacement criteria](RUST_REPLACEMENT.md) describe current boundaries.

`github_app::digest::service` publishes only freshly replayed handoff projections
under separate host service/network/write authority and exact-payload approval.
Domain-separated App-key MACs and a hash-chained inline journal bind intents and
receiver receipts. No SQL transaction spans HTTP; unknown writes prohibit retry
and recover through a bounded read of the same receiver/proposal. Typed source
selectors retain immutable captures. Receiver acceptance is distinct from
processing and from proprietary upstream service equivalence. See the
[service contract](../native/docs/digest-service.md).

`github_app::digest::service::receiver` admits bounded host-granted digest intake
through the shared native TCP/TLS channel. Fixed bearer-hash grants bind sender
App/workspace/installation/repository scope. Exact wire bytes and an immutable
receipt commit together before HTTP success; GET and identical repeats replay
the same receipt. Imported IDs never become local source/CAS/retention authority.
Current grants, revocation, original host cancellation and request deadlines
govern the worker. This data store supplies no model or source-execution pipeline;
see the [receiver contract](../native/docs/digest-receiver.md).

`receiver::query` deterministically projects selected accepted metadata on read.
Independent host/per-grant permissions protect local read-only and authenticated
HTTP queries. Receipt-qualified JSON pointers preserve duplicate positions,
directed parallel relations and sender-claim provenance; no imported ID becomes
a local graph/CAS identity. The remote query client reads only the service token,
checks request/scope/row protocol bindings and opens no sender Store or signing
key. Model calls, execution, semantic identity and source verification remain
unavailable. See the [query contract](../native/docs/digest-query.md).

`digest::acquisition` selects a bounded verified-head history batch and invokes
native Git capture under one owned immediate transaction. A shared acquisition
budget precedes commit/tree/blob admission; fresh source mapping and output
encoding precede publication. Store publication can participate only through a
crate-private pending path; ordinary publication retains its own transaction.
Snapshot, graph, FTS, inherited scope, artifact registration and source receipt
roll back together. CAS bytes can remain unreferenced after failure and confer
no authority. Single-commit Git captures now share this atomic SQL boundary.
See the
[acquisition contract](../native/docs/digest-history-acquisition.md).

`digest::history_jobs` checkpoints explicit local acquisition with immutable
head/origin selection, current host grants, cumulative budgets and exact revision
preconditions. `git_history_read::cursor` reconstructs date-priority traversal
from hash-verified raw commit objects and an ordered visited prefix; persisted
queue labels or exhaustion claims carry no authority. Lookahead can checkpoint
without choosing from a partially verified frontier. Each mapped batch and job
revision publish in one owned immediate transaction after output admission.
Status uses existing-only read-only state and labels capture references as
recorded progress; it does not replay every historical source body. Retention
validates closed local source selectors instead of walking commit text or Git
IDs. [Persistent job contract](../native/docs/digest-history-jobs.md) records the
finite ceilings and remaining unattended App/semantic/platform boundaries.

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
