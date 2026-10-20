# Yoke native Rust core — 0.5.0

An independently implemented MIT core for local code orientation, semantic navigation, source graphs and reviewable changes. The default executable needs no Node/Python runtime, Graft, Serena, Graphify, model or API key. Standard third-party parsing/database/algorithm libraries are linked into the binary. Host-selected language servers are optional separate executables.

The optional [PostgreSQL catalog host adapter](docs/postgres-catalog.md) captures
fixed read-only metadata through an explicitly trusted/pinned psql18 installation
into immutable received-data artifacts. `postgres-catalog read` queries these
offline with no source Store, credential lookup or network. This bounded adapter
does not establish server truth, original DDL, full upstream or platform parity.

The optional [HTTP source host adapter](docs/source-acquisition.md) captures
explicit public text/raw-HTML/PDF responses into immutable body archives.
`source-acquisition read` replays exact ranges offline; explicit `--pdf-text`
uses the existing bounded native parser. The dedicated
[arXiv route](docs/arxiv-source.md) adds host-authorized metadata/PDF capture
with shared rate coordination and exact received-tag evidence. Article/semantic,
Google/social/video/database routes retain their separate capability boundaries.

## Build and use

Rust 1.94.1 and the Windows MSVC toolchain are exercised locally. Cargo requires Rust 1.94.1 or newer, including the native AWS signing dependencies. Older compilers and hosted macOS/Linux CI are not claimed tested here.

```sh
cargo build --release --locked -j 2
./target/release/yoke --root /path/to/project index
./target/release/yoke --root /path/to/project find helper
./target/release/yoke --root /path/to/project context helper
./target/release/yoke --root /path/to/project outline src/main.rs
./target/release/yoke --root /path/to/project trace helper --direction in --relation calls
./target/release/yoke --root /path/to/project analyze
./target/release/yoke --root /path/to/project export --format html
./target/release/yoke --root /path/to/project serve
```

On Windows, use `target/release/yoke.exe`. JSON is written to stdout; operational failures go to stderr with a nonzero exit status. `--snapshot ID` reads immutable history; otherwise saved source is strictly recaptured. Global `--timeout` is milliseconds and `--max-bytes` bounds tool envelopes. Large complete responses are immutable artifacts, paged with `ci_read`. `tokenBudget` is conservatively estimated as UTF-8 bytes / 3; impossible envelope budgets fail explicitly.

Analyzer 8 remains the default. Explicit host `--analyzer-epoch 9` selects the
additional structural rules for new captures and supported Git/session queries;
historical snapshots keep their proven policy. See [analyzer epochs](docs/analyzer-epochs.md)
and [session policy](docs/session-analyzer-epochs.md). This opt-in does not certify
complete language or language-server behavior.

Ordinary `ci_find`, `ci_read`, `ci_trace` and `ci_context` can explicitly select an
already published `graphViewId` to query code, document and semantic overlays
through the same eight tools. [Selected graph queries](docs/selected-graph-queries.md)
describe the strict scope, evidence and output budgets. Authored decisions and
personal preferences have separate immutable revision, Markdown exchange and
literal-check workflows; see [decision records](docs/decision-records.md) and
[their host CLI](docs/decision-cli.md). Personal content stays in the selected
personal store, while minimal dependency pins retain its source history.

The Rust library also provides [historical selected-View comparison](docs/historical-view-comparison.md).
It compares two explicitly pinned Views or checked Meaning archives with exact
source bindings, bounded output and visible structural ambiguity.

Separate [portable Meaning projection consumers](docs/meaning-projection.md)
read serialized group/join exports and perform bounded node, degree and directed
trace queries without a Store. Imported metadata remains untrusted; structural
validation does not provide source replay or semantic identity.

An existing Git report also supports [captured-hierarchy affected details](docs/git-affected-details.md)
through the Rust library and `git affected`, with separate historical source
bindings and structural candidate explanations.
[Captured impact areas](docs/git-areas.md) additionally classify saved impact
paths by explicitly selected stored concepts or exact directory spelling.
They preserve all claims, visible ties and the original quoted parent.
Separate [captured owner candidates](docs/git-owners.md) retain exact per-rule
and owner-token byte evidence for each base/head path. Explicit empty-owner
rules and unsupported/uncaptured cases remain visible without verified identity
or review authority.
Explicit [index conflict triage](docs/git-conflicts.md) observes the selected
worktree without changing its files or index. Git-stage source pages require
separate host permission and checked native captured-path scopes.

