# Optional upstream integrations

Version pins and primary-source references are in ../UPSTREAMS.json. The default native engine works without these applications. Live upstream runs were not possible in the offline build environment, so treat the bridges as opt-in compatibility candidates until tested against your installed versions.

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
