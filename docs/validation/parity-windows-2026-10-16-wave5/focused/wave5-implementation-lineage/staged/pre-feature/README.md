# First old-production envelope regression

Status: ignored, uncompiled and unexecuted staging only. Root owns promotion and
execution. This package does not alter the reviewed12-case profile package or
its manifest `4fa161403aac3c5f73c15a436ff0953148a2d97505d48e00620d920613293529`.

Promote only:

`native/tests/monorepo_pre_feature_envelope.rs`

The destination is identical relative to the repository root. No support file,
new module, unavailable type, new profile field, scorer stub or product wiring
is required. Imports and calls use existing public Engine, Session, Deadline,
protocol Connection, Trail adapter, model hash and database interfaces only.
`source-full-definition-test.rs.txt` is an exact reference extraction from the
current12-case draft; it must not be promoted or compiled.

Run from the repository root, only by the coordinator:

```text
cargo test -p yoke-code-intelligence --test monorepo_pre_feature_envelope full_definition_envelope_overflow_is_an_error_without_an_artifact_fallback -- --exact --nocapture
```

The manifest separately records the Cargo target and exact test selector.

The fixture retains the existing two2,000,000-byte Rust sources and aggregate
query with `full=true`, unchanged byte for byte. Five shared public-API helpers
are copied verbatim. The standalone version adds two valid controls: each
existing file, selected individually with `full=true`, must return its complete
untruncated function with exact source bytes, hash and original range. These
controls must succeed before the aggregate assertion can be reached. They prove
that an excerpt, unavailable route or broken source fixture cannot be this RED.

From these actual full bodies the test computes their exact JSON-encoded sizes
plus the minimum status/data/items/body envelope. Even this lower bound must
exceed the exact4,000,000-byte logical envelope limit. The aggregate request then
uses the real existing Trail protocol. Desired behavior is the exact public
error `Manifest-balanced retrieval output_capacity: compatibility_envelope`,
with no new artifact or guarded-change record and unchanged original sources.
The string asserts the future typed error without importing any future type.

Expected old-production causal RED: both full-definition controls pass, then
the protocol reports success via `immutable_paged_artifact` and publishes an
artifact. The assertion fails on that concrete success, rather than on an
unavailable auto driver. This expectation has not been executed by this agent.
No specific generated snapshot/artifact ID is hard-coded.

Failure diagnostics contain only bounded status/transport/error previews,
counts, booleans and byte lengths. The diagnostic object is admitted below2048
bytes. Full bodies and source contents are never interpolated into assertion
messages. The old reference excerpt's body-printing assertion is deliberately
replaced in the executable test.

Required sequence after the current whole-wave preservation:

1. Root captures and reruns the nonempty old-global golden on unchanged old
   production and preserves its GREEN receipt.
2. Root installs only this independent test, keeps all A/B/C sources, stubs and
   wiring unpromoted, and observes the concrete old artifact-success RED.
3. Only after those old-production receipts are preserved may root promote the
   separately reviewed monorepo tests and unavailable API stubs/wiring and run
   their distinct mode/ranking RED gates.

The manifest verifies mechanical lineage to the current profile test: exact
source construction, aggregate public arguments, all five helper functions and
reference excerpt. Only this small pre-feature package and its immediate source
references are hashed; no broad source audit or execution is claimed.
