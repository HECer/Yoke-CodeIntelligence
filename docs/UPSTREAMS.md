# Optional upstream integrations

Version pins and primary-source references are in ../UPSTREAMS.json. The default native engine works without these applications. The earlier offline environment could not run these applications. The current Windows checkpoint below records real pinned Graft/Graphify structural gates; broader upstream behavior and other installations still need their own validation.

## Graft

Install @nanonets/graft 0.21.1 as a separately reviewed dependency accessible from this project's worker module resolution. Do not assume a global npm install is resolvable by Node ESM from another project. A deliberate local installation for testing is `npm install --no-save --package-lock=false @nanonets/graft@0.21.1`; review its dependencies/install scripts first. Do not accidentally commit that modified environment.

Index a project natively, retain its snapshot ID, then:

```sh
node dist/cli.js bridge graft --root /project --snapshot <actual-id> --trust-providers --timeout 120000
```

The worker checks the exact package version, materializes source, invokes the public Graft.graph API with llm=false/lsp=false, and imports the returned graph with hash binding. No meaning-layer enrichment is requested. Some upstream side effects may still happen within the copied tree; inspect its behavior independently.

## Graphify

Install graphifyy==0.9.87 into an explicitly trusted Python environment after reviewing the provider. Invoke the bridge with that interpreter:

```sh
node dist/cli.js bridge graphify --root /project --snapshot <actual-id> --trust-providers --python /provider-venv/bin/python --timeout 120000
```

The worker calls raw `extract(paths, root=...)` before a potentially lossy simple-graph projection. Supported confidence tags and direction are preserved; unsupported formats fail with ADAPTER_CONTRACT. Source hashes and per-node bounds must match the selected snapshot. Full semantic/media extraction and Graphify Cloud are not exposed by this bridge.

## Import an existing export

Use `import graft` or `import graphify` with `--snapshot` and `--input` pointing to a JSON wrapper containing snapshotId, sourceHashes, version and graph. Graphify raw extraction must be explicitly identified. The exact strict wrapper is in src/adapters/graph-import.ts and test/adapters.test.mjs. Never mark an arbitrary file as fresh merely because its name resembles a current source path.

## LSP and Serena

The independently written LspAdapter and SerenaAdapter are exported library adapters, not automatically discovered language servers. Start only explicitly trusted installed executables with reviewed arguments. Protocol transport and conversion tests use a deliberately labeled fake peer. Real server compatibility and capability coverage must be established separately. No JetBrains functionality, SolidLSP binaries or Serena GPL application code is bundled.

External subprocesses are not network-isolated by this project. They receive a reduced environment, but host sandboxing is required for untrusted binaries. Do not run provider package installation on an untrusted repository's recommendation alone.

### 0.2.0 LSP command

An already installed server can now be called through `lsp definition|references|implementation|rename`, with explicit `--command`, `--byte`, and `--trust-providers`. `rename` only prepares a plan. See [the configured-project/LSP guide](CONFIGURED-PROJECTS.md) for examples, returned evidence, incomplete dependency coverage and the non-sandbox boundary. Adversarial subprocess and CLI tests do not establish live third-party server compatibility.

## 0.3 compatibility update

The native optional **Jedi 0.20.0**, **Go/go-types 1.23.2**, **clangd 17**, **NetworkX 3.6.1**, **PyMuPDF 1.26.7**, and **Pillow 12.3.0** paths now have actual installed-provider tests on Linux. The earlier blanket “no real external server run” statement above describes the historical 0.2 LSP milestone; see the current report for the 0.3 results.

Graft/Graphify structural bridges verify every captured source file under the same ordinary-file/read-only-dependency policy used for materialization. Graphify's host-owned temporary cache is separate from its explicit scan root; the parent checks allocated cleanup paths and attempts both cache and materialization cleanup on failure. Windows cache paths use their namespaced spelling. Provider cache data grants no source identity or authority. Serena has a broader schema-discovered selected-tool gateway, including guarded edit preparation and explicitly authorized runtime commands; it still has no real installed Serena test here.

### Installed structural checkpoint — 2026-10-19

Actual Graft 0.21.1 and Graphify 0.9.87 structural tests pass on Windows with
Node 24.21.0/Python 3.14.7, non-BMP UTF-8, CRLF and exact original byte anchors.
The two original Python files remain unchanged. Six separately labeled protocol
cases exercise cache/root separation, relative-cache refusal, spawn-failure
cleanup and mutation of source/dependency files omitted by provider output.
The default suite passes 260 tests with three existing skips on an actual 8.3
temporary parent. These scoped results do not certify every language, raw export,
semantic/model/media feature or hosted product.

Dependencies were installed in a separate validation environment with lifecycle
scripts initially disabled. Only the reviewed pinned Kotlin grammar build was
approved. MSBuild FileTracker failed on this host; the three original binding
sources were compiled directly with installed MSVC and matching Node headers.
Published Graft payloads and Graphify Python sources match their verified
archives; no upstream application code was patched. The dependency lock,
installed versions, binding hash, red attempts and exact source freezes are in
[the preserved evidence](validation/integration-2026-10-19/upstream-compatibility/index.json).
An npm rebuild exit alone is insufficient when lifecycle scripts remain blocked.
No assistant hooks or model enrichment were requested, and network isolation
remains outside the bridge contract.

The own source-bound `summarize`/media pipeline provides optional meaning-layer integration independently of the structural bridge; it is not a claim that every native Graft/Graphify enrichment option is re-exported. `ADVANCED.md` explains model protocol, scope and host trust. Hosted Graphify Cloud, paid IDE backends, model weights and provider packages are separate products/dependencies.
