# Analyze a selected immutable graph view

`graph_extensions::analyze_view(store, view, options, deadline)` computes hubs,
Leiden communities, audit counts, surprising connections and suggested questions
over a checked, already published `graph_view::View`. It never overwrites the
canonical graph or canonical report registrations. Existing canonical `analyze`
keeps its source validation, report format, options and content identities.

Graphify `get_community`, `god_nodes` and `graph_stats` use this implementation.
Their optional `graphViewId` selects a historical published view; omission uses
the existing default structural view. The six analysis resources accept the same
selector, for example `graphify://audit?graphViewId=<id>`. Invalid, repeated and
unknown selectors fail. Source lookup does not index live files for an explicit
view, start providers, execute project code or fetch remote content.

Reports distinguish `baseGraphHash` from `projectionGraphHash` and bind the view
ID, immutable view artifact, workspace, snapshot, selected overlays and options.
They register under `native_graph_view_report_v1`, separate from canonical
`native_graph_report_v1`. The projection's `graphHash` is not a canonical Store
graph identity. `previousSnapshotId` is rejected for selected analysis until a
separately checked view comparison exists.
The audit/surprises/questions resource envelopes retain `graphHash` as an alias
of the graph actually analyzed (`projectionGraphHash`), with `analysisScope`
set to `selected_view`; `baseGraphHash` identifies the unchanged canonical graph.

The numerical algorithm counts resolved parallel in/out observations; a
self-loop contributes twice to degree. Unresolved observations remain in the
audit without contributing resolved degree. Leiden uses the existing explicitly
labeled weighted undirected projection and deterministic seed. Neither this
projection nor community membership changes the original directed observations.

Original nodes, source bindings, observations, evidence, overlay lineage and
provider attestations accompany the report. Hub/community/surprise/question
anchors resolve back to their original source node. A sole binding retains its
original precision, including enclosing-file and unique-literal evidence.
Multiple bindings remain explicit, with a null representative anchor and
`multiple_source_bindings` selection. No generated source label or file-level
anchor becomes an exact symbol token. Provider claims remain unverified where
their selected overlay says so. Coverage distinguishes canonical coverage from
the selected overlay manifests and retains omissions/limitations; completeness
remains false.

`prepare_view_report` constructs an opaque `PreparedViewReport` without
publishing artifacts or report records. `report()` exposes an immutable result;
`publish(store, deadline)` rechecks its original immutable view and store scope.
The Graphify adapter admits the final tool response or formatted resource body
before publication, so invalid options, insufficient response budgets and
oversized resource bodies do not publish selected reports. Budgeted community
responses may omit the complete descriptor, explicitly directing readers to
`reportArtifactId`; original report records remain intact.

`validate_view_report_record(store, id, value, deadline)` is a read-only retention
boundary. It strictly decodes and verifies the registered scope, reloads the
checked View, reconstructs the report and Markdown, and compares exact artifact
bytes and identities. It returns `ViewReportRoots` containing the snapshot,
view identity and exact report/Markdown/view/overlay artifact roots. It does not
repair, publish or reinterpret a projected graph as canonical source authority.

Validation covers existing Graphify behavior with actual manifest/Markdown
nodes, hand-calculated parallel/self-loop degrees, original precision, semantic
provider lineage, multiple bindings, historical replay, tampering and budget
admission. A fixed-workspace pre-change oracle separately freezes canonical
report JSON, registration JSON and Markdown identities.
