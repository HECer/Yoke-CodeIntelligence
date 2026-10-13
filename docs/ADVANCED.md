# Version 0.3 capabilities and operating guide

These are actual implemented CLI/library entrypoints. An optional provider is not bundled and an unexecuted live compatibility gate is not a success. See `STATUS.md` and `validation/report.json`.

## Saved files, declarations and TypeScript builds

Installed `node_modules` package metadata and TypeScript declaration files (`.d.ts`, `.d.mts`, `.d.cts`) are captured as immutable **read-only** inputs. Package JavaScript, build plugins and installation scripts are never executed by native indexing. `dependencyMode: "none"` disables this feature. Symlinked package stores (including some pnpm/workspace layouts) remain excluded, with omissions; remote types are never fetched automatically. Build configurations outside the captured scope remain unavailable.

```sh
node dist/cli.js index --root /project --timeout 120000
node dist/cli.js build --root /project --timeout 120000
node dist/cli.js verify --root /project --build --timeout 120000
```

`build` uses TypeScript's solution builder on an in-memory filesystem, including generated declaration output for project references. Source and build output directories on disk remain unchanged. This covers ordinary configured TypeScript builds; it is not a replacement for framework bundlers, custom compiler plugins or arbitrary `npm run build` commands. Explicit trusted verifier profiles cover those commands separately.

Strict capture remains the default. `index --mode metadata` reuses unchanged file metadata and hashes; it is a weaker freshness claim and may **not** authorize source writes. Semantic per-program cache keys include configured inputs/reference/dependency hashes. Changed imports, config and type dependencies invalidate them. Language programs may still be initialized and query preparation has costs; this is not a claim of zero work on every warm request.

`watch --mode metadata` uses debounced filesystem hints and a periodic reconciliation. `--gc-every 3600` opts into hourly retention while watching. By default nothing is automatically deleted. Retention defers if another store client is live.

## Unsaved editor buffers and VS Code

The included VSIX contains the compiled runtime but requires an external **Node.js 22.16+** executable. Install it using VS Code's “Install from VSIX” action, then run **Code Intelligence: Start Local Editor Bridge** and **Find Context (Includes Unsaved Buffers)**. Set `codeIntelligence.nodeExecutable` in user settings when `node` is not on PATH. Workspace-provided executable settings are ignored. Untrusted/virtual workspaces are refused.

The bridge has actually been tested over HTTP, including unsaved search and disk non-modification. The VSIX structure is checked; an actual VS Code extension host was unavailable here.

```sh
node dist/cli.js editor --root /project --timeout 120000
```

Startup prints `{endpoint, token}`. Treat that token as a local privilege, never commit it. `POST /editor` requires `Authorization: Bearer <token>`. `open` returns a session ID; `buffer` accepts `{path,version,text}`; `capture` produces an immutable overlay; `tool` exposes only five read tools; `closeFile`/`close` release overlays. Versions are monotonic. Conflicting independent disk changes are rejected, not merged silently. The editor transport cannot apply source writes.

## Python, Go and external semantic servers

```sh
node dist/cli.js semantic python definition src/example.py --byte 50 --trust-providers --root /project --timeout 120000
node dist/cli.js semantic python rename src/example.py --byte 50 --new-name improved_name --trust-providers --root /project --timeout 120000
node dist/cli.js semantic go references main.go --byte 90 --trust-providers --root /project --timeout 120000
node dist/cli.js lsp definition demo.cpp --byte 56 --command clangd --arg=--log=error --trust-providers --root /project --timeout 120000
```

Offsets are **UTF-8 bytes**, not character counts. Python uses separately installed Jedi; Go compiles the bundled worker with the installed Go toolchain, uses `go/types`, captured local modules and GOROOT, and disables module/network downloads. Missing dependencies block Go rename. Renames produce reviewable plans, never immediate source changes. Python dynamic behavior, Go reflection, build tags for other platforms and downstream clients outside the snapshot are not exhaustively resolved. Real small-fixture Jedi/Go/clangd tests passed; this is not universal language correctness.

## Serena's advertised tools

```sh
node dist/cli.js serena capabilities --command serena --arg=start-mcp-server --arg=--project --arg='{workspace}' --trust-providers --root /project
node dist/cli.js serena read --command serena --arg=start-mcp-server --arg=--project --arg='{workspace}' --input read-operation.json --trust-providers --root /project
```

Example arguments must match the installed version's CLI. `read-operation.json` is `{"tool":"find_symbol","arguments":{"name_path_pattern":"Example","relative_path":"src/example.py"}}`. Use `prepare` for supported edit tools. The facade discovers actual advertised schemas, preserves grouped payloads, validates source changes and prepares a reviewed patch. It does not guess that every version exposes every tool. Unsupported REPL/debug/runtime operations require additional **`--allow-runtime`**, meaning explicit arbitrary code permission in a copied tree, **not an OS sandbox**. Full JetBrains operation requires the user's compatible IDE/plugin/license. No GPL Serena application source or paid plugin is distributed here. Live Serena itself remains untested in this environment.

## Retrieval, meaning and communities

