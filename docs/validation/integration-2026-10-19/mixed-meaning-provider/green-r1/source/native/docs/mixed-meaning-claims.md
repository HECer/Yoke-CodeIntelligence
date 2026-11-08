# Checked mixed-source interpretation claims

`mixed-view --selection HOST_SELECTION --meaning-inputs HOST_JSON
--meaning-claims HOST_CLAIMS_FILE` checks host-submitted interpretation data
against opaque checked mixed inputs. Schema2 selections still require explicit
`--native-pdf-text`. The closed claims file has `schemaVersion: 1`, the exact
prepared `inputHash`, and a typed `fragment` containing `concepts`, `relations`,
`hyperedges` and `coverage` as in the native Meaning fragment shape. This is a
separate interpretation family, not a native code Meaning graph or provider
receipt. The command runs no model/provider and persists no graph or input.

Support `sourceId` is a prepared unit ID. Its `startByte` and `endByte` use the
original typed unit's coordinate space, not offsets relative to the excerpt or
container. Every support must be nonempty and lie inside its selected unit;
the original checked reader then rereads it and checks exact UTF-8 boundaries.
Each exported support quote retains the native file, Office decoded/XML or PDF
page reference and corresponding precision, XML origins and container identity.
Decoded coordinates never become canonical native source coordinates.

The opaque `CheckedMixedMeaningClaims` can only be constructed by
`CheckedMeaningInputs::check_meaning_claims`. Its `mixed_meaning_claims_v1`
export nests the exact prepared inputs and original receipt/source-scope
metadata. Exported JSON cannot construct either checked capability. The graph
ID binds the closed request, input hash and original fragment order. Original
concept keys, aliases, claim order, repeated directed parallel/self relations
and ordered role-bearing hyperedge participants remain unchanged. Equal labels
do not merge concepts. Exact repeated participant pairs inside one hyperedge
and repeated support spans inside one claim are refused under the existing
fragment shape contract.

Every selected unit needs exactly one coverage row: `claims` when used by any
claim support, otherwise `no_claims`. Empty claim arrays with complete explicit
`no_claims` coverage are accepted as a host observation. They establish no
semantic completeness or relationship absence. No provider execution or receipt
is inferred from host JSON. `interpretationOrigin` remains
`host_submitted_unverified_interpretation`, `providerAttestation` remains
`not_established`, and `canonicalSourceAuthority` remains false. Checked source
quotations establish their exact bytes, not the interpretation's correctness.

Admission retains 128 concepts, 256 relations, 64 hyperedges, 2 through 16
hyperedge participants, 8 supports per claim and 256 unique support descriptors.
The closed request is at most 1,000,000 encoded bytes. Distinct unit observations
are charged separately even when their selected source bytes coincide; unique
support text is bounded to 128,000 bytes, each materialized read item to 256,000
encoded bytes, and their combined materialization to 4,000,000 bytes. Final
output still obeys the caller's combined budget including the nested inputs.
Deadline/cancellation failures and invalid inputs produce no partial stdout,
saved overflow or source/database changes. Semantic provider extraction,
durable mixed-graph publication/replay and general graph consumers remain open.
