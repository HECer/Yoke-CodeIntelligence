# Native source knowledge and retrieval

`native/src/knowledge.rs` implements project knowledge against immutable native snapshots. These records are application data in the project's `.yoke` store. They are independent of the coding agents' central AgentMemory service and do not replace its backend.

The library never runs indexed code, a model, a coverage collector, a tracing agent or a SARIF analyzer. All source hashes and entity identifiers are checked against the selected immutable snapshot and graph. Inputs cannot grant write or provider permissions. Path checks use the same protected-path and portability policy as native source reads.

## Authored knowledge

`remember(store, snapshot, input, deadline)` accepts this strict JSON shape:

```json
{
  "kind": "hypothesis",
  "text": "Invoice totals require integer cents.",
  "actor": "reviewer",
  "paths": ["src/invoice.rs"]
}
```

Kinds are `hypothesis`, `confirmed`, `dead_end`, `correction` and `rule`. Each record captures the exact content hashes for 1–100 paths in the selected snapshot. Text is limited to 20,000 UTF-8 bytes; obvious credentials, unsupported control characters, unknown fields and unanchored paths are rejected. Actor names are attribution labels, not authentication. A `confirmed` kind is the author's assertion, not an independently verified fact.

An optional `supersedes` names an existing record whose source scope overlaps. Supersession is checked and inserted in one SQLite transaction. A record can have only one direct successor; subsequent corrections can supersede that successor. Originals remain immutable.

`recall(store, snapshot, query, limit, deadline)` returns source anchors classified as `current`, `changed`, `excluded` or `deleted`, along with `stale` and `superseded`. Deleted means absent from the selected snapshot; an explicit recorded exclusion is reported separately. A historical snapshot can still report an unchanged historical anchor. All records are labeled `authored_data_not_execution_authority`. Recall includes stale and superseded records so a correction history is visible. Limits are 1–100 results, 10,000 records and 32 MiB of knowledge JSON.

## Local sparse TF-IDF

`build_local_vectors(store, snapshot, graph, deadline)` builds a reproducible sparse vocabulary and normalized TF-IDF weights for symbols and document concepts. Unicode letters and numbers, underscores and identifier case boundaries contribute lexical terms. Up to 1,024 preceding source bytes capture nearby comments; a document is limited to 8,192 source bytes. This is statistical lexical retrieval; no pretrained semantic or multilingual model is present.

`search_local_vectors(store, snapshot, graph, model, query, limit, deadline)` computes normalized cosine ranking against that exact snapshot's saved vocabulary. Unknown vocabulary yields no ranked matches. Ranking is explicitly `ranking_not_evidence` and cannot establish a call, dependency or runtime relationship.

The corpus is bounded to 2,000 entities, 20,000 vocabulary terms, 2,048 terms per entity and 200,000 sparse coordinates. The result reports omitted entities and term occurrences. Source verification uses a bounded 8 MiB cache shared by each request. Collections are immutable under their versioned model identifiers.

## External vectors

`import_vectors(store, snapshot, graph, input, deadline)` accepts:

```json
{
  "snapshotId": "<native snapshot SHA-256>",
  "model": "provider:model-version",
  "items": [
    {
      "entityId": "<entity SHA-256>",
      "contentHash": "<source SHA-256>",
      "vector": [3.0, 4.0]
    }
  ]
}
```

Items must be unique, have one shared dimension and bind to the exact selected entities and source hashes. Vectors are normalized before an atomic collection insert. Zero, nonfinite, underflowed and overflowed norms fail. Bounds are 10,000 items, 4,096 dimensions, 2,000,000 coordinates and 24 MiB per collection. Dense and sparse collections together are limited to 512 collections and 128 MiB. Reimporting an identical collection is idempotent; replacing values requires a new versioned model identifier.

`search_vectors(store, snapshot, graph, model, query_vector, limit, deadline)` verifies persisted dimensions, source hashes and unit norms before cosine ranking. Hash binding authenticates the referenced local bytes, not the provider's claim that it inspected them. No model or provider is executed.

## Imported execution and analysis artifacts

Each import validates the complete input before persisting a content-addressed JSON artifact. Returned `artifactId` is readable with native `ci_read` artifact cursors. Imports retain their declared trust and do not silently mutate the canonical graph.

`import_lcov(store, snapshot, input, deadline)` accepts `{snapshotId, hashes: {relativePath: sourceHash}, lcov: text}`. Relative forward-slash paths, source line bounds, duplicate line records and supplied `LF`/`LH` summaries are checked. Every source record must end with `end_of_record`; truncated input fails. Test-name labels and line hits are retained. Function and branch records are explicitly listed as ignored; this implementation does not claim full LCOV analysis or independent execution attestation.

`import_runtime(store, snapshot, input, deadline)` accepts `{snapshotId, spans}`. Each span requires `id`, `path`, `contentHash`, and a one-based `line`. Optional fields are `parentId`, paired UTF-8 `startByte`/`endByte`, `name`, `startTimeNs`, `durationNs`, and up to 32 bounded string `attributes`. Parent IDs must exist and the parent graph must be acyclic. A byte range must start on its declared line. Limits are 10,000 spans and 24 MiB. The artifact describes `this_recorded_execution_only`; supplied spans are not independent proof of execution or of unobserved paths.

`import_sarif(store, snapshot, input, deadline)` accepts `{snapshotId, hashes, sarif}` with SARIF 2.1.0 runs. Relative physical source URI locations are decoded and scoped before reading source. UTF-16 code-unit columns and Unicode-code-point columns convert to exact UTF-8 boundaries; split surrogate pairs fail. URI bases, artifact indexes, logical-only locations and offset-only regions fail explicitly. Limits are 100 runs, 10,000 declared results, 20,000 source findings and 24 MiB. Supplied findings remain `static_analysis_finding_not_formal_proof`.

This is a bounded source-bound import API, not support for every possible SARIF or tracing schema. Empty declared SARIF results are accepted as a supplied report and do not prove absence of vulnerabilities.
