# Historical selected-View comparison

The Rust library compares two explicitly selected immutable Views. It reports
structural candidates, directed observation changes, selected Meaning changes,
receipt provenance and coverage differences. Every result identifies both inputs
and remains bounded by one caller deadline.

## Native CLI

```sh
yoke --root /absolute/project --max-bytes 400000 graph compare-views --before before.json --after after.json
```

Each host-selected pin file is a strict JSON object with these six fields:

```json
{
  "schemaVersion": 1,
  "workspaceId": "<published workspace ID>",
  "snapshotId": "<published snapshot ID>",
  "viewId": "<published View ID>",
  "viewArtifactId": "<published View artifact ID>",
  "baseGraphHash": "<published canonical graph hash>"
}
```

Replace every placeholder with the exact 64-character lowercase hexadecimal
identity from the selected published View. Both complete registrations are
checked by the existing comparison adapter. A View ID alone never selects the
other fields implicitly. The earlier and later pins may name the same View.
Each pin file is limited to 4,096 bytes. Unknown fields, duplicate keys, invalid
versions/types/identities, links and parent-traversal host paths refuse before
opening source state.

`--continuity continuity.json` optionally supplies a strict object with
`schemaVersion: 1` and `declarations`, an array of at most 100 objects containing
`beforePath`, `beforeHash`, `afterPath` and `afterHash`. The document is limited to
128,000 bytes. Paths must be normalized, distinct on each side and bound to
actual primary files in the selected snapshots. These explicit host declarations
retain `verifiedIdentity: false`; they are not rename or symbol-identity proof.

`--max-rows` defaults to 10,000 and accepts up to 1,000,000 recursive array
elements. `--max-input-bytes` and `--max-source-bytes` each default to 64,000,000
and accept up to 128,000,000. The library's independent native admission profiles
still apply. Global `--max-bytes` defaults to 32,000 and accepts up to 4,000,000
bytes, including the single final newline. The CLI reserves that newline in
`options.maxOutputBytes` before comparison, then delivers the existing canonical
JSON bytes without another envelope. Zero is a real refusing budget; no result
is clipped or published as an overflow artifact. `--timeout` defaults to
120,000 ms and accepts 1 through 600,000 ms under one shared deadline.

This command admits a complete existing Store before its normal constructor and
holds the checked state paths through comparison and delivery. It does not
initialize missing/incomplete stores, capture current source, run a provider,
change logical records or publish artifacts. Normal Store client and WAL
bookkeeping retain their existing behavior. Captured history can still be read
after live source files have changed or disappeared. Global `--snapshot`,
`--capture-config`, `--include-ignored-dir`, `--analyzer-epoch`,
`--language-server-config`, `--trust-servers` and `--allow-writes` refuse before
host-file access.

After argument parsing, domain failures return only
`view_comparison_input_invalid`, `view_comparison_capacity_exceeded` or
`view_comparison_deadline_exceeded`; source paths, host JSON values and underlying
error chains are not printed. Admission failures leave stdout empty. Delivery
uses ordinary stdout and cannot retract bytes after an OS transport failure.
Clap argument parsing retains its separate help and syntax diagnostics.
Existing `graph compare BEFORE_SNAPSHOT AFTER_SNAPSHOT` remains the canonical
snapshot comparison. Graphify/MCP schemas and selected analysis's
`previousSnapshotId` behavior are unchanged.

## Library entry points

For two Views registered in one Store, pin all five selection fields:

```rust
use yoke::{
    capture::Deadline,
    graph_view::View,
    store::Store,
    view_comparison::{Comparison, ComparisonOptions, SelectionPin, compare_published},
};

fn compare_registered(
    store: &Store,
    before: &View,
    after: &View,
) -> anyhow::Result<Comparison> {
    let before = SelectionPin::from_view(before);
    let after = SelectionPin::from_view(after);
    compare_published(
        store,
        &before,
        &after,
        &ComparisonOptions::default(),
        &Deadline::new(30_000),
    )
}
```

