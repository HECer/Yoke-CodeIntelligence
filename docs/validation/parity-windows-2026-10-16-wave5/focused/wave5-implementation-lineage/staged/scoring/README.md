# B scoring tests/stubs draft

Ignored staging only. No normal source, tests or docs were edited. No Cargo,
Node, provider, benchmark or test execution took place. Rustfmt parses these
draft files; that is not a compile or behavioral receipt.

This draft follows the accepted parent CONTRACT.md, API.md and FIXTURES.json.
Root alone promotes coherent tests/stubs and records actual RED before releasing
functional work. Every new scorer, arithmetic kernel and same-run fallback
driver is explicitly unavailable. Native absent/global delegates to the exact
old global function, changing only its private function name. The recognized
global selector is removed before delegation; no partition/run is constructed.
Strict invalid-mode taxonomy remains unavailable until its actual RED release.

## Files and intended promotion

PROMOTION.json maps each draft to its eventual normal location. Root registers
A's crate-private retrieval_scopes module separately. B's private balanced module
is nested inside retrieval.rs and its unit tests use the actual proposed
production arithmetic functions. None of these plain rows grant source access.

- retrieval.rs: entry/type/driver stubs followed by the unchanged legacy global
  body. New balanced/auto modes fail explicitly; there is no hidden global retry.
- retrieval_balanced.rs: whole-unit schedule, per-scope BM25/seeds, canonical
  arcs, graph iterations and complete per-file/final fusion unavailable stubs.
- retrieval_balanced_tests.rs: 15 literal arithmetic tests. Expectations are
  literal values from the accepted fixture matrix, never computed by production
  helpers. The source and overlapping-document budgets are separate schedules.
  The source fixture is also passed in reversed scope/unit order and must retain
  the same literal phase-name/omission order after index remapping.
- monorepo_retrieval.rs: seven genuine captured-source integration tests. These
  test the exact 31-file global/balanced contrast, historical source/marker
  replay, zero/one-scope explicit mode, real parallel call observations and
  graph-only packages, actual 32 MB source admission, and actual nested Rust
  declaration overlap at the separate 32 MB document ceiling.
- retrieval_driver_tests.rs: four direct private B-driver tests. Auto consumes
  a saved snapshot/graph with a malformed earlier marker and a corrupted later
  marker CAS object, requiring typed SourceIntegrity rather than success or
  generic unavailability. No adapter/index precedes that call. The fallback
  driver receives a real successfully probed opaque A run, then cancellation is
  activated and must reach rank_global_after_probe with the original run,
  deadline pointer and usage intact. No production environment switch is used.
  This proves the driver's cancelled-hard-error path and no replacement of the
  run's borrowed deadline. A child Deadline created inside run_bounded inherits
  cancellation, so this is not by itself a causal proof against every timer
  reset mutant. Inspect actual deadline-construction call sites during later
  implementation review; no timed fixture or universal timer claim is added.
  A genuine 66-scope mixed-marker corpus pins 64 canonical summaries/2 omitted,
  128 canonical declaration bindings/5 omitted and 64 diagnostics/6 omitted.
  A second call at the exact mandatory whole-result size returns empty optional
  prefixes and literal omitted totals; one byte below must be OutputCapacity.
  Pure serialized-cost rows separately pin the 256 KB optional planner and
  smaller caller remainder. They are arithmetic rows, not invented source
  anchors, and are not evidence of a massive-path live wire fixture.
- support/monorepo_retrieval_fixture.rs: B-owned shared literal31 fixture,
  also consumed by C's profile drafts. Every file is written with actual UTF-8
  bytes, indexed normally, and asserted against captured size/hash. Source and
  document boundary tests never forge SourceFile sizes, graph rows or anchors.
- legacy_retrieval_oracle.rs: mandatory OLD-production golden capture before
  promoting new mode stubs. The placeholder is deliberate, never evidence.

## Baseline and RED order

