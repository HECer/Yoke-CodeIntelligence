# Explicit source-category analysis policy

`AnalysisOptions.sourceCategoryPolicy` selects a separate derived-report policy.
Omission retains the original default JSON/options, producer, report and Markdown
identities. The original canonical graph, resolution strings, directed parallel
observations and source bindings remain unchanged under either policy.

```json
{
  "sourceCategoryPolicy": {
    "schemaVersion": 1,
    "resolutionCategories": {
      "syntactic": "extracted",
      "heuristic": "inferred",
      "inferred_concept": "inferred",
      "ambiguous": "ambiguous"
    },
    "relationWeights": {},
    "excludedKinds": ["file", "concept", "semantic_concept"],
    "excludedLabels": [],
    "excludeStubLabels": false,
    "extensionlessAsConcepts": false
  }
}
```

Save the host-selected JSON outside the analyzed fixture and run
`yoke --root /absolute/project --snapshot <saved-id> graph analyze --options /absolute/policy.json`.
Library callers can also pass these options to canonical or checked selected-View
analysis. With the explicit saved selection this performs no capture, provider,
model or source execution. Without `--snapshot`, the CLI retains its ordinary
current-source indexing behavior. The host
selects an existing immutable snapshot/View through the ordinary source gates.

Resolution-to-category mappings are declared projection parameters. They do not
convert heuristics into compiler proof, probabilities or observed execution.
Unlisted strings remain `unmapped`, with counts in the audit and explicit reasons
in relevant surprise rows. Original `resolution` and evidence IDs remain visible.

Clustering uses base weights selected by relation (default one). Only a mapped
`ambiguous` category receives the existing `uncertainWeight` multiplier; inferred,
extracted and unmapped categories retain their base weights. These are selected
projection weights, not measurements of an original upstream edge weight.

Hub suppression under this policy uses the sorted-degree position
`max(0, floor(nodeCount * percentile / 100) - 1)`, clamped to the available range.
Degree still counts resolved parallel incoming/outgoing observations and a loop
twice. Kind/label filters affect hub and surprise display; they never remove
entities or relationships from the source graph. Name-based stub and extensionless
path heuristics require their explicit flags. Actual source declarations named
`Path` or `Mock`, and real extensionless files, remain eligible by default.

Surprise candidates exclude structural import/contains/method relationships and
selected node filters. Multiple-source corpora select cross-file candidates;
single-source corpora select cross-community candidates. Runtime families group
TypeScript/JavaScript, JVM and C/C++ variants. Code/document/paper/image boundaries,
top-level directories, communities and peripheral/hub degree add explained score
components. Inferred calls/uses/references crossing runtime families or code/document
boundaries suppress the confidence and structural bonuses. A peripheral/hub bonus
can remain. Semantic-similarity relations remain visibly authored claims.

This is an independent native policy, not certification of identical Graphify
partitions, rankings or source interpretation. Native Leiden and weighted hub
reattachment remain; reattachment can exceed the library's `maxCommunitySize`.
The report retains those existing limitations. The source-category producers are
`yoke-source-category-report-1` and `yoke-source-category-view-report-1`.

Schemas are closed. There are at most 32 resolution mappings, 32 relation weights,
32 excluded kinds and 128 excluded labels; labels are 1–256 bytes, without controls
or obvious secret patterns. Weights must be finite in [0,1000]. The complete policy
fits 16,384 serialized bytes, preserving the selected-report record budget.
Duplicate JSON keys, unknown fields/versions, invalid budgets and expired deadlines
refuse before report publication. Selected-report replay reconstructs the exact
policy, checked View, original bytes and artifact identity; changed stored policy
metadata cannot reinterpret an earlier artifact.
