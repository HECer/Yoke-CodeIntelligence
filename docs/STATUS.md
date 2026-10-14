# Release status — native 0.4.0 and retained TypeScript 0.3.0

## Current native Rust implementation

The default Rust executable independently owns immutable source capture/storage, 24-language structural analysis, exact UTF-8 evidence, incremental parse reuse, FTS/BM25 retrieval, bounded context plans, directed graphs/Leiden, offline exports, eight MCP tools, guarded changes/recovery and a host-selected LSP client. Graft, Serena and Graphify are unnecessary for these operations; Node/Python/model calls are unnecessary for the default native runtime.

Rust also implements service-scoped contract extraction, source-bound knowledge and statistical/external vectors, LCOV/runtime/SARIF imports, versioned unsaved overlays and authenticated loopback editor requests. Technical image/audio/video inspection, supplied transcript binding and actual PDF text extraction use native code. PDF decompression runs in a separate process with bounded memory/CPU/output and a shared deadline; it is not a general OS sandbox.

Current evidence includes Windows MSVC builds/tests, actual rust-analyzer and TypeScript language-server definition/rename runs, historical-snapshot references through actual rust-analyzer MCP, real Leiden partitions, native PDF subprocess tests and an actual VS Code 1.140 extension-host run. The VS Code run preserves saved source, finds unsaved Rust symbols, ignores repository-selected executables and excludes an open protected buffer. Lifecycle/cancellation races have separate labeled protocol regression tests. The standalone HTML export was exercised against captured source in the actual browser, including hostile text and a narrow viewport.

The native standard suite passes with **159 passed, 5 explicitly ignored live/integration gates, 0 failed**. All three actual language-server gates pass separately; the two actual Windows credential-store gates also passed in this session. The TypeScript suite passes on this Windows host with Node 24: **240 passed, 3 skipped, 0 failed**. Logs, release measurements and package checks are recorded in [native validation](validation/native-rust.json) and its distribution companion. Cross-platform CI definitions are present; hosted Linux/macOS runs have not been executed here.

Offline Ed25519 source exchange and conservative quarantine maintenance now have independent Rust CLI/library paths and real behavioral tests. Windows Credential Manager round-trip/overwrite/concurrent-creation gates also passed. No private keys are exported or persisted in project configuration; receiving grants remain explicit host inputs. Generic artifact reads cannot bypass foreign-source grants through an unpublished raw CAS hash.

Remaining Rust migration includes configured compiler/solution-build ownership, neural/model/media providers, selected dataflow/per-test execution, full signed service transport, destructive history retention and Docker verification. Persistent debugger/REPL workflows, all-language-server certification, proprietary hosted products and whole-agent cost/correctness comparisons remain unverified. The [replacement matrix](RUST_REPLACEMENT.md) defines the evidence needed before claiming completion.

## Historical TypeScript 0.3.0 report

The following report preserves the earlier environment's results. Its claims that Windows/Node 24, a real browser, native Leiden or a VS Code host were unavailable are historical and are superseded by the current evidence above. It does not establish the implementation or validation of a Rust port.

**A substantially extended working implementation, not certified completion of every originally discussed possibility.** This document distinguishes working code, real local evidence, optional-provider contracts and unexecuted external gates. No public publication was performed or attempted in this update.

## Implemented and exercised locally

