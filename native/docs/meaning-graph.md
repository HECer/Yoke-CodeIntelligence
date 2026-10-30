# Typed meaning claims over submitted source

The `meaning_graph` module validates and materializes model-supplied concepts, directed relations and ordered-role hyperedges. It is a pure data operation: it does not read source files, open a store, execute a provider, authorize a command or contact a service. The host-selected build workflow separately verifies immutable captured source and provider accounting; see [meaning-build.md](meaning-build.md).

Every materialized result remains `resolution: "inferred_concept"`, `providerAttestation: "unverified"` and `complete: false`. Exact supporting bytes identify what the inference cites. They do not establish that a model inspected those bytes, prove its interpretation, or certify exhaustive semantic coverage. Pure materialization explicitly reports `bindingValidation: "submitted_bytes_only"`; it cannot establish captured-store membership from a supplied hash.

## Versioned wire contract

The additive operation is `extract_meaning_v1`, carried in the existing schema-1 model envelope. Existing summary, embedding, query, ranking and receipt identities retain their original definitions.

`MeaningInput` has `workspaceId`, `unitSetHash`, `contractHash` and `sources`. Each source contains a strict `anchor` (`sourceId`, `path`, `contentHash`, `startByte`, `endByte`), exact UTF-8 `text`, and `omittedBytes: 0`.

Two new identities have deliberately different serialization rules:

- `sourceId` is the existing native `identity` of the ordered tuple `("meaning-unit-v1", workspaceId, path, contentHash, startByte, endByte, hash(text))`.
- `unitSetHash` is `identity(serde_json::to_value(sources))`. This hashes the canonical JSON value with sorted object keys, preserving the selected source array order. It does not hash Rust struct field order.

These local source identities do not contain snapshot-specific entity IDs. A build can reuse exact unchanged inputs in another snapshot only after its own capture and context rebind checks. `contractHash` identifies the admitted host extraction policy; the build cache additionally binds the actual compiled prompt/schema contract and pinned provider inputs.

Inputs contain one to sixteen nonempty sources, each at most 16,384 UTF-8 bytes, with at most 128,000 source-text bytes in total and a 1,000,000-byte serialized input limit. A selected range must fit the supported 4-MiB file coordinate space. There is no silent source truncation. Source paths are normalized syntax labels; they grant no filesystem access.

## Claims and their own support

The strict `Fragment` output contains `concepts`, `relations`, `hyperedges` and `coverage`. Empty claim arrays are valid when every selected source explicitly reports `no_claims`.

Concepts carry a local `key`, `label`, `description`, `aliases` and `support`. Relations carry directed `source`/`target` concept keys, a relation label, description and their own support. Hyperedges carry a relation label, description, support and an ordered `participants` array of `{role, concept}` pairs. Participant order and role assignments are retained; a repeated identical role/concept pair is rejected. Every hyperedge requires at least two distinct referenced concepts. No implicit clique or binary relation is manufactured from it.

Every support entry names a submitted `sourceId` and absolute `startByte`/`endByte`. It must be a nonempty range inside that source's submitted excerpt. The validator subtracts the excerpt's original start offset exactly once when checking the UTF-8 text. Materialized evidence keeps the absolute offsets and exact full quotation, its path and content hash. Identical quotations from different source units retain distinct bindings.

`validate_fragment_shape` can check typed endpoints, ranges and coverage from anchor descriptors. `normalize` additionally checks the actual submitted UTF-8 bytes, and `materialize` always performs that complete validation. A worker-side shape pass cannot substitute for source validation in the parent build.

Per fragment, limits are 128 concepts, 256 directed relations, 64 hyperedges, two to sixteen participants per hyperedge, and one to eight support ranges per claim. Labels and aliases are at most 160 UTF-8 bytes, descriptions at most 1,000 bytes, and concepts have at most eight unique aliases. Local keys, relation labels and roles are bounded lowercase ASCII identifiers. Obvious secrets and unsupported control characters are refused.

At most 256 unique support ranges and 128,000 aggregate quotation bytes are accepted. Shared support is quoted once. A larger valid-looking graph fails instead of returning truncated proof. Fragments are limited to 1,000,000 serialized bytes and materialized packets to 4,000,000 bytes; the build admits aggregate worst-case capacity before dispatch.

## Conservative normalization and coverage

Normalization sorts unordered support and alias sets, then deduplicates only byte-equivalent typed claims with identical supporting bindings within the same workspace, submitted context and contract. It remaps local endpoints to the resulting immutable concept IDs. Descriptions, relation direction, hyperedge roles and participant order remain identity inputs. Contradictory claims remain separate.

Aliases and matching labels do not merge distinct concepts. There is no case folding, Unicode normalization, embedding threshold or fuzzy identity assertion. Parent build provenance retains the original provider response even when identical claims normalize together.

If exact concept deduplication collapses all participants of a hyperedge to one concept, the fragment is refused rather than publishing a degenerate hyperedge. Empty file selections are also refused as model inputs; hosts must choose explicit supported nonempty ranges instead of silently dropping selected files.

Every submitted source appears exactly once in coverage. `claims` requires actual support from at least one accepted claim; otherwise the source must report `no_claims`. Neither outcome proves that no other meaning or relationship exists. Pending, ready and stale build states are separately scoped to the captured inputs and comparison snapshot.
