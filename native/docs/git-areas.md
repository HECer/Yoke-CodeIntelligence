# Captured Git impact areas

`git areas REPORT_ID` derives a read-only classification from a freshly replayed
native Git impact report. Every original impact-detail row, quote, side, directed
parallel observation and parent result ID remains intact in `impactDetails`.
New occurrence pointers, path classifications and groups add an interpretation;
they do not rewrite either structural or Meaning graphs.

```sh
yoke --root /existing/workspace --max-bytes 4000000 git areas REPORT_ID
yoke --root /existing/workspace git areas REPORT_ID --meaning SAVED_MEANING_GRAPH_ID
```

There is no Git, provider, credential or executable-selection flag. Both commands
read saved sources only. They do not capture source, inspect a working tree or
ref, invoke a provider, publish state or fall back to current/latest data.
Explicitly selected unavailable or mismatched Meaning graphs refuse; they do
not silently turn into directory-only success. Existing state is required.

## Area evidence

Without selected Meaning graphs, a path is classified by its exact captured
parent directory, including the trailing `/`, or `(root)` for a top-level file.
This is labeled `directory_spelling_only`, not semantic module or ownership
identity. Base and head remain separate even when paths and directory labels
are equal.

One to four unique host-selected saved native Meaning graphs can contribute
inferred concept/file memberships. Their registration, completed job and full
receipt/source closure are replayed. Workspace, exact base/head snapshot and
canonical graph hash must match. Evidence paths and content hashes must also
match that side's captured manifest. Imported labels or JSON do not become a
source/read/write grant.

Every concept keeps its graph, packet, original concept position and identifier.
Original records and exact support evidence are retained. Equal labels never
merge concepts across packets, graphs or source sides. Candidates rank by the
number of distinct supported paths; all smallest sets are preferred. Equal
minima remain visibly ambiguous, with `label:null`, even when their labels are
identical. Broader candidates remain available. Directory fallback applies only
when no selected saved concept claims that path.

This independent policy is informed by the pinned public
[Trail module-label contract](https://github.com/trailhq/Trail-Code/blob/3f10899e2fcee1fc990e8c11f27085016d79d0de/src/blast/modules.ts).
Its distinct-file counting and visible ties are explicitly Yoke behavior;
upstream byte/schema equivalence and semantic quality are not established.
Full original labels are retained.

`occurrences` preserve changed-path, per-side hunk, declaration, incoming-impact
and observation-evidence positions through JSON pointers into the unchanged
`impactDetails` parent. `pathClassifications` reference every occurrence.
Concept groups retain all candidate memberships and identify preferred ones;
directory groups contain only fallback paths. A group with several members is
a classification, not proof that those members share identity or execution.

## Admission and resource bounds

`--options /absolute/host/options.json` is a closed object with exactly `details`
and `limits`. `details` uses the existing eight-field
[impact-detail options](git-impact-details.md); this consumer caps each field at
that producer's default. `limits` contains all eleven fields below. Defaults are
also hard maxima; zero remains zero. Unknown/missing/duplicate fields, invalid
types, relative/unsafe host input paths and values above maxima refuse.

| Field | Default / maximum |
| --- | ---: |
| `maxPaths` | 20,000 side/path classifications |
| `maxOccurrences` | 200,000 original-position references |
| `maxSnapshotFiles` | 20,000 across both manifest selections |
| `maxConcepts` | 2,048 side-qualified concept rows |
| `maxEvidenceRows` | 16,384 selected graph evidence rows |
| `maxSupportReferences` | 32,768 concept support occurrences |
| `maxMemberships` | 32,768 distinct concept/side/path memberships |
| `maxAssignments` | 65,536 path/candidate assignments, or one per fallback |
| `maxMeaningBytes` | 8,000,000 per saved graph artifact |
| `maxMetadataBytes` | 16,000,000 new preflight bytes |
| `maxOutputBytes` | 4,000,000 canonical result bytes |

Manifest counts include both sides even when the snapshot ID repeats. Concept
and support counts include each projected source side; evidence-row counts
include the full packet tables once per selected graph. New metadata counters
cover the two manifest reads and selected Meaning registration/job rows.
Registrations and actual graph artifact sizes are admitted before Meaning
replay. These counters are not total inherited replay I/O, peak memory or a hard
scheduler quota. The existing impact/Meaning loaders keep their own limits.
A retained SQLite cell ceiling of 16,004,096 bytes can refuse a larger legacy
input that those direct APIs would otherwise admit.

Disjoint serialized payload reservations bound parent/proof/result growth; the
final bounded canonical writer admits the complete result. No rows or source
proofs are clipped to make a result fit. Global `--max-bytes` independently
includes the CLI's one final LF. The complete envelope fits before stdout.
Global timeout is 1–600,000 ms.

One owned read transaction pins all inputs. Caller-owned transactions refuse.
The original SQL length, busy and deadline/cancellation policy is restored on
success and failure. No SQL/application/CAS/source mutation or latest-pointer
change occurs. Unrelated snapshot, capture, analyzer, LSP, trust-server and write
overrides are unavailable.

The Rust result is sealed, serializable, and exposes `value()` and
`canonical_bytes()` without a public constructor or deserializer. `resultId`
hashes the full result with only that field omitted. The embedded parent's
original identity and canonical contents stay unchanged.

`classificationCompleteForSelectedPaths` describes only the admitted impact-path
selection. `complete:false`, `semanticIdentityEstablished:false`, unverified
provider attestation and directory/inference bases remain explicit. This is not
an exhaustive repository module catalog, authenticated ownership, compiler
override/dispatch binding, merge-safety proof, runtime observation or universal
upstream certification.