`SelectionPin` binds workspace ID, snapshot ID, View ID, View artifact ID and base
graph hash. Both registrations and all requested pins are checked under one
retained Store read transaction. Each side must pass the real source policy for
its complete primary and original/rebound history before its source objects are
read. A later denial on the second side can follow authorized reads on the first.

`compare_meaning_archives(&before, &after, &options, &deadline)` accepts two opaque
`graph_view_archive::CheckedMeaningArchive` values. Their checked packages retain
the complete captured source union, so comparison can work after the original
Store or source workspace has been removed. Arbitrary JSON cannot construct a
checked archive or a `Comparison`.

Both entry points require the same workspace, analyzer epoch and primary capture
policy. They replay archived epoch-8/epoch-9 evidence without upgrading it to a
different analyzer epoch. The operation derives its result without publishing an
artifact, changing the registry or starting a provider.

## Result semantics

`Comparison::value()` exposes immutable structured data.
`Comparison::canonical_bytes()` exposes its bounded canonical JSON serialization.
`comparisonId` hashes the complete canonical result before that ID is inserted;
original-row digests cover the complete original rows before redaction. Object
keys are canonicalized for these new comparison identities. Array order and the
difference between absent fields and explicit null values are preserved.

Structural correspondences are candidates based on captured source and structural
context. Ambiguous candidates remain explicit. Directed parallel observations and
self-loops preserve their original multiplicity. Ordinary observation groups are
reported when multiplicity changes under the complete structural, relation and
evidence key; equal nonambiguous groups contribute to totals without producing a
delta row.

Meaning comparison preserves selected IDs separately from materialized claim IDs,
ordered role-bearing participants, source-binding order and evidence precision.
Receipt reuse uses the original job, response artifact, attempt and request
identity. Unknown usage/cost, explicit zero values, currency and reported versions
remain distinct. Missing Meaning data is reported through presence and coverage
changes. Captured quotes and complete source objects are excluded from the result;
evidence keeps its exact captured ranges and original digests.

Up to 100 `DeclaredFileContinuity` entries may explicitly connect primary file
paths and hashes. Both ends must match the selected primary snapshots and form a
one-to-one declaration. Such declarations remain unverified identity statements;
they do not infer symbol continuity or remap descendant symbols. Results retain
`renameProofAvailable: false`, `complete: false` and explicit limitations.

## Budgets and cancellation

Defaults are 10,000 recursive array elements, 4,000,000 serialized output bytes,
64,000,000 pair-proof bytes and 64,000,000 distinct source bytes. The four limits
are caller-settable `usize` budgets; zero is a real zero budget and values above
the defaults remain supported. A budget failure returns
`comparison_capacity_exceeded` without truncation or an artifact fallback.

The proof budget counts each complete typed side envelope, including that side's
primary Snapshot and archived proof. Comparing an archive with itself counts the
proof twice. Source objects are excluded from this counter; the source budget
separately counts actual UTF-8 bytes once per distinct hash across both complete
source unions, including uncited historical members.

In source-only comparisons, incidental whitespace in the initial Snapshot/Graph
SQL rows is removed by typed parsing and reserialization. Raw strings retained
inside an archived Meaning proof remain part of the counted envelope. Existing
native admission limits apply independently: the comparison budget does not
replace the fixed raw-proof and source-admission profiles. These counters are not
a total process-memory limit or a guarantee that all reads happen after pair-wide
budget checks.

Recursive result rows include nested arrays and repeated copies. Serialization,
hashing and source reads check the same inherited deadline and cancellation state.
Exact limits succeed; the next charged row or byte refuses. No throughput or
total-memory service guarantee is claimed.

The public surfaces are the Rust library API and the explicit `graph compare-views`
host CLI. The native eight-tool MCP contract and its existing
`previousSnapshotId` behavior retain their separate contracts.
