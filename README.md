# Code Intelligence

**Local, evidence-first code intelligence for coding agents.**

[Deutsch](README.de.md) · [Rust core](native/README.md) · [Quick start](docs/QUICKSTART.md) · [Advanced 0.3 features](docs/ADVANCED.md) · [Status](docs/STATUS.md)

Independent Rust 0.4 core and retained TypeScript 0.3 advanced implementation combining immutable source snapshots, compiler navigation, directed multiedges, source-bound retrieval, controlled changes and verification. The native default works without Graft, Serena, Graphify, Node, Python, a model or an API key. The [replacement matrix](docs/RUST_REPLACEMENT.md) states which workflows have native evidence and which migrations remain open.

## Native Rust start

```sh
cargo build --workspace --release --locked -j 2
./target/release/yoke --root /absolute/project index
./target/release/yoke --root /absolute/project context invoice
./target/release/yoke --root /absolute/project export --format html
./target/release/yoke --root /absolute/project serve
```

Use `target/release/yoke.exe` on Windows. Rust 1.94+ is required to build. The executable links 24 programming-language grammars, SQLite FTS5, graph algorithms including real Leiden, an independent LSP client and guarded changes with durable recovery. The HTML graph inspector works offline. Standard MCP tool names are shared, but advanced optional arguments and snapshot stores are distinct. [Native instructions](native/README.md) cover trusted semantic servers, context plans, verification, portable packaging and remaining limits. Actual Windows checks are recorded in [native validation](docs/validation/native-rust.json).

## TypeScript advanced runtime

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

The historical screenshot uses labeled frontend fixture responses. The native Rust offline export has separately been opened in the actual collaborative browser and exercised with real captured source, Unicode anchors, a mobile viewport and hostile embedded text.

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

Full migration to Rust and feature-for-feature parity with all upstream/commercial products remain open. The Rust path has actual Windows tests, live rust-analyzer/TypeScript-server checks, native Leiden, real PDF worker extraction, a live offline-export browser check and an actual VS Code 1.140 extension-host test. The older TypeScript validation report remains historical; its Node 24/Windows regression suite was rerun successfully in this update. Other language servers, proprietary services, neural models, Docker, hosted macOS/Linux CI and whole-agent savings remain unverified. [STATUS.md](docs/STATUS.md) records the exact boundaries.

## License

Own implementation: MIT. TypeScript runtime notices retained. Optional upstream applications, model weights and media libraries have their own licenses and are **not** bundled. No GPL Serena application code, paid JetBrains plugin or hosted service is included. External commands in copied trees retain process privileges unless a separately validated host/container sandbox is used.