1. Root first promotes only shared support plus legacy_retrieval_oracle.rs.
   Run it against old retrieval.rs, record the complete result SHA256 emitted
   by its expected placeholder assertion, and freeze that literal in the test.
   Then rerun the SAME oracle to GREEN while normal retrieval.rs still has
   SHA c209e6da3dc36e09613066ea1031d3174067675d8ac97e235f887de4e6cc633b,
   before any A/B/C source promotion. The placeholder failure occurs before its
   final no-publication assertion and alone is not complete baseline evidence.
   A fixed test workspace ID makes the captured graph/entity IDs repeatable.
   The real global control must first return big_first and big_second with 31
   positive candidates; a setup/fixture failure is not a feature RED.
2. Promote coherent A/B/C tests and unavailable stubs. B's public 31-file test
   pins the global literal BM25 values and absent/global byte equality before
   the explicit balanced unavailable failure. Record actual test names/counts.
3. After root releases functional implementation, use the same assertions to
   verify [big_first, small_only], local ranks [1,1], local normalized scores
   [1,1], original source body/hash/range bindings and zero scoring writes.
   The small package's raw BM25 is independently pinned to
   0.28768207245178085 (sole-doc corpus, ln(4/3)); globally scored documents
   normalized only per package must fail. A separate small balanced capture
   pins Unicode query normalization and exact UTF-8 byte17 through the original
   CRLF function range, including internal CRLF and accented source text.
4. The boundary fixture first verifies actual parser ranges: outer functions
   cover 4 MB/3 MB files and inner functions span byte100..1000100. If the real
   parser setup fails, repair the fixture and obtain actual causal RED; never
   label an unavailable-source or range-setup failure a capacity regression.

The two real capacity tests allocate roughly 35 MB/29 MB of source text at
runtime and parse normally. They are not microbenchmarks and have no elapsed
time/SLA assertion. Source omission is c/01.rs at 3 MB after protected passes;
document omission is inner_a at 1 MB even though its source was admitted.

## Shared A/B/C contract

A owns exact Store/snapshot/graph/captured scope authority, borrowed deadline,
verified path cache and a single debit ledger. B gets source_cost (physical and
incremental/cached bytes), immutable VerifiedSource, exact original graph entity
document_cost/admit_document and immutable usage counters. B never resets that
run for compatibility fallback. C owns native pre-select enum validation and
Trail/Graft auto conversion, including the real 4 MB malformed-marker plus
29 MB code fallback test. A owns after-probe cancellation/foreign-handle and
forged-entity boundary tests. No source decoder error is caught generically as
an automatic fallback.

Balanced output retains bm25Score/graphScore/rankingScore and adds
normalizedBm25Score/normalizedGraphScore/localRank/queryCoverageCount/scopeId.
The exact mandatory retrievalScope contract is unchanged. Optional arrays use
retrievalScope.scopes, declarationBindings and sourceOmissions. Scope summaries
have id/root/kind/labels/memberships/counts; declarations are path/contentHash.
Admission counters are retrievalScope.admission.{markerBytesCharged,
bodyInputBytesCharged,inputBytesCharged,admittedSourceBytes,admittedSourceCount,
omittedSourceCount,documentBytesCharged,admittedDocumentCount,
omittedDocumentCount,queryExcludedSourceCount,queryExcludedDocumentCount}.
Reserve omittedScopeSummaryCount/omittedDeclarationBindingCount/
omittedSourceOmissionCount before bounded optional prefix admission. These
mechanical field choices were coordinated with A and C under the accepted API.

## Limits of the draft

The large 100,000-path/entity, 1,000,000-observation, 64 MB streamed graph,
4,096-byte canonical path and 16 MB ownership-map boundaries remain uncovered
by these B drafts. A's partition capacity tests do not prove optional response
serialization. Count-prefix and actual caller-envelope fixtures above provide
that separate coverage; no complete bound-matrix claim is made.

No passing tests, performance result, upstream ranking parity, full-contract
certification, wider manifest ecosystem support or Linux/macOS runtime result
is claimed. All accepted inventory identities/digests and false certification
boundaries remain unchanged. Current archive/product files remain frozen.
