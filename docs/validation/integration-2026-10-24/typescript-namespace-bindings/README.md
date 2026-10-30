# TypeScript namespace binding diagnostic

The classic function-body worker previously resolved every free identifier using
the union of value, type and namespace meanings. A caller's unrelated binding in
another TypeScript namespace therefore caused a false refusal. The worker now
uses the actual reference meaning from the selected TypeScript 5.8.3 compiler.
The pinned internal classifier must exist and return a known nonzero meaning;
otherwise preparation refuses. No source recipe, protocol field, Rust admission
rule, application permission or process budget changes.

`probe.cjs` assembles the ten actual worker components in the order declared by
`native/src/projects.rs`, without patching worker diagnostics. It uses the actual
installed TypeScript 5.8.3 module and Node, a 512 MiB worker heap, 30 second child
timeout and 32 MB capture bound. This is a diagnostic for trusted compiler code,
not an OS sandbox or process-tree containment certification. Only the literal
fixtures embedded in this probe are transpiled and executed, in a VM with a
one second timeout. No analyzed repository module is executed.

The final probe contains eleven fixtures, each with definition removal and
retention: **22 direct worker cases**. Six valid cases cover a return type under
a value parameter, a value under a type alias, a qualified type root, `typeof`
under a type alias, shorthand properties, and non-ASCII identifiers with an
astral comment and CRLF byte coordinates. Five adversarial cases cover actual
type, value, `typeof`, namespace and shorthand shadows.

- At parent `c1366e8d24e02732059d5eae11b71a75789faca2`, all 22 refuse with the
  original free-binding error: twelve false refusals and ten expected refusals.
- With the changed worker, twelve accept and ten refuse as expected. For every
  accepted result, the probe checks request/compiler/source identities, exact
  UTF-8 byte edits and result hash, absence of before/after compiler errors,
  actual copied-reference targets, unchanged `result` types and equal owned
  runtime observations. Both keep-definition modes are checked.

See `baseline.json`, `worker.json` and `checks.json` for actual results and source
hashes. Counts belong to this probe and are not native test function counts.
The new native integration tests exercise both analyzer epochs, immutable source
preservation, refusal without publication, and the existing compiler/Rust path.
**They were not compiled or executed locally**: the available C: space remains
below the established 1.2 GB native build admission. No older Yoke executable
was substituted. Native/platform acceptance remains pending in CI.

`npm run typecheck`, `npm test`, Node syntax checking and changed Rust formatting
are recorded separately in `checks.json`. This narrow correction does not close
R1, universal semantic parity, the current native matrix, Windows history/VSIX
live acceptance, the main merge or release.

Reproduce from the repository root with Node, choosing fresh output directories:

```text
node --max-old-space-size=256 docs/validation/integration-2026-10-24/typescript-namespace-bindings/probe.cjs c1366e8d24e02732059d5eae11b71a75789faca2 validation/namespace-baseline
node --max-old-space-size=256 docs/validation/integration-2026-10-24/typescript-namespace-bindings/probe.cjs worktree validation/namespace-current
```

The baseline intentionally reports `matchesExpected=false` because it demonstrates
the twelve false refusals; the current worker must report true. Output files use
exclusive creation. Partial failed diagnostic directories are not reused.
