# Release status — 0.1.0

This file separates implemented behavior from tested compatibility and the larger specification. No production-readiness, all-language completeness, formal-correctness or agent-token-savings claim is made.

## Implemented and locally exercised

Saved-file snapshots/CAS; SQLite/FTS5; UTF-8 exact reads; TS/JS symbol and reference extraction; TypeScript compiler rename; native Python AST and Go parser; source-first retrieval; directed multigraph traversal; bounded read plans; immutable patch plans; isolated application; approved workspace writes; conflict preservation; actual killed-writer recovery; static diagnostics; actual authorized Node test command execution; source-bound memory/vector/LCOV/runtime/SARIF features; CLI; stdio MCP; read-only authenticated loopback HTTP.

The test report records the exact count and platform. Native Python/Go tests were actually run. Tests use small fixtures, not arbitrary production-language completeness proofs.

## Implemented adapters; real upstream execution not verified here

| Adapter | Implemented boundary | Current verification |
|---|---|---|
| Graft | V1 graph import with source hash binding; public Graft.graph process bridge pinned to 0.21.1 | Schema/contract fixtures; no installed live Graft |
| Graphify | Raw extraction/persisted schema import; direction markers and parallel relationships; explicit root; bridge pinned to graphifyy 0.9.87 | Schema/contract fixtures; no installed live Graphify |
| LSP | Independent framed process client; initialize, definitions/references, UTF-16 changes and rename patches | Protocol peer fixture; no real external language server run |
| Serena | External MCP initialize, symbol query and grouped-payload normalization | MCP fixture; no live Serena run |

Graft's optional meaning model, Serena's complete tools/REPL/JetBrains backend and Graphify's multimodal/cloud capabilities are not silently reimplemented or bundled. Installing an upstream does not automatically enable all its features through this facade.

## Deliberately limited in this release

| Area | Actual boundary / next work |
|---|---|
| TypeScript build context | Indexed source and trusted standard library only; no automatic node_modules/@types loading, arbitrary plugins or complete multiple-tsconfig/project-reference support |
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

A clean online npm installation and GitHub-hosted CI did not run because the build container lacked network access. Local build used the available pinned TypeScript and Node type packages; lockfile structure was checked offline. The browser's managed policy blocked live localhost navigation. The actual frontend passed an offline DOM fixture test; independent Node HTTP tests exercised the real backend, authorization and origin restrictions. Do not conflate these with a live browser-to-backend end-to-end test.

## Public publication

At delivery, **not published from this session**. Authenticated owner HECer was verified, but the target returned 404 and the connector lacks repository-creation capability. The supplied local publisher performs the requested public creation and verifies the pushed commit using the user's own GitHub CLI authentication. No npm registry release or hosted release asset upload has been performed.

## Next acceptance gates

Live upstream version tests, production multi-tsconfig fixtures, adversarial external-server tests, unsaved-buffer integration, independent patch-task benchmarks, and three-platform hosted CI must pass before stronger compatibility or maturity claims are made. Preserve the current source-bound failure semantics while extending capabilities; do not hide missing analysis behind empty result sets.