Explicit [captured-history acquisition](docs/digest-history-acquisition.md) and
[persistent local history jobs](docs/digest-history-jobs.md) capture and map
verified Git objects under the original head's disk scope. `digest-history-job`
supports creation, read-only status, exact-revision steps, bounded multi-batch
runs, pause/resume and cancellation. Host grants and cumulative limits remain
explicit; no signing key, provider/model call or source execution is introduced.

## Native capabilities

- AST structure in 24 programming languages: Rust, TypeScript, JavaScript, Python, Go, C, C++, C#, Java, Ruby, PHP, Swift, Scala, Lua, Bash, Elixir, Solidity, OCaml, Zig, Nix, R, Kotlin, Dart and Clojure. TSX/JSX and OCaml interfaces are additional grammar variants. Support is structural; per-language constructs and dynamic dispatch are not exhaustive.
- Exact UTF-8 source ranges, immutable SHA-256 blobs, strict double-scan capture, ignored/protected paths, obvious-secret exclusions, explicit binary/invalid-UTF-8 omissions, persistent parse reuse and parallel parsing.
- SQLite FTS symbols with identifier-component indexing, BM25 document-body retrieval, literal/regex source search, source-first context, file outlines, authored rationale and Markdown links, SQL table declarations, directed multiedges, bounded trace and shortest paths. A strictly validated context plan supports find/filter/references/read/select/group/limit over one immutable snapshot.
- Native PageRank, components, directed cycles and actual `leiden-rs` community detection with seed 42 on a labeled undirected weighted projection. Original directions and parallel observations are preserved. Community analysis is bounded to 10,000 entities / 100,000 observations and 10 iterations; synchronous native algorithms check deadlines cooperatively.
- Canonical JSON, directed GraphML and a standalone offline HTML inspector. GraphML labels unresolved placeholders explicitly. HTML renders untrusted fields as text, escapes embedded JSON, includes no external assets and refuses network connections. Exports do not overwrite different existing bytes.
- Independently implemented LSP definition, declaration, references, implementation, hover, document symbols and rename planning; capability checks, UTF-16 conversion, bounded framing, path/version/edit validation and copied-source integrity checks. Actual rust-analyzer and TypeScript definition/rename workflows have separate live tests.
- Eight MCP tools; strict declared fields, bounded UTF-8 error envelopes, recoverable request failures and immutable artifacts. The wire-compatible tool names remain `ci_status`, `ci_find`, `ci_read`, `ci_trace`, `ci_context`, `ci_change_prepare`, `ci_change_apply`, `ci_verify`. This does not imply identical optional arguments or all advanced TypeScript tools.
- Hash-bound explicit create/replace/delete and symbolic replace/insert/delete plans, unified diffs, derived snapshots, isolated application, explicit host approval, executed verification, OS writer locks, conditional original writes and durable recovery. Rollback preserves unrelated foreign writes.
- Service-scoped SQL, Prisma, OpenAPI, GraphQL, Protobuf and JSON Schema observations, plus literal route/request/event/environment candidates. Contract cross-links retain their uncertainty and exact declaration spans. See [contract extraction](docs/contracts.md).
- Source-bound authored knowledge, local sparse TF-IDF retrieval, imported finite external vectors, and validated LCOV/runtime/SARIF artifacts. Statistical scores rank candidates; they do not establish meaning or correctness. See [knowledge and imports](docs/knowledge.md).
- Versioned unsaved editor overlays, authenticated loopback HTTP, and a VS Code client that defaults to the native executable. Saved source remains untouched, disk conflicts and stale buffer updates fail, and project settings cannot select an executable. See [HTTP protocol](docs/editor-http.md).
- Explicit native PNG/JPEG/WAV/MP4 technical capture, source-bound supplied transcripts and actual PDF text extraction through a resource-limited native worker. Metadata remains distinct from OCR/ASR/scene interpretation. See [media](docs/media.md).
- Ed25519-signed source envelopes, host-selected receiving grants, replay protection, separate immutable remote namespaces and local graph rebuilding. Offline exchange and an optional TLS service/client share the same signed-source rules. Signing seeds, tokens and TLS private keys use native OS stores; no private bytes enter JSON profiles. See [federation](docs/federation.md), [transport](docs/federation-http.md) and [credentials](docs/credentials.md).
- Exclusive-client maintenance with dry runs, source/record/artifact reachability, prepared quarantine manifests and bounded recovery. It preserves every history root and does not reclaim physical disk space. See [maintenance](docs/maintenance.md).

