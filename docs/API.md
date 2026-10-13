# Agent and library API

All requests are JSON objects. Common options are `snapshotId`, `maxBytes`, `timeoutMs`, optional estimated `tokenBudget`, and optional `taskId`. Unknown fields are rejected. Responses carry schemaVersion, requestId, snapshotId, status, data, warnings and metrics. A partial result may include artifactId and nextCursor. Tool errors have an explicit code/message. Do not interpret an empty list as proof of exhaustive absence; inspect coverage.

The exact machine-readable definitions are exported as `TOOLS` in `src/protocol/tools.ts` and served by MCP `tools/list`.

## Eight tools

| Name | Typical request |
|---|---|
| `ci_status` | `{ "mode": "index", "timeoutMs": 120000 }` |
| `ci_find` | `{ "snapshotId": "<sha256>", "query": "invoiceTotal", "kind": "symbol", "limit": 10 }` |
| `ci_read` | `{ "snapshotId": "<sha256>", "entityId": "<entity-sha256>" }` |
| `ci_trace` | `{ "snapshotId": "<sha256>", "entityIds": ["<id>"], "direction": "in", "relations": ["calls"], "depth": 2 }` |
| `ci_context` | `{ "snapshotId": "<sha256>", "query": "invoice tax", "limit": 5 }` |
| `ci_change_prepare` | `{ "snapshotId": "<sha256>", "rename": { "entityId": "<id>", "newName": "calculateTotal" } }` |
| `ci_change_apply` | `{ "planId": "<plan-sha256>", "idempotencyKey": "invoice-refactor-1", "mode": "isolated" }` |
| `ci_verify` | `{ "planId": "<plan-sha256>" }` |

Placeholders are not executable IDs: use actual IDs returned by earlier calls. Byte ranges are zero-based UTF-8 half-open intervals; source line ranges are one-based. Do not mix an entity ID with an overriding path/range, or byte and line range modes. Truncated original source is not silently claimed to be a complete body.

Read the full overflow result with `{ "cursor": "<nextCursor>" }` to `ci_read`. Cursor pages carry `data.text`; concatenate pages and JSON-decode to recover the original full result. Signed cursors cannot be reused in a different workspace. Artifact IDs refer to local immutable data, not public URLs.

## Bounded read plan

```json
{
  "snapshotId": "<actual-snapshot-id>",
  "plan": [
    { "op": "find", "query": "invoiceTotal", "kind": "symbol", "limit": 5 },
    { "op": "references", "limit": 20 },
    { "op": "read", "limit": 5 }
  ]
}
```

At most 16 operations. There is no shell, Python evaluation or arbitrary JavaScript in a read plan. `filter`, `select`, `group` and `limit` work on the current bounded result collection. Detailed semantics are in `QueryEngine.plan` and its tests.

## Explicit file edits

```json
{
  "snapshotId": "<actual-snapshot-id>",
  "edits": [
    { "kind": "replace", "path": "src/invoice.ts", "expectedHash": "<actual-source-hash>", "startByte": 20, "endByte": 30, "text": "replacement" },
    { "kind": "create", "path": "docs/invoice.md", "text": "# Invoice\n" }
  ]
}
```

`replaceSymbol` accepts a native TypeScript entityId and replacement text. For imported or heuristic spans, use explicit hash-bound edits. Exactly one of edits/rename/replaceSymbol is accepted. Source limits, protected paths, obvious-secret checks and UTF-8 boundaries apply.

## Controlled workspace integration

1. Prepare; inspect the full patch and `testFilesChanged`.
2. Invoke `ci_verify` for that plan. Static success means no new diagnostics within the implemented coverage, **not** proven functional correctness. Use reviewed command profiles for real tests.
3. In the trusted host, run `node dist/cli.js approve <plan-id> --root /project`. Test-file changes need `--allow-test-changes`. `--allow-unverified` is an explicit risk override, never inferred from source content.
4. Run `node dist/cli.js apply <plan-id> --root /project --mode workspace --allow-writes --key unique-operation-1` or use an equivalently authorized MCP host.

A changed base snapshot, expired/tampered plan or foreign edits must produce an error/conflict, not a force write. `recover` inspects interrupted journals and conditionally repairs only owned bytes. Never erase recovery-required state without reviewing it.

## Command verification

`code-intelligence.json` may contain:

```json
{
  "schemaVersion": 1,
  "verify": {
    "unit": { "command": "node", "args": ["--test", "test/invoice.test.mjs"], "timeoutMs": 30000 }
  }
}
```

Then use `node dist/cli.js verify <plan-id> --root /project --profile unit --trust-project --timeout 120000`.
Profiles run in a copied source tree. Dependencies are not installed or copied automatically. Use profiles appropriate for that tree, or explicitly authorize a dependency bootstrap after reviewing its risk. Any profile modifying captured source invalidates the report's source-integrity condition. Timeout/start failure is incomplete, not passed; test count is null if the actual runner output does not expose a supported count.

## Library

```ts
import { CodeIntelligence } from './dist/index.js';

const ci = new CodeIntelligence('/absolute/project', {
  config: { timeoutMs: 120_000 },
  allowWrites: false,
  trustProject: false
});
try {
  const indexed = await ci.invoke('ci_status', { mode: 'index' });
  if (indexed.status === 'error') throw new Error(indexed.error?.message);
  const found = await ci.invoke('ci_find', {
    snapshotId: indexed.snapshotId, query: 'invoiceTotal'
  });
  console.log(found);
} finally { ci.close(); }
```

The MCP transport supports initialize, initialized notification, tools/list, tools/call, cancellation, ping, and read-only local artifact resources. JSON is newline-delimited on stdout; diagnostics go to stderr. Supported negotiated protocol versions are 2024-11-05, 2025-03-26 and 2025-06-18. Tool results contain one complete text JSON payload rather than a duplicated structuredContent/text pair. No hosted HTTP MCP service or multi-user authentication is supplied.

## Configuration-aware results (0.2.0)

Coverage now carries optional `analysisContexts`; TypeScript observations/diagnostics retain their config identity. `ci_trace` accepts `analysisContexts: ["path/to/tsconfig.json"]` and reports `crossContextUnion`. Native rename refuses conflicting interpretations of a shared source edit with `RENAME_CONTEXT_CONFLICT`. See [configured projects](CONFIGURED-PROJECTS.md).

The `runLsp` library export and `lsp` CLI entrypoint are explicit host integrations. They are not automatically exposed as additional MCP tools. The options contain host trust, command/argument vector, source path/UTF-8 offset and one shared deadline. Read operations return checked locations; rename returns a prepared, persisted plan. Verification keeps `failed` status if a later check is unavailable or times out; inspect individual stages for incomplete coverage.
