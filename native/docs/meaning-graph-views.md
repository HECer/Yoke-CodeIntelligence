# Explicit meaning claims in immutable query Views

`graph_view::BuildOptions` accepts `meaningGraphIds`, an explicit list of already
published native meaning graph IDs. An omitted or empty list adds no serialized
field to the options, View, record or archive. Existing source-only View and
report identities retain their released serialization.

Selection does not launch a model, resolve an installed provider, read provider
configuration or retry a build. Each selected graph must match the View's exact
workspace, snapshot, canonical graph, analyzer epoch and capture policy. A graph
from an older snapshot cannot be silently rebound during a query. Hosts must
first publish a separately checked rebound meaning graph through the explicit
meaning build workflow.

The optional typed `meaning` layer is separate from the existing source
`nodes`, `observations`, `evidence` and `overlays` arrays. Its selections and
packets preserve original graph/claim IDs, packet context, exact source bindings,
coverage and receipt provenance. Query IDs namespace claims by selected graph
and packet, so equal local claims in two selected graphs stay distinct. Reused
responses retain their original snapshot, job, request, response artifact,
attempt, reported currency/cost/usage and rebound status. They do not become a
fresh provider receipt.

Concepts, directed relations and ordered-role hyperedges remain
`inferred_concept` with `providerAttestation: unverified`. Aliases are search
candidate labels; matching aliases do not merge concepts or establish identity.
A concept has no code path, declaration byte range or declaration anchor. Its
citations describe the exact captured bytes supporting an inferred claim.

## Queries through the existing tools

The ordinary native profile keeps its eight existing tool names. An explicit
published `graphViewId` selects these read-only behaviors:

- `ci_find` searches source nodes and typed meaning concepts, relations and
  hyperedges. Meaning kinds are `meaning_concept`, `meaning_relation` and
  `meaning_hyperedge`. A path filter matches a claim's own citation paths, not
  the first file in its packet. Alias matches are labeled as candidates.
- `ci_read` with a meaning `entityId` returns `item` and all of that claim's own
  `citations`. Relations and hyperedges can cite multiple files. Meaning
  evidence IDs can also be read directly. Source path/range selectors cannot
  redefine a meaning read. No citation is returned as `declarationAnchor`.
- `ci_trace` returns source entities and tagged meaning query items, with
  separate typed meaning relation, hyperedge, citation-link and evidence arrays.
  Directed relations obey `in`/`out`/`both`. Supporting file links and hyperedge
  incidence are neutral and expand only with `both`. A concept reaches a
  hyperedge vertex in one hop and another participant in a second hop. Roles
  and participant order are preserved; no clique or role-derived arrow is
  manufactured. Relation claims retain their own citation metadata; their
  support does not create extra endpoint relationships.
- `ci_context` combines source excerpts and a selected claim's citations under
  one `maxSourceBytes` budget. Truncated excerpts retain their complete citation
  metadata separately. Exhausted quotations have explicit omissions. UTF-8
  boundaries remain exact. Neighborhood evidence omits quotation text so it
  cannot bypass the source budget in a second field.

Supporting incidence has inferred meaning resolution. An `exact_binding`
filter does not upgrade an inference or citation to a compiler binding. Seed,
depth and visited-node limits apply to source and meaningful query items
together, including hyperedge vertices. Successful bounded queries still report
`complete: false`.

## Admission and consumer boundaries

The checked meaning dependency closure is admitted before default structural
preparation. Its distinct graph/response artifacts and bounded job/cache/attempt
records share the existing 32-MB selected-input budget with structural and
semantic inputs. Meaning bytes are charged once. Combined row limits and the
32-MB serialized View limit remain separate admission checks; these are not a
total-process memory bound or exhaustive semantic-coverage claim.

Default structural graphs are prepared without publishing records or CAS blobs.
Their exact released bytes participate in the combined budget alongside
explicit structural, semantic and meaning selections. Source validation,
semantic selection and final View serialization must all succeed before one
caller transaction checks every expected immutable structural and View
registration before publishing the prepared graphs and View. Existing
conflicting registrations are rejected before any CAS write. Rejected admission
leaves the database and CAS unchanged. Once publication starts, an IO or commit
failure can leave an unregistered physical CAS blob; database registrations
remain subject to the caller transaction.

Source-only Graph projections, existing View analytics, export formats and
single-snapshot collection archives refuse a meaning-bearing View before their
own publication. They must gain a typed consumer before they can include it.
Converting a concept into a source entity with its first citation would invent
a declaration and is deliberately unavailable.

The optional archive type and checked dependency interfaces preserve a future
multi-snapshot bridge. Original and rebound source scopes require exact frozen
source blobs and typed receipt/capture replay. Missing historical source cannot
fall back to live files. Until that container consumer is integrated, a
meaning-bearing View cannot be registered through the legacy collection path.
