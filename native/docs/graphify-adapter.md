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

Community indices follow checked canonical Leiden report order by descending
size and deterministic identity ties. Community and degree ranking scope is
explicitly canonical-only; selected structural overlay nodes remain unassigned.
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
Selected-overlay community analysis and the separate webhook/review-publication
lifecycle remain open.

Tests cover public schemas/resources, Unicode, parallel call evidence, directed
and undirected shortest paths, BFS/DFS depth, explicit/heuristic context filters,
actual output budgets, ambiguous labels and foreign-root refusal.
