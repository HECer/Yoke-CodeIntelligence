# Code Intelligence

**Local, evidence-first code intelligence for coding agents.**

[Deutsch](README.de.md) · [Quick start](docs/QUICKSTART.md) · [Advanced 0.3 features](docs/ADVANCED.md) · [Status](docs/STATUS.md) · [Validation](docs/validation/report.json)

Independent TypeScript core combining immutable source snapshots, compiler navigation, directed multiedges, source-bound retrieval, controlled change plans and verification. Optional adapters connect Graft, Graphify, Serena and external language servers without treating every upstream claim or inferred edge as established program truth.

## Run

Requires **Node.js 22.16+**. Python/Go and additional optional providers are installed separately. Node SQLite is experimental on the tested Node version. No model or API key is required by native indexing.

```sh
npm ci
npm run build
node dist/cli.js index --root /absolute/project --timeout 120000
node dist/cli.js find invoice --root /absolute/project
node dist/cli.js view --root /absolute/project --timeout 120000
```

The separately supplied runtime ZIP includes compiled code and the pinned TypeScript runtime dependency; start it directly with `node dist/cli.js --help`. Source ZIP includes Git history but excludes installed dependencies. No public repository or registry publication was performed in this update.

## Delivered in 0.3.0

| Capability | Implementation |
|---|---|
| Source truth | Strict snapshots, exact UTF-8 anchors, readonly captured dependencies, editor overlays, signed remote snapshots, explicit coverage limits |
| Configured semantics | TS/JS compiler contexts and rename, memory-only solution builds, actual optional Jedi/Go semantics, framed LSP and Serena capability gateway |
| Retrieval | FTS, literal search, directed traversal, bounded context plans, source-bound memory, local lexical vectors and optional model-generated vectors/reranking |
| Relationships | Additional Prisma/SQL/OpenAPI/GraphQL/Protobuf hints, candidate contract links, bounded intraprocedural dataflow, explicit Louvain/Leiden adapters |
| Changes and tests | Prepared immutable patches, isolated apply, approved original-tree writes, recovery journals, diagnostics, per-test-selection V8 coverage and optional Docker verification |
| Integration | Eight MCP tools, CLI/library, local explorer, authenticated editor service/VSIX and host-scoped source federation |
| Operation | Metadata-assisted capture, dependency-scoped semantic caches, periodic watch reconciliation, opt-in retention/pins, model-attempt cost ledger |

![Explorer — actual UI with labeled fixture responses](docs/screenshots/explorer.png)

The screenshot is a frontend fixture test, not evidence of a successful real browser-to-backend test. Managed Chromium blocked localhost navigation; real HTTP authorization and query paths are tested independently.

## Agent interface

`node dist/cli.js serve --root /project` exposes **ci_status, ci_find, ci_read, ci_trace, ci_context, ci_change_prepare, ci_change_apply, ci_verify**. Read exact source before edits. Responses bind a snapshot and distinguish omissions, uncertain relationships and failures. Native tools do not get implicit provider/network/key permissions. Advanced privileges remain explicit host-side CLI/library operations; see [API](docs/API.md).

## Checks

```sh
npm run typecheck
npm test
node scripts/configured-smoke.mjs
node scripts/live-tests.mjs native       # real Jedi/Go/clangd/PDF/image/audio providers required
node scripts/live-tests.mjs communities  # real NetworkX required
node scripts/live-tests.mjs upstreams    # actual pinned Graft/Graphify/reviewed Serena required
node benchmarks/run.mjs                 # synthetic native retrieval, not agent economics
```

Optional live gates **fail** when prerequisites are absent; they do not fall back to fakes. The default suite includes deliberately labeled protocol fixtures and real native operations. The exact completed runs and unresolved gates are in [the report](docs/validation/report.json).

## Evidence boundaries

No feature-for-feature parity with every upstream/commercial product, exhaustive multi-language semantics, formal safety proof, independent audit or measured whole-agent token savings is claimed. Graft/Graphify/Serena live installation, neural model/Leiden/Docker execution, real VS Code host, Windows/macOS/hosted CI and the full paired-agent comparison remain externally unverified here. Clean online npm install failed on DNS; the tested pinned compiler/runtime was restored offline. [STATUS.md](docs/STATUS.md) states each limitation instead of reporting missing capabilities as successful empty results.

## License

Own implementation: MIT. TypeScript runtime notices retained. Optional upstream applications, model weights and media libraries have their own licenses and are **not** bundled. No GPL Serena application code, paid JetBrains plugin or hosted service is included. External commands in copied trees retain process privileges unless a separately validated host/container sandbox is used.