| Area | Delivered behavior and actual boundary |
|---|---|
| Existing core | Immutable CAS snapshots, FTS, directed multiedges, provenance, source-first context, configured TS/JS, eight MCP tools, CLI, controlled changes/recovery, verification, source-bound memory and imports remain regression-tested. |
| TS dependency/build context | Installed declaration/metadata capture is read-only. In-memory solution builds produce and consume project-reference declarations without source-tree writes. No automatic installs, external symlinked package stores or custom compiler plugins. |
| Python / Go semantics | Actual Jedi cross-file definition/reference/rename plans and actual Go `go/types` binding/typechecked rename plans. Missing dependency/context fails explicitly. Reflection/dynamic dispatch/other build configurations remain nonexhaustive. |
| External LSP | Actual clangd definition/reference/rename-plan tests, in addition to adversarial protocol fixtures. Not an all-server compatibility claim. |
| Editor | Versioned unsaved-buffer snapshots and authenticated loopback API; actual CLI-to-HTTP overlay search and original-source preservation tested. VS Code extension supplied and packaged, but no actual extension-host test. |
| Federation / sync | Ed25519 source snapshot transfer, host-controlled tenant/project grants, immutable remote snapshots, local reindexing, federated search, replay handling and receiver excludes. Real HTTP round trips and denial tests. Not whole-agent-state synchronization, hosted user management or CRDT collaboration. |
| Incrementality / retention | Metadata-assisted capture plus strict mode, semantic-unit dependency keys, filesystem watch with reconciliation, opt-in periodic retention, exclusive-client mark/sweep, quarantine and pinning. Not a fully incremental compiler; audit roots can deliberately retain history. |
| Meaning / embeddings | Native statistical lexical vector generation/search and reviewed provider interfaces for neural encoding, reranking, source-bound inferred summaries and media descriptions. Model protocol/accounting tested with labeled fixtures. No model weights, embedding-model live run or LLM run included. |
| Communities | Real NetworkX Louvain execution. Official Leiden adapter implemented, but required igraph/leidenalg packages unavailable; no mislabeled fallback. |
| Contracts | Additional Prisma fields/mappings, SQL operations/migrations, literal HTTP/OpenAPI, GraphQL and Protobuf extraction plus selected OpenAPI change checks. Cross-links remain heuristic, not complete runtime contract verification. |
| Tests / dataflow | Actual isolated Node test-file and name-pattern execution with V8 per-selection line coverage and widening on unknown impact; structured summaries reject zero-case success. Bounded TS/JS intraprocedural taint candidates with explicit source/sink assumptions; not full heap/interprocedural analysis or formal verification. |
| Media | Actual PDF text/page, PNG metadata and WAV metadata tests. ffprobe and subtitle handling implemented. ASR, OCR and scene understanding need a reviewed provider or supplied transcript; metadata is never mislabeled as semantics. |
| Process containment | Optional digest-pinned Docker verifier with no network/pulls, limits and fail-closed cleanup. Argument/security contract checked; no live Docker daemon here. Ordinary trusted copied-tree processes are not an OS sandbox. |

Exact test totals, versions and package checks are in `validation/report.json`. The synthetic microbenchmark is not an agent-quality/cost benchmark. The paired-driver acceptance harness is implemented and its accounting/independent-verifier behavior is fixture-tested; no genuine multi-arm agent campaign was run.

## Remaining compatibility gates — not passed

| Gate | Observed reason / status |
|---|---|
| Installed Graft 0.21.1 | Actual live attempt fails because the package is missing. Pinned structural bridge and source checks are implemented. |
| Installed Graphify 0.9.87 | Actual live attempt fails because `graphifyy` is missing. Raw extraction/root/direction contracts are implemented. |
| Installed Serena | No installed reviewed executable configured. Capability discovery, selected tools, safe-edit planning and explicitly authorized runtime gateway have protocol tests only. |
| Paid / hosted features | Graphify Cloud and Serena JetBrains require the user's external products/licenses; neither source nor service is bundled. Compatibility must be established against the actual installation. |
| Clean online install | `npm ci` attempted and failed on registry DNS resolution (`EAI_AGAIN`). The exact pinned local runtime/development dependencies were restored offline for build/test. |
| Live browser end-to-end | Actual localhost navigation was refused by managed Chromium (`ERR_BLOCKED_BY_ADMINISTRATOR`). No bypass attempted. Frontend fixture checks and real HTTP tests are separate evidence. |
| Platforms / extension host | Windows, macOS, Node 24, GitHub-hosted CI and VS Code extension-host execution were not available. Matrix/optional live jobs prepared, not claimed run. |
| Neural model / Leiden / Docker | Required packages, weights or daemon absent. Explicit optional interfaces exist; missing dependencies do not pass a live gate. |
| Independent audit / full agent benchmark | Neither performed. An internal review or protocol fixture cannot substitute for independent security certification or real agent cost/correctness evidence. |

## Important non-guarantees

The static graph is not the complete runtime. A successful reference query is not proof of absence elsewhere. Python/Go/TS resolution depends on captured configurations and dependencies; dynamic/generated code, unsupported languages and omitted files remain visible limits. Source signatures authenticate bytes/scope, not correctness. Filesystem metadata mode is weaker than a strict capture. Non-cooperating external writers, an untrusted tool executable and a hostile host need operating-system-level controls beyond a library.

The facade deliberately does **not** merge all upstream applications into one implicit privileged process. The native path does not need a model. Advanced provider/network/key/retention privileges remain host CLI/library operations, not automatically granted to an MCP caller. See `ADVANCED.md`, `FEDERATION.md`, `SECURITY.md` and `UPSTREAMS.md` for operation-specific boundaries.

Version 0.3 is therefore usable for continued development and bounded local workflows; calling it universally production-ready, fully externally verified or complete feature-for-feature parity with every upstream product would be inaccurate.
