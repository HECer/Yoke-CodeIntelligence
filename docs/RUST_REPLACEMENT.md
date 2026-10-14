# Rust replacement acceptance criteria

The goal is an independently implemented, MIT-licensed replacement for the useful local workflows of Graft, Serena and Graphify. A working adapter to an upstream application does not count as replacing that application. The native executable must work without Node, Python, upstream application packages, API keys or a model for its default indexing and retrieval path.

Upstream scope was rechecked on 2026-10-13 against the official repositories: [Graft / Trail Code](https://github.com/trailhq/Trail-Code), [Serena](https://github.com/oraios/serena), and [Graphify](https://github.com/Graphify-Labs/graphify). Product claims are not our measured results. This is a workflow replacement, not binary/CLI compatibility with those packages or a replacement for their proprietary hosted services.

## Architecture

Rust owns source capture, content-addressed storage, incremental parse reuse, deterministic structural extraction, directed evidence graphs, search, graph algorithms, controlled changes, CLI and MCP. Tree-sitter grammars parse source without executing it. Compiler-grade operations use an independently written LSP client and explicitly selected host executables; installing Serena or SolidLSP is unnecessary. Structural name candidates must never be advertised as compiler bindings.

SQLite WAL holds immutable metadata and FTS indexes; SHA-256 blobs hold authoritative bytes. Explicit snapshot queries never read current workspace bytes. Fresh queries capture saved bytes before answering. Parse cache entries depend on language, analyzer version and content hash; graph resolution is rerun when any file changes. Rayon parallelizes independent parsing, while persistence uses one writer transaction.

The existing TypeScript 0.3 implementation remains available with separate serialized state. Version 0.5 adds independently implemented Rust paths for the advanced local workflows below; the compiler and debugger use our own workers against explicitly selected actual toolchains. Existing state is not overwritten or converted in place.

## Acceptance matrix

| Workflow | Required evidence |
|---|---|
| Graft: structure, outlines, callers and context | Real multilingual AST fixtures, cross-file edges, parallel call sites, immutable source anchors, source-first context |
| Graft: freshness and efficiency | Deletion/creation/change tests; persistent hash cache; cold, unchanged and one-file-change release measurements |
| Graft: explanations and learnings | Source-bound authored rationale/document links; explicit model-generated inference; central AgentMemory integration where applicable |
| Serena: symbol overview and retrieval | Symbol/name-path lookup, literal/regex source search, dependencies identified separately, real source reads |
| Serena: semantic operations | Negotiated LSP capabilities, definition/references/implementations/hover/rename; UTF-16 conversion and URI/edit validation; actual server validation |
| Serena: symbolic editing | Replace/insert/delete symbol plans; no textual guess masquerading as semantic rename; precondition, approval, verification and journal tests |
| Serena: execution and debugger | Host-selected bounded verification; separate explicit runtime/debug authority; no repository-granted execution |
| Graphify: directed knowledge graph | Code/document/rationale entities, exact evidence, uncertainty labels, unresolved targets and parallel arcs |
| Graphify: queries and analytics | Bounded directed trace, shortest path, components, cycles, centrality, clearly named community algorithm |
| Graphify: export and visualization | Offline JSON/GraphML/HTML with escaped untrusted source, direction preserved, reproducible export |
| Graphify: multimodal and broader contracts | Existing native/provider functionality retained; each Rust migration separately exercised with actual input |
| Agent integration | Strict eight-tool MCP envelopes, initialization, errors, input/output budgets, artifacts, no hidden authority |
| Distribution | Locked Cargo dependencies, Windows build/test, cross-platform CI definitions, runnable release executable and installation guide |

## Completion rule

Only label a row native when its Rust path has real behavioral evidence. Protocol fixtures are useful but are not live language-server certification. A missing language, model, debugger, provider, commercial feature or migration remains visible. A clean build or a few passing tests cannot establish exhaustive parity with 40+ language servers, dynamic execution or hosted formal verification.

## Current executed native coverage

| Workflow | Rust implementation and evidence | Remaining boundary |
|---|---|---|
| Structure and context | 24 real grammars, multilingual exact call anchors, cross-file/parallel graph tests, source/rationale/document retrieval and bounded plans | Structural candidates remain distinct from compiler bindings; constructs/dynamic dispatch are nonexhaustive |
| Freshness and efficiency | Strict double capture, immutable historical queries, persistent hash cache, one changed-file reuse and measured 500-file release corpus | Fresh capture scans source bytes; whole-agent/upstream performance comparisons remain open |
| Configured compilation | Actual TypeScript 5.8.3 binding/type diagnostics, custom configs/extends/references, source-reference redirects and declaration-consuming virtual solution builds; exact compiler/library hashes enter context identity | TS/JS toolchain is optional host software; missing captured dependencies and unsupported build inputs remain explicit |
| Explanations and knowledge | Authored records, supersession/staleness, sparse/external vectors and imports; native executable providers for embeddings/query/reranking/cited summaries/media; durable integer ledger and local Ollama adapter | Neural outputs are inference; sent unknown costs remain reserved; source binding is not model attestation; project knowledge is separate from central AgentMemory |
| Semantics | Own LSP client, exact UTF-16/source conversion, persisted source/graph manifest checks, bounded copied-file hashing, capability/edit/version/URI validation and six MCP read operations; actual rust-analyzer, TypeScript and Jedi workflows | Other servers require actual certification; dependency/build contexts vary by server |
| Editing and verification | Exact plans, isolated apply, actual rustc and digest-pinned local Docker execution, approval/preconditions, OS locking, durable rollback and foreign-write tests | Test-count patterns are host assertions; Docker policy/cleanup is verified for the selected local Linux daemon, not every deployment |
| Debugging and execution | Persistent Rust-owned Python session with actual stack/variables/hash-bound breakpoints/step/next/continue/evaluate/REPL; separate execution/evaluation authority, bounded framed I/O and child lifecycle | Executed code retains host privileges; program effects outside copied source are unmonitored; main-thread Python debugging is not an all-language debugger |
| Dataflow and test mapping | Bounded TS/JS intraprocedural AST taint candidates; supplied maps and actual copied-snapshot Node per-selection V8 collection; conservative widening and atomic map publication | No heap/interprocedural/formal proof; coverage is observed execution, not exhaustive test adequacy |
| Graph and analytics | Directed multiedges, unresolved targets, cycles/components/PageRank/shortest paths and actual Leiden partition tests | Static observations do not establish complete runtime behavior or formal correctness |
| Contracts and media | Service-scoped SQL/Prisma/OpenAPI/GraphQL/Protobuf/schema/literal candidates; native image/audio/video metadata, supplied transcripts, actual bounded PDF worker and explicit media inference provider protocol | Runtime enforcement and neural accuracy are not established; local Ollama image inference requires a vision model and does not support ASR/audio/video/PDF interpretation |
| Export and agent integration | Offline JSON/GraphML/HTML, hostile-text escaping, strict eight-tool MCP, published artifact paging, foreign CAS denial, real editor HTTP and actual VS Code 1.140 host | Earlier live browser export evidence is retained; the latest preview reattachment could not reach localhost and is recorded separately |
| Source exchange and maintenance | Signed scoped offline/TLS source exchange; native CA/name-verifying client, role/grant-scoped principals and revocation; OS-held TLS/tokens/signing seeds; reviewable retention, durable quarantine/recovery/purge with reachability rechecks | Linux Secret Service/macOS Keychain require their actual OS services and separate live gates; no whole-agent-state sync, hosted administration UI or hostile-host isolation |
| Distribution | Locked native build, license inventory/portable packaging, matching native VSIX, three-platform CI definitions | Only this Windows host is exercised locally; hosted Linux/macOS CI and public publication are separate gates |

The 0.5 workflow validation record is [native-workflows.json](validation/native-workflows.json). The prior [0.4 record](validation/native-rust.json) remains historical. This matrix documents implemented local replacements and keeps exhaustive upstream/commercial parity separate from measured evidence.
