# Configured projects and host-selected language servers

## TypeScript / JavaScript

0.2.0 discovers captured `tsconfig.json` files and, where no sibling tsconfig exists, `jsconfig.json`. Explicitly referenced configuration files can use other names. An `extends`-only base is not activated as an extra project. With no configs, one `[inferred]` program provides the original default behavior.

The compiler parses `files`, `include`, `exclude`, `extends`, aliases and references against the immutable filesystem. Relative inherited paths retain their declaring config's origin. Exclusion limits roots; a file imported by an included root can still enter the program. Configuration plugins never load. Uncaptured external references and configuration errors are reported. Discovery is capped at 128 configurations.

Each configuration has its own program. A file may appear in more than one program and resolve imports differently. `coverage.analysisContexts` lists `id`, `configPath`, `rootFiles`, `analyzedFiles`, `references` and limitations. Per-language `semanticFiles` is the union of analyzed files, not a claim that every captured file was semantically checked. Omitted files remain available to exact reads and text/file search.

Run the included example:

```sh
node dist/cli.js index --root examples/monorepo --timeout 120000
node dist/cli.js find subtotal --root examples/monorepo --timeout 120000
node dist/cli.js verify --root examples/monorepo --timeout 120000
```

Use actual returned IDs with `ci_change_prepare`. Native rename gathers locations in every applicable configured program. When a shared file resolves the same source position to another target under a second configuration, it fails with `RENAME_CONTEXT_CONFLICT` rather than silently changing that configuration's behavior.

Graph observations and diagnostics include an `analysisContext` when produced by the TypeScript compiler. Filter a graph traversal to the desired context:

```json
{
  "entityIds": ["<actual-id>"],
  "direction": "in",
  "relations": ["calls", "references"],
  "analysisContexts": ["packages/app/tsconfig.json"]
}
```

Send this to `ci_trace`. The CLI equivalent adds `--analysis-context packages/app/tsconfig.json` to `trace`. Without a filter, the graph is a union and `crossContextUnion` identifies traversals containing multiple contexts. A combined path is not evidence that one build configuration contains every edge. Unlabelled third-party edges are not included when an explicit context filter is used.

**Build boundaries.** Project references use editor-style source redirects: no emitted `.d.ts` build is required. This is not `tsc --build`, generated API validation or a substitute for your actual build pipeline. Only captured source and the pinned trusted TypeScript standard library are read; normal external `node_modules`/`@types` are not loaded. Reported unresolved external dependencies are not suppressed as success. Saved files only: editor buffers are not connected.

## Explicit LSP CLI

An installed language server is an external executable. Review it and its arguments before granting `--trust-providers`. The following paths are placeholders for **your existing installed server**, not instructions to run an unreviewed package download:

```sh
node dist/cli.js lsp references src/invoice.ts --root /absolute/project \
  --command /absolute/path/to/language-server --arg=--stdio \
  --byte 24 --trust-providers --timeout 120000
```

Operations: `definition`, `references`, `implementation`, `rename`. Positions are exact, zero-based **UTF-8 byte offsets**, not character offsets. `--arg` is repeatable and is passed as an argument vector without shell interpolation. Use `--arg=--stdio` when the value starts with a dash. `{workspace}` in server arguments expands to the materialized snapshot root. This entrypoint currently opens TS/JS, Python or Go source documents, matched to the requested file language; it is not automatic universal language support.

```sh
node dist/cli.js lsp rename src/invoice.ts --root /absolute/project \
  --command /absolute/path/to/language-server --arg=--stdio \
  --byte 24 --new-name calculateInvoice --trust-providers --timeout 120000
```

This **only prepares a plan** and returns `planId`; inspect its diff, run `verify <planId>` and explicitly `apply <planId>` separately. There is no implicit write permission. Resource operations, ambiguous/conflicting ranges, unknown required annotations, stale versioned edits and out-of-snapshot writes are rejected.

The server sees a copied snapshot. Source hashes are checked after shutdown is requested, and the temporary tree is removed when possible. This detects ordinary mutation of captured source; it does **not** prove a hostile server's internal analysis, prevent background activity, attest full semantic coverage or enforce network isolation. Native model calls are zero, while external provider usage is `not_reported`; `localOnly` is false. Missing dependencies in the copy can limit a real server. Unknown external locations are listed but their files are not opened.

Returned locations have source hashes, half-open UTF-8 ranges and optional selection ranges. LocationLink direction/ranges and UTF-16 surrogate boundaries are checked. Oversized read positions can be clamped to line content and are flagged; edits are never silently clamped. Diagnostics from unopened documents or stale versions are ignored. Server-initiated workspace edits are refused.

Library users can import `runLsp` and `LspCommandOptions` from `dist/index.js`. `LspAdapter` remains available for a host-managed connection. Prefer its checked `locations()` over raw `query()` output. The one-shot CLI does not add more tools to the eight-tool MCP interface.

## Verified scope

Compiler behavior, monorepo references/renames, malformed protocol peers and the real local CLI process are tested. The scripted provider is a **fixture**, not a real third-party server. Live installed Graft, Graphify, Serena and LSP server runs remain an acceptance gate.

Primary specifications consulted for implementation:

- https://www.typescriptlang.org/tsconfig/extends.html
- https://www.typescriptlang.org/tsconfig/paths.html
- https://www.typescriptlang.org/docs/handbook/project-references.html
- https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
