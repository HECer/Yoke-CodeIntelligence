# Explicit community projection policy

Windows scoped checks pass six numerical units and 97 graph/View/retention
integrations, formatting and all-target Clippy with warnings denied. An actual
CLI control preserves both old report families and repeats the selected mode
byte-exactly with original graph/source evidence. Primary-checkout integration
and retained TypeScript checks remain separate pending steps. This is not full
upstream or platform acceptance.

`AnalysisOptions.communityPolicy` selects a separate numerical policy:

```json
{
  "communityPolicy": {"kind": "neighbor_majority_v1"},
  "excludeHubsPercentile": 80,
  "seed": 42
}
```

The kind is a closed versioned selection, independent of the optional
[source-category policy](source-category-policy.md). Unknown versions, extra
fields and grants refuse. Omitting communityPolicy preserves the old default
and source-category-v1 options, producers and report identities. The selected
policy uses `yoke-community-policy-report-1` and
`yoke-community-policy-view-report-1`; it never changes original source, directed
parallel observations, evidence anchors or resolution labels.

The existing source/relation/uncertainty settings supply projected weights.
Only positive resolved edges enter this numerical graph. Parallel observations
and opposite directions are summed into one undirected pair in a fixed addition
order. Original observations remain separately present in the graph and audit.
Unmapped source-category labels retain their explicit audit. A zero projected
weight is not an absence claim about original evidence.

Hub exclusion uses unique positive-weight neighbor degree, with a self-loop
counted twice. Its percentile index is floor(nodeCount * percentile / 100) - 1,
clamped to the available sorted degrees, and excludes strictly larger degrees.
The report's existing God-node degree still counts original directed parallel
observations. These are different declared measurements.

True projected isolates stay separate. A non-hub whose other neighbors are all
excluded hubs is held out of the initial partition, including when it also has
a self-loop. The connected core runs the pinned native Leiden implementation.
Hubs then join the community with the largest count of distinct already assigned
non-self neighbors, with one vote per neighbor regardless of edge weight or
parallel count. A hub without a vote starts a community. Held-out neighbors join
after hubs are assigned. Ties and processing order follow the canonical native
source path/name/kind/position order, avoiding snapshot-hash-induced churn.
Duplicate structural identities retain the existing ambiguous/snapshot-scoped
identity disclosure.

Two bounded split stages follow reattachment. A group larger than
max(10, floor(totalNodeCount / 4)) gets a new induced Leiden pass at resolution 1.
Next, a group of at least 50 nodes with cohesion below 0.05 gets one more such
pass. The second stage sees the first stage's groups. Each stage operates on
disjoint subgraphs and does not recurse or retry indefinitely. The configured
seed, iteration bound and library maximum apply to each pass. A pass can return
the same group: neither splitting nor a hard final size bound is claimed.

Cohesion counts unique positive non-self pairs divided by possible distinct
pairs, with singleton cohesion 1 for this policy only. The quality field refers
to the initial connected core before reattachment and splitting; it is null if
no such numerical pass was available. Diagnostics show stranded/isolated nodes,
thresholds, attempted splits and actual library-call count. Deadlines are checked
through numerical loops and before/after the bounded library call, not inside it.

Canonical report retention reconstructs the exact selected configuration and
serialized bytes. Selected-View publication and retention separately replay the
checked View and its source bindings. A rehashed diagnostic, changed mode or
forged registration cannot reinterpret an older artifact. The policy grants no
source writes, runtime, network, model or approval authority. Existing report
capacity and publication admission remain mandatory.

This is an independent native policy using leiden-rs 0.8.1, not an assertion of
identical graspologic partitions, remote interpretation or runtime truth. It uses
positive-weight topology and its own declared ordering. Broader upstream,
platform and semantic-quality qualification remains separate.
