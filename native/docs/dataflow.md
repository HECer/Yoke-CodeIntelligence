# Structural intraprocedural dataflow

`dataflow::analyze(&Store, &Snapshot, &Graph, Policy, &Deadline)` analyzes immutable TypeScript, TSX, JavaScript and JSX source with the linked tree-sitter grammars. It produces source-bound **candidates**, not vulnerability proofs. Analysis never imports or executes captured source, calls a model, or starts a compiler/provider.

The snapshot and graph must exactly match the current native Store's immutable records, including workspace identity. Reports carry `snapshotId`, `workspaceId`, `graphHash`, exact UTF-8 byte ranges, native CR/LF line ranges, content hashes and containing entity IDs. The complete report is published as a hash-addressed artifact without altering the original graph.

`Policy` is explicit host input. Its source selectors match structural member paths; sink selectors identify argument indices; sanitizer selectors assume that the named operation removes taint. None of these names are resolved through a compiler, imports or aliases. A same-named local operation can therefore have different runtime behavior. Repository comments and report data cannot grant commands or permissions.

```json
{
  "sources": ["req.query", "req.body"],
  "sinks": {"eval": [0], "query": [0]},
  "sanitizers": [],
  "maxIterations": 32,
  "maxNodes": 100000,
  "maxRecords": 1000
}
```

The visitor tracks lexical shadowing, function parameters and function-scoped `var` bindings. Direct assignments replace prior values; concatenation combines sources; conditional paths join their environments. Loops use bounded monotone joins. Unknown calls conservatively propagate argument and receiver taint and are counted as structural call sites. Each function is analyzed separately using its declaration-time enclosing environment. This does not model actual call order or closure timing.

Bounds include 1–64 loop iterations, 1–1,000,000 visited nodes, 1–10,000 source/finding records, 128 recursive levels, 16 MB per parsed file, a shared deadline, 4 MB of source/finding payload and an 8 MB serialized report. Parsing uses a deadline callback. Result or fixpoint exhaustion sets `boundedOut: true` and `status: "incomplete"`; deadline or unavailable-grammar failures are errors. Unsupported captured languages and constructs are disclosed. `exhaustive` and `semantic` are always false.

Destructuring conservatively shares the input's taint. Heap/property assignments, reflection, imported aliases, exceptional/abrupt control, JSX runtime behavior, dynamic calls and interprocedural effects are not fully modeled. Unsupported constructs have source anchors; their results must not be treated as complete analysis. Zero findings is not a security certificate.

Run `cargo test --locked -p yoke-code-intelligence --test dataflow` for native behavioral fixtures covering shadowing, overwrites, joins, exact Unicode anchors, sanitizer assumptions, unknown calls, budgets, unsupported languages and immutable scope rejection.
