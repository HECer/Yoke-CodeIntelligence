# Changelog

## 0.2.0 — 2026-10-13

- Add immutable TypeScript compiler filesystem and configured `tsconfig`/`jsconfig` discovery. Honor roots, inherited settings, path aliases and source-based project references; separate globals and analysis contexts. Reference cycles and unavailable targets are explicit diagnostics, not successful empty analyses.
- Preserve configuration identity on graph observations and diagnostics. Add `analysisContexts` trace filtering and expose cross-context union. Rebuild resolution when a captured config changes.
- Combine cross-project compiler rename locations and refuse edits when the same physical source binds differently in another configuration.
- Harden LSP/JSON-RPC framing, malformed response handling, callbacks, document versions, UTF-16/UTF-8 positions, LocationLinks and overlapping/resource edits. Cache text indexes during location conversion.
- Add explicit host-authorized `lsp` CLI/library operations over copied snapshots. Reads expose bound source locations; rename produces a persisted plan, never an implicit apply. Report unknown external usage and source-mutation failures.
- Preserve failed verification status when a subsequent stage is unavailable or times out; retain incomplete stages separately. Support standalone CR line endings in the shared source-position model.
- Add a two-package configured project example and new regression tests. The publisher derives the release tag from package metadata and refuses an existing release tag pointing at another local commit.

Live Graft/Graphify/Serena/third-party LSP runs, unsaved editor buffers, dependency installation and hosted multi-platform CI remain outside the verified scope. This is a local archive release, not a completed GitHub or npm publication.

## 0.1.0 — 2026-10-13

Initial independently written local implementation: immutable snapshot/CAS storage, SQLite FTS5, TypeScript compiler navigation/refactoring, Python AST and Go parser workers, directed evidence graph, compact retrieval, eight MCP tools, CLI and read-only loopback explorer.

Adds hash-bound patch preparation, copied-tree application, trusted verification profiles, approval-gated workspace integration, durable recovery and foreign-edit preservation. Adds strict optional Graft/Graphify imports and bridges, independent LSP/Serena transports, source-bound memories, precomputed vectors and LCOV/runtime/SARIF imports. Includes tests, a synthetic microbenchmark, documentation and a safe local GitHub publication script.

Development regression fixes include Unicode source offsets, grouped semantic payloads, directed imported edges, deleted Git-tracked files, rationale extraction, immutable result truncation, shared request versus semantic snapshot configuration, durable write recovery, and rejection of repository-selected interpreters.

This release is not the entire long-term specification, is not published to npm, and does not claim live upstream compatibility or production readiness. See docs/STATUS.md and the actual validation report.

## 0.3.0 — 2026-10-13

- Capture installed type declarations/metadata read-only and build TypeScript solution references entirely in memory.
- Add monotonic unsaved editor overlays, authenticated loopback editor API and locally packaged VS Code extension.
- Add real optional Jedi and Go/type semantic providers; validate actual clangd definition/reference/rename behavior.
- Add signed Ed25519 immutable source exchange, receiving-policy checks, fixed-root tenant/project authorization and federated retrieval.
- Add metadata-assisted capture, dependency-scoped semantic caches, watcher reconciliation and opt-in retention/quarantine/pinning.
- Add local statistical vectors, reviewed neural/model provider protocol, reranking and source-bound inferred summaries with failure-preserving billing.
- Add explicit NetworkX Louvain/official Leiden community adapters, richer contract hints and bounded intraprocedural dataflow candidates.
- Add per-test-file/name-pattern Node coverage collection. Fix zero-matching-case false success by using structured file-level summaries.
- Add native media ingestion and explicit optional interpretation providers; retain binary hashes and never fabricate ASR/scene meaning.
- Add optional fail-closed Docker verifier, expanded Serena gateway and full captured-source validation around upstream bridges.
- Extend public library/CLI, live gates and independent paired-driver benchmark harness. Keep current external/platform/audit limitations explicit.
- No GitHub or registry publication performed.
