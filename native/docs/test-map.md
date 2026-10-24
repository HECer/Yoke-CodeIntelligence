# Snapshot-bound test selection and actual Node collection

`test_map::import(&Store, &Snapshot, Value, &Deadline)` imports supplied per-selection observations. Import checks the exact immutable snapshot/workspace identity, unique bounded test IDs, `executed: true`, Boolean outcomes, source paths, content hashes and valid native CR/LF line numbers. It canonicalizes line hits and publishes an immutable artifact plus a `native_test_map_v1` record. Unknown input fields cannot grant execution authority. A supplied observation is **not an execution attestation**.

```json
{
  "snapshotId": "<native snapshot ID>",
  "tests": [{
    "testId": "invoice tests",
    "snapshotId": "<same native snapshot ID>",
    "executed": true,
    "passed": true,
    "files": [{"path": "invoice.mjs", "contentHash": "<captured SHA-256>", "lines": [1, 7]}]
  }]
}
```

Import accepts at most 10,000 tests, 1,000,000 supplied line hits per file and 24 MB of structured input/output. Foreign, duplicate, malformed, stale or uncaptured source observations fail. Persisted maps are revalidated against their source snapshot and immutable artifact when selected.

`test_map::select(&Store, &base, &target, &known_test_ids, &Deadline)` compares exact captured path/hash sets. It selects tests whose observed files changed and always retains failed tests, including when no source changed. Missing maps, new/unknown tests, unknown changed paths, configuration changes and test-file changes widen the result to the union of every recorded test and the caller's known tests, even if the caller list is empty or partial. Deleted and new paths participate in the comparison. Snapshot provenance must match the native Store; a map from another workspace is rejected.

Selection is a conservative aid for iteration. Dynamic coverage only describes its supplied or locally observed runs. `exhaustive` remains false; the full suite is the final release gate.

## Trusted optional Node collector

`test_map::collect_node(&Store, &Snapshot, &[Selection], node_command, host_trust, &Deadline)` actually executes each selected `.js`, `.mjs` or `.cjs` test file in a separate copied snapshot. `Selection` contains `testId`, `path` and an optional `namePattern`. Collection requires explicit host trust, Node 22.16+ and 1–100 unique selections. Missing commands or capabilities fail; there is no simulated fallback. JavaScript module context should be present in captured metadata, for example `package.json` with the appropriate `type` or an explicit `.mjs`/`.cjs` extension.

An absolute host command is bound once. Bare command names are resolved through absolute PATH directories outside the analyzed workspace, before entering the copied source directory. Relative executable paths are refused. Repository content cannot choose the executable or enable trust. The shared native process runner applies one deadline, bounded input/output, sanitized environment inheritance and process-group/Windows Job Object cleanup.

Each selection gets its own V8 coverage directory and host-authored reporter. The reporter forwards actual Node test summaries, failures and stderr events; source stdout is not parsed as protocol. Passing requires a successful exit, complete bounded output, a successful per-file summary, at least one executed/passed test and zero failed/cancelled tests. A skipped-only, zero-executed, timed-out or overflowing selection never passes. Execution and log artifacts identify the selected test file/name pattern.

After execution, collection verifies the entire copied tree's captured file set and hashes. Changed/deleted files, added files and links reject collection before import, including files absent from coverage URLs. Original source is never written. A failed collection leaves the existing test map intact.

V8 coverage is bounded to 1,000 JSON files, 16 MB per file, 64 MB in total and 1,000,000 ranges per selection. Only URLs belonging to captured copied paths are interpreted; unsupported/out-of-tree URLs are ignored without reading their targets. UTF-16 offsets are validated against source, including surrogate boundaries. Native source lines sample their first nonwhitespace character. The narrowest active range supplies its hit count; equal-width zero hits win conservatively. This is observed line sampling, not complete statement or assertion attribution.

A copied tree is not an OS sandbox. Trusted tests retain the host account's filesystem and network privileges. The collector does not install excluded dependencies. Node may access paths outside the copy under host privileges; their coverage is not imported. Per-selection maps do not prove test adequacy or exhaustive coverage.

Run `cargo test --locked -p yoke-code-intelligence --test test_map` for import/selection fixtures and actual local Node collector checks, including source stdout forgery, skipped-only selections, whole-tree tampering, UTF-16/CRLF mapping, deadline and output exhaustion.
