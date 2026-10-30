# Meaning graph host CLI

`meaning build` performs one explicitly selected, bounded extraction job. It requires both a host provider configuration and `--trust-provider`. Default indexing, `meaning load`, `meaning query` and `meaning status` do not execute or resolve a provider.

```powershell
yoke --root C:/Work/project meaning build --config C:/Host/provider.json --policy C:/Host/meaning-policy.json --trust-provider
yoke --root C:/Work/project meaning load GRAPH_ID
yoke --root C:/Work/project meaning query GRAPH_ID "storage policy"
yoke --root C:/Work/project meaning aliases GRAPH_ID --min-score-bps 9500
yoke --root C:/Work/project meaning group --review C:/Host/groups.json
yoke --root C:/Work/project meaning join --review C:/Host/join.json
yoke --root C:/Work/project meaning status JOB_ID --compare-snapshot SNAPSHOT_ID
```

The provider file uses the existing host process-provider format. `command` and `args` select a trusted worker implementing the versioned native provider protocol; repository text cannot select executable authority. Credentials belong in the OS credential store, not these arguments or files.

```json
{
  "command": "C:/Host/meaning-worker.exe",
  "args": [],
  "modelId": "host-selected-model",
  "modelVersion": "pinned-version",
  "taskId": "project-meaning-pass",
  "currency": "EUR",
  "maxCostMicros": 1000,
  "taskLimitMicros": 10000
}
```

`maxCostMicros` reserves each missing packet attempt. The task limit must admit every missing attempt before the first worker starts. Reported costs and usage are provider claims; unknown sent costs retain their reservations. A successful all-cache-hit build starts zero workers and creates zero accounting attempts.

The policy explicitly selects complete files or captured symbol bodies. `contractHash` is a 64-character lowercase SHA-256 identity chosen by the host for its extraction contract. `generation` selects an immutable refresh namespace. The builder also binds its fixed prompt/schema contract, provider executable and selected input files, source bytes, context, analyzer epoch and capture policy.

```json
{
  "contractHash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "generation": "architecture-pass-1",
  "selection": { "files": ["src/storage.rs", "src/query.rs"] },
  "contextGroups": [["src/storage.rs", "src/query.rs"]],
  "providerInputFiles": [],
  "maxPackets": 64,
  "maxTotalSourceBytes": 4000000,
  "maxGraphBytes": 64000000
}
```

Use `{"bodies":["ENTITY_ID"]}` instead of `files` for exact captured body ranges. Context groups reference selected keys and add fixed packets; they do not replace the individual unit packets. Empty files and selected ranges above 16,384 UTF-8 bytes fail admission. There is no silent first-N truncation. Optional context groups and provider input files default to empty; the capacity defaults are those shown above. Select relevant absolute supplemental provider input files when ambient dependencies must participate in cache identity.

Builds honor the normal explicit snapshot, analyzer and capture options. Source access policy remains authoritative. Provider and policy JSON are strict, reject duplicate/unknown fields and are bounded to 64 KB and 128 KB respectively. Host permission and output capacity are checked before opening workspace state. No command automatically retries or resumes failed work.

Read commands require an existing workspace store and use their stored source bindings. They reject capture, analyzer, global snapshot and language server configuration; status compares against an optional `--compare-snapshot` without capturing current files. Changed and missing selected sources are reported separately. Queries match literal concept labels, descriptions and aliases, retaining the matched packet's directed relations, ordered hyperedge roles, citations and original receipt provenance.

`--max-bytes` must be 1024 through 4,000,000 and includes the trailing newline. A completed build whose status exceeds that budget returns a small durable reference containing its job and graph IDs. A later output transport failure reports both IDs in the error. Load, query and status reject an oversized response without creating overflow artifacts; increase the transport budget or narrow a query. Publication and original model receipts remain durable independently of stdout delivery.

The read-only [alias candidate command](meaning-aliases.md) compares labels and
declared aliases with bounded Unicode normalization and deterministic lexical
scores. It retains both original concept references and source evidence, never
merges concepts and starts no provider. Its separate capacity limits refuse the
entire request when exceeded.

The separate [group projection command](meaning-groups.md) accepts an explicit
host review file selecting disjoint packet-qualified concept groups. It retains
all original claims, evidence, parallel relations and ordered hyperedge slots in
a new read-only projection. It does not merge parent records, select groups from
lexical scores, authenticate human review or establish semantic identity.

The [checked join command](meaning-join.md) extends explicit presentation groups
across graphs sharing one exact canonical snapshot. It replays the joint stored
source/receipt closure and provides node lookup, degree and bounded directed
trace queries while preserving original claims and role slots.

All claims remain `inferred_concept` with `providerAttestation: unverified`. Checked source quotations establish source binding, not interpretation correctness or exhaustive semantic coverage. See [meaning-build.md](meaning-build.md) and [meaning-graph.md](meaning-graph.md) for accounting, cache, retention and source evidence details.
