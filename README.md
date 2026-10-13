# Code Intelligence

**Local code understanding with evidence, immutable snapshots and reviewable changes.**

[Deutsch](README.de.md) · [Quick start](docs/QUICKSTART.md) · [Architecture](docs/ARCHITECTURE.md) · [API](docs/API.md) · [Release status](docs/STATUS.md) · [Security](SECURITY.md)

Code Intelligence gives coding agents one small interface for finding code, reading exact source, following directed relationships, preparing patches and verifying changes. Native indexing and retrieval require **no model, API key or cloud account**.

**Version 0.1.0 is an initial tested implementation, not full feature parity with every upstream or the entire long-term specification.** Native TypeScript, Python and Go paths were exercised locally. External Graft, Graphify, LSP and Serena adapters have contract/protocol tests; live upstream compatibility still needs validation. See [the exact capability matrix](docs/STATUS.md).

![Read-only local explorer, rendered against the included invoice UI fixture](docs/screenshots/explorer.png)

*The screenshot uses the real frontend with an offline invoice fixture. Live browser-to-loopback navigation was blocked by the build environment's managed-browser policy; the HTTP service is tested independently.*

## What works

| Area | Included implementation |
|---|---|
| Source integrity | Immutable content-addressed snapshots, explicit omissions, UTF-8 byte anchors and hash-bound reads |
| Native language analysis | TypeScript/JavaScript compiler navigation and rename; Python AST; Go parser |
| Knowledge graph | Directed multirelations, separate provenance and resolution, unresolved targets kept explicit |
| Retrieval | SQLite FTS5, literal source search, scope filters, compact context, bounded read-only operation plans |
| Changes | Preview, full patch, isolated materialization, optional approved workspace application, durable recovery and conflict preservation |
| Verification | New-versus-baseline diagnostics, explicit trusted command profiles, logs and source-integrity checks |
| Additional knowledge | Source-bound memories, precomputed vector search, LCOV/runtime/SARIF imports, route/schema/document hints |
| Interfaces | CLI, eight-tool MCP stdio server, read-only loopback explorer, library API |
| Optional integrations | Version-checked Graft/Graphify imports and process bridges; independent LSP transport; external Serena MCP symbol adapter |

A static reference is not a call; a possible dispatch is not observed execution. The graph records these distinctions instead of converting all relationships into equally confident edges.

## Start from source

Prerequisites: Node.js **22.16+**, npm and Git. Python **3.10+** enables Python AST analysis; an installed Go toolchain enables Go analysis. No project source is imported or executed by native indexing.

```sh
npm ci
npm run build
node dist/cli.js index --root /path/to/project --timeout 120000
node dist/cli.js find calculateInvoice --root /path/to/project
node dist/cli.js view --root /path/to/project --timeout 120000
```

Use the local URL printed by `view`; its fragment contains a temporary access token. Do not share that URL. The explorer cannot apply changes.

The supplied **portable runtime archive** already contains `dist`, workers and the pinned TypeScript runtime with its license. Extract it and run `node dist/cli.js --help`; it does not need `npm ci` to run. It does not bundle Node, Python, Go or external upstreams.

No npm package publication is claimed. Do not assume `npm install @hecer/code-intelligence` is available.

## Connect an agent with MCP

Configure your client's stdio server with absolute paths:

```json
{
  "mcpServers": {
    "code-intelligence": {
      "command": "node",
      "args": ["/absolute/Code-Intelligence/dist/cli.js", "serve", "--root", "/absolute/project", "--timeout", "120000"]
    }
  }
}
```

On Windows, forward slashes in JSON paths avoid backslash escaping. Do not add `--allow-writes` or `--trust-project` until the workspace and its command profiles have been reviewed. Client configuration envelopes vary; the executable and argument list are the portable part.

The eight tools are `ci_status`, `ci_find`, `ci_read`, `ci_trace`, `ci_context`, `ci_change_prepare`, `ci_change_apply` and `ci_verify`. The ordinary path is **find → read → prepare → verify → isolated apply**. Workspace integration additionally requires host permission and explicit plan approval. See [API and change examples](docs/API.md).

## Validate

```sh
npm run typecheck
npm test
npm run test:coverage
npm run bench
```

The committed [validation report](docs/validation/report.json) states the actual local environment, tests, browser limitations and unperformed checks. CI workflows are supplied for three operating systems; their presence is **not** evidence that hosted CI has run.

The microbenchmark is a small generated fixture with explicit-snapshot warm queries. It is not an agent benchmark, a production latency guarantee or evidence of a percentage reduction in billed tokens.

## Publish under HECer

The intended public repository is `HECer/Code-Intelligence`. At delivery, repository creation from this session was blocked: the available GitHub connector could write files in existing repositories but could not create a repository, and the target returned 404.

After extracting the source archive **including its `.git` directory**, install GitHub CLI and authenticate locally:

```sh
gh auth login
npm run publish:github
```

Alternatively, `node scripts/publish-github.mjs` needs no npm dependencies. The script verifies the account is `HECer`, creates the public repository if missing, refuses private/unrelated targets, never force-pushes, and verifies the final remote commit. It does not ask for or print tokens. Preview with `npm run publish:github -- --dry-run`.

## Design and boundaries

This is an independently written MIT-licensed core inspired by the complementary capabilities of Graft, Serena/SolidLSP and Graphify. It does not bundle those applications or claim their commercial/cloud features. Native compiler/parser providers make the default path runnable without installing three overlapping servers. Optional adapters can enrich the same evidence model.

The copied edit/verification trees are **not OS security sandboxes**. Approved external commands may execute code and access the network. A snapshot captures saved files; it does not know unsaved editor buffers. Fresh disk capture remains proportional to the source bytes read. There is no multi-machine shared SQLite service, formal verification, embedded embedding model or universal language support in this release.

See [Security](SECURITY.md), [release gaps](docs/STATUS.md), and [third-party notices](THIRD_PARTY_NOTICES.md). License: [MIT](LICENSE).