```sh
node dist/cli.js embed --root /project
node dist/cli.js semantic-search 'invoice calculation' --model <returned-model-id> --snapshot <returned-snapshot-id> --root /project
node dist/cli.js embed --host-config /trusted/provider.json --trust-providers --root /project
node dist/cli.js rerank --host-config /trusted/provider.json --input candidates.json --trust-providers --root /project
node dist/cli.js summarize --host-config /trusted/provider.json --input candidates.json --trust-providers --root /project
node dist/cli.js communities --algorithm louvain --root /project
node dist/cli.js communities --algorithm leiden --root /project
```

Without `--host-config`, vectors are local **hashed TF-IDF lexical embeddings**, not a pretrained neural semantic or multilingual model. With an explicit provider, generation, query embedding, cross-encoder-style reranking and source-bound summaries use a bounded process protocol and cost ledger. `candidates.json` contains `entityIds` and, for reranking, `query`. No API credentials or model weights are included. Review `docs/examples/model-provider.json` and `workers/sentence_provider.py`: it requires an already downloaded safetensors-only model directory, `sentence-transformers`, offline loading and `trust_remote_code=False`. `CI_MODEL_PATH` selects the local directory. The host model identity must match responses. Model-provided IDs are validated; summaries are always **inferred**, never established program facts. Unknown billing remains reserved; malformed output is not “free”.

`envFrom` in host model config maps child variable names to existing host environment variable names. Never put actual credentials into tracked examples. The generic model protocol also supports a separately implemented/reviewed summarization or media-description process; the bundled sentence worker is embeddings/reranking only.

Louvain uses NetworkX. Leiden uses the official `igraph`/`leidenalg` libraries and fails explicitly when unavailable. There is **no mislabeled Louvain fallback**. A separate undirected weighted projection is used only for clustering; original directed multiedges and evidence remain intact. Leiden is implemented as an adapter but was not executed here.

## Frameworks, test maps and dataflow

Native indexing now extracts additional Prisma models/fields/mappings, SQL table operations/migrations, literal HTTP clients, JSON OpenAPI operations, GraphQL schema fields and Protobuf headers. Name-based links are service-scoped **candidates**, not runtime-verified contracts. Selected OpenAPI compatibility changes can be checked with `contract-diff --input file.json`, containing `before` and `after` objects. Complex `$ref` expansion, all type variance, every framework and arbitrary string-built routes are outside this finite extractor.

```sh
node dist/cli.js taint --input docs/examples/taint-policy.json --root /project --timeout 120000
node dist/cli.js coverage-run --input test-selections.json --trust-project --root /project --timeout 120000
node dist/cli.js test-select --base <coverage-snapshot> --snapshot <changed-snapshot> --input known-tests.json --root /project
```

`coverage-run` executes JavaScript Node tests separately, by file or an explicit name pattern, and imports exact hash-bound V8 line coverage. It uses **per-file structured runner summaries**, not the potentially misleading top-level passing file wrapper. Zero matching cases do not pass. Named selection may include multiple matching subtests; it is not guaranteed a single case unless the pattern is unique. Source mutation makes a job fail. Final release gates should use the full suite. Other runners can supply `coverage-tests --input JSON` using the documented `TestObservation` contract. TypeScript source maps and arbitrary language test runners are not guessed.

`taint` performs bounded intraprocedural TS/JS flow with compiler-bound locals, assignments, branch joins, loop bounds and explicit host source/sink/sanitizer assumptions. Heap aliasing, interprocedural behavior and reflection are not comprehensively modeled. Findings are candidates; no findings is **not** a security certificate or formal proof.

## Media

```sh
node dist/cli.js media guide.pdf --root /project --expected-hash <sha256>
node dist/cli.js media recording.wav --input supplied-transcript.json --root /project
```

Native extraction covers PDF text/page anchors via optional PyMuPDF, image metadata via Pillow, WAV metadata, ffprobe metadata for other media, and text/subtitles. PDF scans require an explicit OCR/model provider; video/image scene understanding and audio speech transcription require an explicit supplied transcript or reviewed `describe_media` provider. No transcript or image meaning is invented from metadata. Imports make an immutable derived text representation and retain the binary content hash; source files remain untouched. Optional provider labels stay inferred/unverified. Large binaries and malformed media are bounded/refused. External media libraries have their own licenses and are not bundled.

## Storage and process containment

`gc` previews retention; `gc --mode apply` quarantines unreachable blobs before a later purge. Pins, current/recent snapshots, active editor sessions, valid plans, recovery journals, memories and audit artifacts retain their dependencies. Audit retention can deliberately keep large histories. Close all other clients before applying maintenance. `pin --key release-1` and `unpin --key release-1` control explicit roots.

Verification profiles can include `sandbox:{kind:"docker",image:"node@sha256:<reviewed digest>"}`. Docker must already be installed and that exact image present. The runner denies network, extra privileges and image pulls, limits resources and removes the named container even after client termination. Unconfirmed cleanup fails closed. **Docker execution was not available for live testing here.** Without a sandbox profile, trusted external commands run in a copied directory and still have host-process privileges.

## Signed multi-project and multi-device exchange

See `FEDERATION.md`. This is authenticated immutable **source snapshot** transfer and federated retrieval, not copying a live SQLite WAL database, automatic checkout, full user/account management or merging arbitrary editor histories.
