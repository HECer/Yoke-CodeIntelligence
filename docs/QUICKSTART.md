# Quick start

## Native Rust executable

Build with Rust 1.94.1 or newer: `cargo build --workspace --release --locked -j 2`. Use `target/release/yoke.exe` on Windows or `target/release/yoke` on Unix. A portable archive can be built with `python scripts/package-native.py --skip-build --fetch-missing-licenses`; its default runtime requires no Node, Python or upstream application.

```sh
./target/release/yoke --root examples/demo index
./target/release/yoke --root examples/demo find invoice
./target/release/yoke --root examples/demo read src/invoice.ts
./target/release/yoke --root examples/demo context invoice
./target/release/yoke --root examples/demo export --format html
./target/release/yoke --root examples/demo serve
```

The Rust state directory is `.yoke`; it contains immutable source copies and recovery journals. It is separate from TypeScript `.code-intelligence` state. Snapshot IDs are runtime-specific. Review [native guidance](../native/README.md) before configuring semantic servers or applying changes. Advanced TypeScript operations below remain available during migration.

For the native Windows distribution, unpack the portable ZIP and run `yoke.exe --help`, or install the matching `win32-x64.vsix` with `code --install-extension PACKAGE.vsix`. The native VSIX includes the executable and defaults to Rust; no Node installation is required for its runtime. VS Code itself supplies the JavaScript extension host. Semantic servers remain separate host-selected executables. The package license inventory and [validation record](validation/native-rust.json) describe the exact tested build.

## Source checkout or supplied source archive

Use Node 22.16+ and Git. Run `npm ci`, `npm run build`, then `node dist/cli.js --help`.
The lockfile pins TypeScript 5.8.3, @types/node 25.1.0 and undici-types 7.16.0. The two development-only package integrities were unavailable in the offline build environment; versions and tarball URLs are pinned. The runtime TypeScript integrity was verified against npm metadata. Lockfile consistency was checked offline. A clean online `npm ci` was attempted for v0.3.0 and failed on registry DNS resolution (`EAI_AGAIN`); local build and tests used the exact available dependencies restored offline.

On systems where Python is not called `python3` (or `python` on Windows), supply an explicit host setting:

```sh
node dist/cli.js index --root /project --python /absolute/path/to/python --timeout 120000
```

A repository file cannot silently choose another executable. For a deliberate alternative provider environment, `--python` is also used by the Graphify bridge.

## Minimal workspace

The included `examples/demo` is a small non-secret TypeScript invoice project.

```sh
node dist/cli.js index --root examples/demo --timeout 120000
node dist/cli.js find invoiceTotal --root examples/demo
node dist/cli.js read src/invoice.ts --root examples/demo
node dist/cli.js analyze --root examples/demo
node dist/cli.js context "invoice tax" --root examples/demo
node dist/cli.js view --root examples/demo --timeout 120000
```

The state lives in the target project's `.code-intelligence` directory, never in a cloud service. It contains source copies: protect it like the project itself and keep it out of version control. The service excludes it from its own snapshots. Delete it only while all processes are stopped; doing so discards snapshots, approvals, journals and memories.

Copy a returned `snapshotId` into `--snapshot <id>` to issue several queries against exactly the same immutable state. Omit it when you intentionally want a new saved-file capture. A status query reports known state and does not prove the disk is unchanged.

## Result sizes

The default envelope cap is 32,768 bytes. `--max-bytes` may lower it. To raise the host cap, set `maxResultBytes` in `code-intelligence.json`; a per-request option cannot raise the host maximum. Truncated collections retain whole records and provide an immutable artifact/cursor for the complete result. `ci_read` follows that cursor.

`tokenBudget` is an estimate (`UTF-8 bytes / 3`), **not a model tokenizer**. The envelope byte cap excludes JSON-RPC transport escaping and does not account for the agent's surrounding prompts.

## Testing

`npm test` builds TypeScript and runs Node's real tests. Python and Go tests require their local toolchains; symlink tests are conditionally skipped on Windows. `npm run test:coverage` uses Node's experimental coverage reporting. `npm run bench` measures only the supplied synthetic fixture.

Optional UI checks require Python Playwright and Chromium. `python scripts/browser-smoke.py --chromium /path/to/chromium --save` uses the live loopback UI. `--offline-fixture` explicitly substitutes in-page test responses and a history API shim; it is not a live UI/API integration test.

## Configured TypeScript example

`examples/monorepo` contains an inherited base config, a solution config and two referenced packages. Index it, search `subtotal`, then verify it as shown in its README. Use [configured-project guidance](CONFIGURED-PROJECTS.md) for context filtering and opt-in installed LSP servers. Upgrading changes the analysis version in newly captured snapshots; re-index after upgrading rather than assuming an old explicit snapshot was rebuilt.

## Earlier TypeScript runtime archive

The runtime archive includes compiled JavaScript and the TypeScript runtime package. It does not include Node.js itself. Run `node dist/cli.js --help`, `node scripts/configured-smoke.mjs` or `node scripts/run-tests.mjs` directly; `npm test` is the source-build entry point and needs development dependencies. Additional native semantic/media providers require the documented local Python/Go tools.

## Advanced v0.3.0 workflows

See [advanced operations](ADVANCED.md) for unsaved editor overlays, optional semantic providers, in-memory solution builds, retention, contracts, per-selection coverage and local model adapters. See [federation](FEDERATION.md) for the TypeScript signed service. The earlier TypeScript 0.3 VSIX required an externally installed Node 22.16+ executable and lacked a live extension-host gate. The native 0.4 distribution has a separate actual VS Code host test and bundles its Rust executable; the native source-exchange guide is [federation](../native/docs/federation.md).
