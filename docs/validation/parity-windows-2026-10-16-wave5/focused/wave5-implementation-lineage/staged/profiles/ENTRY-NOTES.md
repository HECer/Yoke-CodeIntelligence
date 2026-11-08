# C profile-entry staging package

Status: ignored, uncompiled drafts only. No normal source/test/document files
have been changed. No Cargo, Node, provider or product execution was performed.
Root must promote tests first, obtain causal baseline RED and release production
separately after the current recovery/process work and frozen acceptance.

The accepted inputs are CONTRACT.md
`4dc1105faa6731076dde6f0d3e5003624049a3392a4a7c1dafd1ba94f6fad53e`,
API.md `f94a6cde...45b5f5` and FIXTURES.json `39f87515...a1f7` in the parent staging
directory. MANIFEST.json records their complete hashes and all staged hashes.

## Ownership and promotion

- C owns the staged copies and deltas for `native/src/trail_adapter.rs` and
  `native/src/protocol.rs`, plus `native/tests/monorepo_profiles.rs`.
- A owns `retrieval_scopes.rs`, the opaque checked run/partition, typed hard
  errors and ledger tests. B owns the strict native mode decoder, retrieval
  driver, balanced scorer and shared31-source fixture.
- Root owns normal-file promotion, private module registration and all tests.
  These copies are wiring drafts. They do not implement partitioning, balancing
  or fallback; their new call depends on B's explicitly unavailable driver stub.
- Promote B's shared helper to
  `native/tests/support/monorepo_retrieval_fixture.rs` before compiling the C
  test. C deliberately does not duplicate or edit that independently owned file.

## Exact entry changes

Session `ci_retrieve` advertises only `global` and `manifest_balanced_v1` in its
optional string `scopeMode`. It calls B's pure `native_scope_mode(&args)` before
select/index. Null, other JSON types, `auto` and unknown strings cannot reach
indexing. The ordinary native inventory remains8; the existing session profile
remains9 (8 minus2 guarded-write tools plus3 session tools).

Native absent/global still calls public `retrieval::find`; B strips only the
recognized global field and retains the old path without constructing a probe.
C does not change the old native final data-size admission or ordinary8schemas.

Trail/Graft keeps the exact6 argument schemas and adapter body conversion. Only
the `find_code` description and retrieval entry change. It forwards the original
deadline to `find_compatibility_auto` with a private caller-derived byte ceiling.
It contains no error catch, second `find`, new run or new deadline. The checked
driver alone decides allowed policy fallback using its already spent ledger.

The draft subtracts the exact serialized status/data envelope overhead from the
4,000,000-byte logical envelope ceiling. After the unchanged full/eight-line
definition conversion and existing adapter fields, it admits that exact final
envelope again. Overflow returns typed `OutputCapacity("compatibility_envelope")`
before protocol's existing artifact branch. Other Trail tools retain existing
transport behavior. No client knob or retry is added. A reviewed this caller
boundary: late conversion may hard-fail without reranking/reprobing; optimal
refitting of optional prefixes after conversion is not a new C guarantee.

## Test oracles and causal gates

`tests/monorepo_profiles.rs` uses only actual public session/protocol, adapter and
retrieval APIs. B's independently literal31-source helper supplies the same
approved corpus; there are no precomputed production score rows in C.

1. Freeze all8 native names and the exact6 compatibility schemas; inspect the
   optional native enum and unchanged9-tool session count.
2. Reject invalid modes with valid queries and require the exact public error
   `Manifest-balanced retrieval invalid_request: scope_mode`, mapped from
   `RetrievalHardError::InvalidRequest("scope_mode")`. An internal unavailable
   stub must fail this oracle. Preserve the independent checks that snapshot/
   change counts and SQLite total_changes remain unchanged. Reject a public adapter
   `scopeMode` argument before indexing: auto is host-selected only.
3. Compare native absent and explicit-global JSON bytes to a hand-authored old
   empty-result golden under epochs8 and9. The golden uses14source bytes,
   13declaration bytes, no matching terms and zero graph iterations, avoiding
   dependence on score formulas or runtime-dependent entity identifiers. A
   captured malformed marker outside the query ensures global does not probe.
   This golden is not claimed validated until root runs the baseline control.
4. Exercise explicit native balanced against a saved snapshot after a live
   manifest mutation. Exercise real Trail and Graft auto with the actual adapter
   defaults; a same-input native-global control must choose big_first/big_second
   while auto must choose big_first/small_only. Check exact source/hash/body
   anchors and both body conversion modes. Adapter index publication remains
   allowed; only guarded changes/artifacts/providers are excluded.
5. Check no-marker, one-package and malformed-source fallback metadata, zero/one
   query-scope reasons and preserved partition IDs. Unsupported syntax must not
   disclose raw marker bytes. Explicit balanced does not become auto/global in
   a degenerate one/zero-scope query.
6. Use real4x1,000,000-byte malformed markers plus29,000,000bytes of code, filtered
   away from the markers. Markers intentionally share CAS bytes but have distinct
   canonical paths. Independent native global admits all29MB; auto must retain
   the spent4MB, admit28MB and omit code/07.rs at32MB total. This is an observable
   budget-reset mutant check, not a performance claim.
7. Corrupt an actual saved marker blob. Native balanced must reach the typed
   source_integrity category, not an unavailable/mode/policy error. The real
   adapter must not return a success or publish an artifact. Host cancellation
   remains an error. This entry test covers initial inherited cancellation;
   deterministic cancellation after probing and exact borrowed-deadline identity
   are A/B run-level obligations, not falsely claimed by this test.
8. Restore two2MB full definitions through the actual protocol. Their final
   logical envelope must fail specifically at compatibility_envelope and leave
   artifact counts unchanged. An unavailable stub, excerpt truncation or old
   artifact success must not satisfy this oracle.

The heavy budget fixture has a180s test deadline and no ignored test annotation.
Root chooses when to execute it centrally. Partition/scoring authority, foreign
Store/partition handles, hard error taxonomy and read/cache multiplicity require
A/B's causal tests in addition to these profile tests. C does not claim all
pipeline paths were exercised merely because an entry returned an error.

## Scope of claims

These are proposed acceptance tests and unavailable wiring, not runtime evidence
or a released feature. Existing index/select side effects, analyzer8/9 source
proofs and all previous symbol/file refactoring sources remain unchanged. No
capture expansion, provider execution, write authority, exact upstream ranking,
performance, broader ecosystem, Linux/macOS or full parity certification follows
from the finite manifest policy.
