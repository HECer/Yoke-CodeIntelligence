# Native Graphify adapter

The Windows Rust adapter independently implements the pinned public ten-tool,
six-resource surface. The seven local graph tools use checked immutable source
snapshots and a derived View with manifest, configuration and Markdown overlays.
The first query captures the project if no index exists; subsequent queries read
the existing immutable latest snapshot. Explicit indexing updates that selection.
Every View build validates canonical and selected source origins.

All seven local graph tools accept an optional `graphViewId` for an already
published immutable View. An explicit selection loads that checked View and its
original source bindings, even after newer indexing or saved-source changes; it
does not capture live source or start a provider. Unknown, malformed or
inconsistent selections fail without falling back to a default View. Omission
keeps the existing default structural selection. The hosted PR tools do not
accept this local-view selector.

Typed Meaning layers require the native typed query APIs. These Graphify tools
and resources reject such selections before producing results or reports.

`project_path` selects only a canonical absolute root already registered by the
host. It does not open, canonicalize or probe arbitrary client-supplied roots, and
does not change the active project. Graphify resource URIs select the active
project. These adapters do not execute repository code or remote commands.

Queries support bounded outgoing BFS/DFS, depth 1–6, explicit context filters and
labeled question-based context inference. Resolved parallel observations retain
separate IDs and exact evidence sites. Unresolved observations retain null targets;
they do not invent dependency definitions. Non-seed traversal hubs are bounded by
the degree p99 with a floor of 50. Returned induced observations retain parallel
and cross edges. Node labels use exact matches first; ambiguous candidates remain
explicit. Shortest paths use stored direction unless `undirected` is requested.

Community indices follow the checked selected-View Leiden report order by
descending size and deterministic identity ties. Canonical nodes and selected
structural overlay nodes participate in that projection; excluded overlay
nodes do not. Communities, degree ranking and statistics carry the selected
View, base-graph, projection-graph and immutable report artifact identities.
Parallel resolved in/out degree counts self-loops twice. Percentile exclusion and
community provenance reuse the checked native report, with no model calls. These
are structural heuristics rather than compiler, runtime or ownership guarantees.

For token-budget tools, the entire serialized JSON response is bounded by three
UTF-8 bytes per requested budget token, up to 256 KB. This follows the pinned
approximate three-character convention more conservatively for Unicode; it is
not a model-specific tokenizer guarantee. Truncation remains explicit, with original
selected counts, and retains no dangling observation/evidence references. If even
metadata cannot fit, the adapter reports an explicit budget error.

Report, statistics, hubs, surprising connections, confidence audit and suggested
questions are generated from source-bound immutable reports. Confidence is
represented by actual resolution/producer categories, never fabricated numerical
probabilities. Disk snapshots have no asserted Git build commit. Large resources
use the protocol's artifact transport or report explicit output-capacity limits.

`list_prs`, `get_pr_impact` and `triage_prs` use the independent
[host-authorized GitHub read provider](../../docs/github-read-provider.md).
The host must select its repository allowlist, network permission and OS-held
credential reference. Missing authorization fails explicitly. Owned HTTP fixtures
exercise the GET contracts; they do not certify a live external installation.
The separate webhook/review-publication lifecycle requires its own source-bound
qualification.

Tests cover public schemas/resources, Unicode, parallel call evidence, directed
and undirected shortest paths, BFS/DFS depth, explicit/heuristic context filters,
actual output budgets, ambiguous labels and foreign-root refusal.

## Source-specific execution evidence

At source `0e17b87a23f630752e2db8a61995a87ed36371fe`,
[Windows partition 4](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41239700108/artifacts/13057242787)
passes all eleven `selected_view_analysis` cases and all thirteen
`graphify_selected_queries` cases, without failures, ignores or filtering.
The former includes actual default-View package/Markdown community membership,
historical selected reports/resources and refusal before report publication.
The latter checks immutable nondefault queries and bounded route-prefix
behavior. The complete Windows default-profile source/text receipt is
`86d741f376519a5bf5945d3586e79da014d58c7350bc6368c6b788ec3c262270`.
These are executed selected-View contracts, not complete upstream, platform or
release acceptance.