Structural call targets are candidates, never compiler bindings. Bare package imports remain unresolved without a semantic resolver. A missing provider never becomes a complete empty success. LSP source-binding checks validate the supplied result against captured bytes; they do not establish exhaustive compiler coverage.

## Optional semantic servers and MCP

Write an explicitly selected host configuration outside an untrusted project's control:

```json
[{"language":"rust","command":"rust-analyzer","args":[]}]
```

```sh
yoke --root /project --language-server-config /trusted/servers.json --trust-servers serve
```

`ci_find` can then read definitions, declarations, references, implementations, hover or document symbols:

```json
{"snapshotId":"<immutable snapshot ID>","languageServer":{"operation":"references","path":"src/main.rs","offset":20},"timeoutMs":15000}
```

`offset` is an exact captured UTF-8 byte offset. The strict request cannot carry executable, trust, rename or ordinary search fields. See [semantic navigation](docs/semantic.md) for all operations and provider-attempt metrics. An actual rust-analyzer MCP test validates references against a historical snapshot after the original saved file changes.

`ci_change_prepare` can also accept `rename: {entityId, newName}`. It runs against the exact requested snapshot and prepares a plan. Repository contents and MCP arguments cannot register commands or grant trust. One-shot operations use `yoke lsp --command rust-analyzer --language rust --path src/main.rs --offset 20 --operation references --trust-server`.

Native semantic processes use Unix process groups / Windows jobs and bounded I/O. External servers still run with host privileges; this is process lifecycle management, not an OS sandbox. The Rust adapter disables rust-analyzer build-script/proc-macro execution and uses offline Cargo metadata settings, but host executables remain trusted.

## Guarded changes

Prepare with `ci_change_prepare` or `yoke prepare --input edits.json`. Review the returned diff and exact plan ID. The source tree is untouched. `yoke apply PLAN --key TASK` materializes a complete isolated result.

Original-tree integration requires all of:

```sh
yoke verify --plan PLAN --command rustc --arg=--crate-type=lib --arg=main.rs --trust-project
yoke approve PLAN                         # --allow-test-changes when appropriate
yoke --allow-writes apply PLAN --key TASK --mode workspace
yoke --allow-writes recover JOURNAL       # conditional rollback when needed
```

`ci_verify` checks syntax-diagnostic differences, including multiplicity. It cannot substitute for executed verification or authorize a workspace write. Plans expire after 24 hours; stale source and overlaps fail. Per-file writes and journals are durable, but a multi-file change is not atomically visible to external readers. State and immutable source copies in `.yoke` need the same privacy controls as source. SQLite database/sidecar links are refused; this is not a guarantee against a hostile process racing the filesystem with equal privileges.

An interrupted original-tree application returns `RECOVERY_REQUIRED` and `error.journalId`. This identifier remains available when the request deadline prevents updating the emergency journal: recovery also inspects files absent from `appliedPaths`. Workspace apply requires a minimum effective 1,024-byte response budget so recovery information can fit. Do not delete state when an unfinished journal exists.

Saved change plans replay their original identity, complete captured file delta,
exact review diff and test-change warnings before approval, new application,
verification or recovery. See [saved plan integrity](docs/plan-integrity.md)
for compatibility, finite limits and storage-corruption boundaries.

One complete captured text directory can also move through a matching
host-selected LSP folder operation. The plan retains empty directories, modes
and physical identities; copied verification, approved application and
conditional recovery share that declared tree. The [directory move contract](docs/directory-moves.md)
records finite limits, facade naming and unsupported multi-provider cases.

