# Native derived graph reports

An optional [source-category policy](source-category-policy.md) adds explicitly
mapped categories, ambiguity-only weighting, display filters and explained
runtime-family/source-kind surprise ranking. Omission retains the original
default options, producer and report/Markdown identities. Its declared
classifications never replace original resolution strings or source evidence.

`graph_extensions::analyze(&Store, &Snapshot, &Graph, &AnalysisOptions, &Deadline)` produces a deterministic source-bound JSON report and escaped Markdown report. `compare` compares two persisted snapshots from the same workspace. These APIs never execute source, start a provider, change the canonical graph, or infer complete runtime/compiler behavior.

The validator checks exact persisted snapshot and graph identities, unique IDs, endpoint membership, evidence references, source path policy and every captured source blob. Reads stream in 64 KiB chunks with one shared deadline, declared-size/type/link checks, SHA-256 verification and UTF-8 validation. Entity ranges are exact half-open byte ranges; entity end lines refer to the last included byte. Existing native evidence producers use either an inclusive last-byte or an exclusive-end line convention, so evidence end lines must match one of those two source-derived boundaries. Reported byte ranges and hashes are unambiguous. A persisted graph with incorrect line anchors is rejected even if the caller presents that same stored graph.

`AnalysisOptions` uses strict camelCase JSON. Defaults are:

```json
{
  "hubLimit": 10,
  "surpriseLimit": 10,
  "questionLimit": 10,
  "resolution": 1.0,
  "excludeHubsPercentile": null,
  "uncertainWeight": 0.5,
  "maxIterations": 10,
  "seed": 42,
  "maxCommunitySize": 0,
  "previousSnapshotId": null
}
```

Result limits are 1–100, resolution 0–100, uncertain weight 0–1, optional exclusion percentile 0–100 and iteration limit 1–50. All floating-point controls must be finite. Inputs are bounded to 5,000 entities, 50,000 directed observations, 100,000 evidence records, 20,000 captured files, 4 MB per source and 64 MB of validated source bytes. These are explicit implementation bounds, not a declaration that arbitrary repositories are fully analyzed.

The report includes:

- Architectural hub candidates ranked by resolved incoming plus outgoing observation count. Parallel observations count separately; a self-loop contributes twice. File containers are excluded from hub display. Degree is a structural ranking, not importance or confidence probability.
- Resolution/relation counts, unresolved observations, original coverage and omissions. Categories retain the canonical graph's labels; they do not certify compiler bindings.
- Actual pinned `leiden-rs` clustering over an undirected weighted projection. Syntactic observations have weight one; other resolved categories use `uncertainWeight`. Unresolved targets are excluded from this projection. Original direction and parallel evidence remain in the canonical graph.
- Optional percentile hub suppression followed by weighted neighbor-majority reattachment, or a separate group if no assigned neighbor exists. Hub reattachment can exceed `maxCommunitySize`, which applies to the library partition. The record states this distinction.
- Deterministic source-hub labels and unique-positive-pair undirected cohesion in [0,1]. An unavailable/nonfinite library quality is `null`; it is not replaced by a measured zero.
- Cross-file/community candidate connections and interleaved unresolved, candidate-validation, bridge and isolation questions. Their scores are heuristics. They never become new canonical relationships.
- Exact entity source bindings and original evidence anchors. JSON and Markdown artifacts are linked by a `native_graph_report_v1` record containing snapshot/workspace/graph identities, optional previous snapshot, artifact IDs and options.

The library exposes no internal cancellation callback. Checks occur before and after its bounded call; this API does not claim that a running Leiden iteration can be preempted exactly at the deadline. Expiration prevents report publication. SQLite publication uses an immediate transaction, the remaining request budget, an immutable record check and a deadline check before commit. Report identities have no timestamps and repeated identical analysis returns the same artifacts.

Snapshot comparison matches unique structural identities comprising path, kind, qualified name and parent context. It compares exact source-body hashes independently of snapshot-dependent entity IDs and shifted byte positions. Added, removed, body-modified and relocated entities are separate. Repeated semantic identities are explicit ambiguous groups; renames and cross-path moves are not guessed.

Observation comparison uses semantic endpoints, relation/resolution and exact evidence-snippet hashes as a multiset. Parallel call-site multiplicity is preserved. Edges involving ambiguous entity identities are reported separately. This is source-structure comparison, not formal behavioral equivalence. Communities with unique structural members have stable member-derived IDs. Communities containing ambiguous member identities use explicit snapshot-scoped IDs so distinct source groups cannot collide. Community history matching is a deterministic one-to-one, unique highest member-overlap candidate at or above 0.5; ties remain explicit ambiguity.
