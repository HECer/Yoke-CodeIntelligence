# Canonical policy retention and exact JSON floats

The [checkpoint](checkpoint.json) records the final green-r4 guard: 101 Rust
tests across fifteen targets, format, all-target Clippy with warnings denied,
typecheck and TypeScript 260 passed / 3 existing skips. All inputs remain frozen
during the guard. Documentation follows validation. Overlapping development runs
are not added together. The [index](index.json) hashes every archived file except
itself and preserves the original raw logs.

A valid canonical source-category report previously blocked retention with an
unknown-artifact-field error. Canonical policy reports now replay source scope,
original options, numerical analysis and historical comparisons read-only, then
compare exact native serialized bytes. Markdown and both historical source roots
retain their independent checks. Legacy report shape validation is preserved.

The new historical fixture additionally exposed JSON float parsing that changed
serialized values and caused a noncanonical-report refusal. The independent
float-red-r1 witness loses one IEEE bit for -1.1819158364087053e-285. Enabling
float_roundtrip on the already locked serde_json1.0.154 preserves finite boundary
values and finite members of a deterministic 4096-pattern sample, signed zero, Unicode and
canonical bytes. Duplicate-key, depth, byte-budget, overflow and deadline refusals
still pass. Cargo.lock and all dependency versions remain unchanged.

Canonical retention tests cover policy reports with/without a prior snapshot,
source and artifact roots, expired deadlines, and rehashed changes to cohesion,
audit, configuration, future fields and signed zero. The stronger adversarial
fixture keeps the authentic report registered and adds the forged one, avoiding
an unrelated orphan-artifact refusal. representation-red-r2 proves the zero-sign
mutation passed JSON Value equality; the final comparison requires exact bytes.

Other raw phases remain: red-r1 is a fixture compile error calling a private
Markdown helper; red-r2 reproduces the original retention failure; green-r1
encounters the fixture's deliberately expired Store deadline before resetting it
for a subsequent read; green-r2 exposes historical float corruption. green-r3
passes all tests before strengthening the signed-zero counterexample.
representation-red-r1's early fixture passes via an incidental orphan refusal;
representation-red-r2 retains both registrations and exposes the actual flaw.

The actual CLI repeats the same saved owned snapshot. Default AND source-category
v1 raw stdout, JSON/artifact/Markdown identities match the pre-change controls.
Actual retention and purge planning recognize all saved-root dependencies without
deleting any state. Original source bytes stay unchanged. No model or analyzed
project code is executed.

A complete native recheck for the shared parser feature remains pending here.
Existing Windows OS1314, ignored runtime gates, other R1-R9 work, new community
policy, current-head hosted/upstream qualification, main and release remain open.