[Captured file usage checks](docs/file-usage.md) add default deletion planning
for bounded TypeScript/JavaScript modules of named functions. Exact provider
declarations, references and conservative captured occurrences are checked
before one ordinary guarded plan. Unknown consumers remain explicit; host
approval and executed verification still govern application.

[Symbol deletion boundaries](docs/symbol-deletion.md) bind TypeScript/JavaScript,
Python and Rust declarations to exact captured AST nodes before including their
owning export, decorator or outer-attribute bundle. Structural entity coordinates
stay unchanged. Unsupported
ownership refuses; ordinary usage and application gates still apply.

Native `deleteSymbol.emptyPythonBody: "pass"` explicitly fills an otherwise empty
required Python suite after exact declaration/decorator selection and result
syntax checking. The default remains rejection, with ordinary plan/application
gates intact. See the [finite contract](docs/semantic-refactor-intents.md#explicit-native-python-empty-body-deletion).

Native `deleteSymbol.siblingDeclarators: "preserve"` can separately remove one
checked TS/JS function-valued variable and its direct AST comma, preserving
other declarators and original bytes. Ambiguous comments and mixed operations
refuse. See [symbol deletion](docs/symbol-deletion.md).

[Captured function propagation](docs/propagated-delete.md) can remove checked
standalone call statements and newly unused private function dependencies in a
bounded TS/JS module. Surviving references, possible consumers and exported
dependencies remain protected; incomplete coverage stays explicit.

## Read-only context plans

Use `ci_context` with a query or a plan, never both. Plans contain 1–16 validated steps and cannot execute commands or generated programs. For example:

```json
{"plan":[{"op":"find","query":"invoice","limit":20},{"op":"filter","field":"kind","includes":"function"},{"op":"references"},{"op":"read"},{"op":"select","fields":["path","contentHash","startByte","endByte","text"]}]}
```

References retain their direction, uncertainty and exact evidence separately from selected source fields. Result limits are visible as `truncated`; whole large responses remain retrievable as artifacts. Large context bodies are source anchors with `bodyOmitted` and an explicit `ci_read` request instead of silently cut source.

## Source-bound knowledge and editor buffers

```sh
yoke --root /project remember --input finding.json
yoke --root /project recall "request validation"
yoke --root /project vectors build
yoke --root /project vectors search --model MODEL_FROM_BUILD --query "request validation"
yoke --root /project import sarif --input source-bound-report.json
yoke --root /project editor
```

Knowledge records are project assertions with source hashes and explicit staleness. They are distinct from agents' central AgentMemory service. `ci_context` includes them only with `includeMemory: true`; semantic search requires an explicit model identity. LCOV/runtime/SARIF inputs are strict JSON wrappers documented in [knowledge](docs/knowledge.md), rather than unscoped raw reports.

Explicit [outcome feedback](docs/outcome-feedback.md) binds authored results to original native Views and exact source or imported transcript quotes. Ranking exposes sequence selection, integer age decay and stale-source exclusions; literal transcript mining returns candidates for explicit host selection. Assertions and ranking scores never establish observed execution or independent corroboration.

`feedback plan-record` and `plan-rank` separately bind authored historical outcomes
to replayed saved change plans and their exact base/planned-result source bytes.
The original knowledge-event format stays unchanged. Planned results and scores
grant no current applicability, application, verification or execution authority.

The editor command prints one loopback endpoint and a random per-process bearer token. Keep this token private. The API exposes saved/unsaved capture and read tools; it cannot grant source writes or select provider executables. Buffer text and immutable history are retained in local state. A Rust VSIX can be built from a matching native archive with `python scripts/package-vscode.py --native-archive ARCHIVE --output PACKAGE.vsix`.

`ci_read artifactId` and MCP resources expose registered published artifacts or ordinary local-snapshot source hashes. Unpublished and remote-only raw blobs are unavailable through those generic paths. Artifact pages are snapshot-neutral; authoritative scope remains in the original artifact or explicit source query. Binary paging requires `encoding: "base64"` and advances byte cursors without dropping bytes.

## Media, signed exchange and maintenance

```sh
yoke --root /project media inspect docs/spec.pdf
yoke --root /project media transcript audio/sample.wav --input supplied-transcript.json
yoke federation keygen project-signing
yoke --root /sender federation export --scope trusted-scope.json --key-name project-signing --output signed-source.json
yoke --root /receiver federation import --input signed-source.json --grant trusted-grant.json --trust-grant
yoke --root /receiver federation find REMOTE_ID invoice --grant trusted-grant.json --trust-grant
yoke --root /project maintenance collect
yoke --root /project maintenance collect --apply
```

Signing/reading credentials is an explicit host facility using Windows Credential Manager, macOS Keychain or Linux Secret Service. Unavailable/locked native services fail without plaintext fallback. Receiving grants include exact tenant/project/sender/key and sender-workspace identities plus receiver deny globs. Read the strict JSON formats in the linked guides. Offline federation supplies the signed transfer format; [the optional TLS service/client](docs/federation-http.md) adds grant/role checks and revocation. Hosted user administration and hostile-host isolation remain separate.

Maintenance refuses while another native Store is open. Its default 24-hour age rule applies only to demonstrably unreferenced CAS blobs; `--apply` moves them to recoverable quarantine. Published responses, source history, plans, journals, editor sessions, knowledge/vectors and media remain roots. Separate [reviewed retention](docs/retention.md) supports exact confirmed history-retirement and quarantine-purge plans. Ordinary maintenance never implicitly authorizes those deletions.

## Verification and measurements

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --test live actual_rust_analyzer_definition_references_and_rename -- --ignored
cargo test --test live actual_rust_analyzer_mcp_references_bind_immutable_snapshot -- --ignored
# Set YOKE_TS_LSP to the installed typescript-language-server CLI for its live gate.
cargo test --test live actual_typescript_server_cross_file_definition_and_rename -- --ignored
cargo test --locked --test credentials -- --ignored # actual Windows OS-key-store gates
pwsh -File scripts/test-vscode.ps1                  # installed VS Code, isolated profile
python -m unittest discover -s scripts/tests -v    # labeled offline archive/license fixtures
cargo run --release --example benchmark -- 500
python scripts/package-native.py --skip-build --fetch-missing-licenses
npm run typecheck
npm test
```

Node and Python are development-test prerequisites for deliberately labeled protocol fixtures and process lifecycle regressions; the default Rust executable does not invoke them. Explicit live tests fail when their real tools are unavailable. Hosted CI definitions are not evidence that hosted CI ran. The synthetic microbenchmark does not establish whole-agent savings or relative upstream performance.

The packaging script supplies a portable executable, own MIT license, locked dependency inventory and full dependency license texts. `--fetch-missing-licenses` uses the GitHub CLI to obtain texts missing from crate archives at their exact recorded upstream revision; default packaging makes no such requests and fails on absent notices. The archive contains no captured project or state. Building/packaging requires development tools; running its default core does not. See [native validation](../docs/validation/native-workflows.json) for executed checks and measurements.

After extracting a native VSIX into an isolated directory, the actual Windows extension-host gate can exercise its bundled executable with `pwsh -File scripts/test-vscode.ps1 -ExtensionDirectory /extracted/extension -UseBundledRuntime`. This leaves the user's installed extensions and profile untouched. Offline Unix archive fixtures verify executable permission metadata; they do not establish a live Linux/macOS extension-host result.

## Advanced local workflows

Rust now owns configured TypeScript/JavaScript analysis and virtual solution builds, intraprocedural dataflow candidates, per-test coverage/selection, executable model providers with durable accounting, a local Ollama adapter, persistent Python debugging/REPL, authenticated signed TLS source exchange, reviewed history retirement/quarantine purge and digest-pinned Docker verification. See [workflow commands](docs/workflows.md) and their individual contracts. These operations require explicit host authority; default indexing remains independent of external runtimes and models.

The independently written compiler/debugger workers use the actual selected TypeScript compiler or Python runtime; they do not depend on Graft, Serena or Graphify. Private credentials use native OS stores, with unavailable or locked services failing explicitly. Linux/macOS code and CI definitions are supplied; this Windows session does not certify those OS stores. The retained TypeScript 0.3 implementation has separate state and remains available. Advanced IDE refactorings, arbitrary language servers, proprietary hosted services and exhaustive correctness are outside the measured evidence. See [the acceptance matrix](../docs/RUST_REPLACEMENT.md) and [status](../docs/STATUS.md).
