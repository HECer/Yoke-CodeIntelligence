# Strict saved authority schema v2

Separately versioned ignored test-only addendum after root's explicit decision:
the NEW retrieval Run rejects duplicate keys AND unknown Graph/member/coverage
schema fields as HARD typed saved-source SourceIntegrity/schema corruption.
There is no manifest UnsupportedPolicy, compatibility fallback or budget reset.
Generic Store::graph and native absent/global bytes/semantics remain unchanged.

All bytes bound by cccfe018c3a1dc1cdcc2539f536451ca213c11f0347088e224cce52589483960,
369292a4411180dfd9f4fdbaed8c112902cdb28b4b259baff15a070a3be9234a and
18e8d83be0a8985bb0d711e6321e26d6a6e410961f08d3df698a80b3caf0c2d4 remain
immutable. No normal source, functionality, Cargo, Node, fixture or feature test
execution is released by this directory. Rustfmt is parse/format evidence only.

The private verify_saved_graph_in API is unchanged. After causal release it
must use bounded strict typed/custom decoding; it must not create an unbounded
or duplicate64MB Value tree and typed roundtrip to supply this policy. All
container/count/byte/deadline guards remain active before allocation. Preserve
genuine currently admitted native epoch8/9 Graph controls. Source scope proof,
Store identity, supplied full Graph fingerprint and relevant SQLite read
transaction remain separate mandatory bindings.

Unknown field rejection applies to Graph, Entity, Observation, Evidence and
Coverage objects, including fields that ordinary legacy serde would ignore.
Coverage.languages is a dictionary: keys are data, not an enumerated field
schema. Its duplicate DECODED keys are forbidden. Thus "rust" and "\u0072ust"
are duplicates, with no case folding or Unicode normalization beyond JSON
string decoding. Duplicate known object fields are also forbidden even when
the values are identical. A typed fingerprint alone cannot detect ignored
fields or collapsed same-value dictionary keys.

Six ordinary test drafts are supplied:

1. Genuine Engine-produced epoch8 AND9 saved Graphs remain accepted.
2. Unknown Graph/entity/observation/evidence/coverage fields are hard. A small
   fixture-only legacy Graph decode demonstrates that the typed fingerprint
   would be identical; this isolates the schema bypass. That untrusted typed
   data is never published or used as a checked source proof.
3. Same-value duplicate known Graph/member/coverage keys are hard.
4. Same-value escaped duplicate language-map key is hard, although legacy
   ordinary map decode collapses it to the expected fingerprint.
5. Real compatibility-auto rejects schema corruption with typed SourceIntegrity,
   static errors, unchanged SQLite limit and no returned fallback result.
6. The earlier foreign-transaction test is strengthened: this Store and the
   other same-workspace connection have distinct lower host LENGTH limits.
   Authority refusal must leave BOTH limits and the foreign caller transaction
   unchanged. The verifier cannot end its own caller transaction either.

Only small actual fixture Graphs use serde_json::Value while constructing
explicit invalid saved SQLite data. This is test data mutation, not a production
decoder recommendation or an unchecked authority shortcut. The positive fixture
must succeed before any deep schema assertion is credited as a causal RED.
An unavailable constructor/verifier failure is only unavailable-feature evidence.
No large graph, Windows runtime or schema guard is certified by unexecuted drafts.

Root can add the test child to its eventual private retrieval_saved_graph module
under cfg(test), path retrieval_saved_graph_schema_tests.rs. Root alone promotes,
records causal RED and releases the strict body. Independent review must confirm
no generic/global tolerance change and actual auto/new-Run coupling.
