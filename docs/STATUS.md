# Release status — 0.2.0

This file separates implemented behavior from tested compatibility and the larger specification. No production-readiness, all-language completeness, formal-correctness or agent-token-savings claim is made.

## Implemented and locally exercised

Saved-file snapshots/CAS; SQLite/FTS5; UTF-8 exact reads; configured TS/JS symbol and reference extraction; cross-project compiler rename with context-conflict rejection; native Python AST and Go parser; source-first retrieval; directed multigraph traversal; bounded read plans; immutable patch plans; isolated application; approved workspace writes; conflict preservation; actual killed-writer recovery; static diagnostics; actual authorized Node test command execution; source-bound memory/vector/LCOV/runtime/SARIF features; CLI; stdio MCP; read-only authenticated loopback HTTP.

The test report records the exact count and platform. Native Python/Go tests were actually run. Tests use small fixtures, not arbitrary production-language completeness proofs.

## Implemented adapters; real upstream execution not verified here

| Adapter | Implemented boundary | Current verification |
|---|---|---|
| Graft | V1 graph import with source hash binding; public Graft.graph process bridge pinned to 0.21.1 | Schema/contract fixtures; no installed live Graft |
| Graphify | Raw extraction/persisted schema import; direction markers and parallel relationships; explicit root; bridge pinned to graphifyy 0.9.87 | Schema/contract fixtures; no installed live Graphify |
| LSP | Independent framed client plus host-authorized CLI; checked definition/reference/implementation locations, prepareRename, versioned edits and persisted rename plans | Adversarial protocol/process fixtures and CLI tests; no real external language server run |
| Serena | External MCP initialize, symbol query and grouped-payload normalization | MCP fixture; no live Serena run |

Graft's optional meaning model, Serena's complete tools/REPL/JetBrains backend and Graphify's multimodal/cloud capabilities are not silently reimplemented or bundled. Installing an upstream does not automatically enable all its features through this facade.

## Deliberately limited in this release

| Area | Actual boundary / next work |
|---|---|
| TypeScript build context | Captured `files`/`include`/`exclude`, inherited configs, aliases, independent programs and source-based project references are implemented. No automatic node_modules/@types loading, arbitrary plugins, generated declaration build validation or `tsc --build`. Files outside all configured programs remain searchable with explicit semantic omissions. |
| Python and Go | Syntax/AST heuristics, not complete binding/type resolution or universal safe rename |
| Frameworks/contracts | Selected syntax-based route/event/env/SQL/JSON schema hints; not complete ORM, migration or interservice proof |
| Graph analytics | Degree, weak components, cycles and diff; no Leiden community implementation |
| Retrieval | FTS/literal/graph context; optional precomputed vectors; no built-in embedding model or LLM reranker |
| Incrementality | Parse caching and immutable graph reuse; new disk capture still hashes source bytes and new snapshots rerun global resolution |
| Editor integration | Saved files only; no unsaved-buffer overlay or IDE plugin |
| Multiple projects/devices | Separate workspaces; no authenticated federation, cross-machine state synchronization or team tenancy |
| Verification | Bounded diagnostics plus explicitly trusted commands; no formal verification or general taint/dataflow analysis |
| Test selection | Graph/filename suggestions and aggregate LCOV; not a complete per-test dependency/coverage map |
| Execution security | No project execution by native index; copied trees are not OS/container sandboxes for trusted external programs |
| Storage | No automatic garbage collection/retention policy or hosted database service |
| Benchmarks | Local synthetic microbenchmark only; no full paired agent/Graft/Serena/Graphify cost-correctness sweep |

## Validation boundaries

Linux Node 22.16, Python 3.13 and the available Go toolchain were used. macOS/Windows and Node 24 jobs are configured but were not run in this session. Windows symlink privilege-dependent tests may be skipped. Node's SQLite API is experimental in the tested Node version.

The current report records the re-run suite and package smoke checks. Browser fixture checks in `validation/v0.1.0` are historical, not re-run claims for this update. A clean online npm installation and GitHub-hosted CI did not run because the build container lacked network access. Local build used the available pinned TypeScript and Node type packages; lockfile structure was checked offline. The browser's managed policy blocked live localhost navigation. The actual frontend passed an offline DOM fixture test; independent Node HTTP tests exercised the real backend, authorization and origin restrictions. Do not conflate these with a live browser-to-backend end-to-end test.

## Public publication

**No publication was performed in this update.** The prior attempt verified owner HECer, but the target returned 404 and the connector lacked repository creation. No current remote-state claim is made without a fresh lookup. The supplied local publisher performs the requested public creation and verifies the pushed commit using the user's own GitHub CLI authentication. No npm registry release or hosted release asset upload has been performed.

## Next acceptance gates

Live upstream version tests, larger real-world configured-project corpora, unsaved-buffer integration, independent patch-task benchmarks, and three-platform hosted CI must pass before stronger compatibility or maturity claims are made. Preserve the current source-bound failure semantics while extending capabilities; do not hide missing analysis behind empty result sets.

## Configured-project implementation dependencies

TypeScript is deliberately pinned to 5.8.3. Immutable wildcard matching and editor-style source redirects use compiler-exported/internal hooks (`matchFiles`, `useSourceOfProjectReferenceRedirect`) behind a narrow host adapter. Contract tests must pass before changing that compiler pin. Source reference navigation is not equivalent to building declarations. The final public graph is a union unless an analysis-context filter is supplied; that union is not one executable configuration.
