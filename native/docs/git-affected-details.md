# Captured-hierarchy Git affected details

`git affected` derives structural candidates from an existing verified Git
workflow report. It follows that report's captured base and head sources,
graphs and proven analyzer epochs. It creates no fresh capture, runs no Git,
provider or project command, and publishes no new artifact. Live source files,
refs and the latest snapshot do not replace the selected history.

[Git workflows](git-workflows.md) describes creating and loading those reports.
[Git impact details](git-impact-details-cli.md) supplies exact source quotations
and deleted/added-path rename candidates through `git digest ... impact-details`.
`git affected` adds captured type-hierarchy and incoming-reference explanations
with its own options and result schema.

## CLI

```sh
yoke --root /project --max-bytes 4000000 git affected REPORT_ID
yoke --root /project --max-bytes 4000000 git affected REPORT_ID --options /host/affected-options.json
```

The selected root must already contain a complete initialized Yoke Store.
Options are an explicit host JSON file, admitted through the shared guarded
reader; linked files or ancestor directories are refused. The raw file limit is
**64,000 bytes**, including whitespace. Omitting `--options` uses the defaults
below. A supplied object must contain all thirteen fields exactly once: unknown,
missing or duplicate fields, wrong types, negative/fractional/overflowing counts,
an unknown policy, and values above the maxima are input errors.

Output is JSON only: the complete canonical library bytes followed by one LF.
The global `--max-bytes` includes that LF and defaults to **32,000**; its maximum
is **4,000,000**. Select a larger envelope when needed. An insufficient envelope
refuses before output; it does not clip the result, return an artifact reference,
clamp options or change the result identity. `maxOutputBytes` separately limits
the canonical library result without the CLI's LF.

Global `--timeout` is milliseconds, with a maximum of 600,000. Zero timeout
refuses as deadline exceeded; zero global output capacity refuses as capacity
exceeded. These checks occur before options-file access or Store initialization.
This command does not accept snapshot, capture configuration, analyzer selection,
language-server configuration/trust, write or ignored-directory flags. Its
command-specific option is `--options`; `--format`, `--path` and `--provider` are
not accepted.

## Options

Every numeric default is also its maximum. Rust fields use snake_case;
the JSON field names below are case-sensitive.

| JSON field | Default / maximum | Meaning |
| --- | --- | --- |
| `policy` | `"captured_hierarchy_v1"` | Required exact policy string. |
| `maxGraphEntities` | 5,000 | Complete entity vector on each side. |
| `maxGraphObservations` | 50,000 | Complete observation vector on each side. |
| `maxGraphEvidence` | 100,000 | Complete evidence vector on each side. |
| `maxOriginalSeeds` | 2,000 | Original changed-declaration rows across both sides. |
| `maxExpandedSeeds` | 4,000 | Unique additional side/entity seed pairs. |
| `maxHierarchyDepth` | 16 | Edges in each separate hierarchy direction. |
| `maxIncomingDepth` | 8 | Incoming call/reference edges from each walk seed. |
| `maxStates` | 20,000 | Combined hierarchy and incoming walk states. |
| `maxEdgeVisits` | 200,000 | Whole-graph observation scans plus examined walk adjacency. |
| `maxRows` | 20,000 | Recursive array entries in the complete result. |
| `maxSourceBytes` | 64,000,000 | Distinct-hash source bytes used by the new validators. |
| `maxOutputBytes` | 4,000,000 | Complete canonical JSON bytes. |

Zeros remain zero. `maxHierarchyDepth: 0` disables hierarchy expansion;
`maxIncomingDepth: 0` keeps walk seeds without following incoming edges. Other
zero budgets refuse when positive credit is required. Even a no-change report
can require observation-scan, result-row and output credit. Capacity limits never
authorize dropping an ambiguous candidate or a parallel proof to fit a response.

`consumed.newEvidenceSourceBytes` counts source objects read by the affected
validators once per distinct content hash. Each side/path is still separately
admitted before sharing checked bytes. This counter excludes inherited report
loader and epoch-proof source reads; it is not a bound on total I/O or memory.
The inherited loader retains its own source and metadata limits.

## Reading the candidates

Original changed declarations remain seeds. Hierarchy expansion applies to
TypeScript methods with captured name offsets and an exact parent-key link to a
TypeScript class or interface. Constructors are excluded from expansion;
unsupported roots and same-name members retain explicit eligibility reasons.
A related member must belong directly to the reached type and have the exact
same captured name. Qualified names or matching source ranges do not supply a
missing parent link.

Captured `extends`, `inherits` and `implements` observations connect admitted
class/interface entities. Ancestor and descendant walks run separately from
each original owner: an ancestor walk does not turn around to include siblings.
`implements` is reported as contract membership. Each original or eligible
related member then seeds its own incoming `calls`/`references` walk.

Targets must already be bound to entities in the same captured side graph;
labels are not resolved into new targets. Captured heuristic or ambiguous
resolution categories remain visible. Parallel observations, original edge
direction, and evidence-vector order and multiplicity are preserved. A state has
one deterministic shortest witness, comparing the complete observation-ID
sequence when paths tie; separate proof references retain the examined parallel
observations and cycle edges.

The result binds the report artifact, workspace, options and each side's
snapshot, graph hash and analyzer epoch. Entity, observation and evidence tables
carry their source side and provenance. Original-row references point back to
the selected report; member reasons retain their original root and hierarchy
direction. Incoming states additionally retain their walk seed, so a shared
entity reached from different roots or seeds keeps those separate explanations.
Full source files are not embedded in the result.

Depth-limit states expose `boundary` and `unexaminedAdjacencyCount`; coverage also
counts unresolved/unsupported observations and unexamined boundary adjacency.
`coverage.withinPolicyComplete` describes the selected finite policy, including
these visible boundaries. It does not assert exhaustive dependency reachability.

## Rust and canonical JSON

```rust
use yoke::{
    capture::Deadline,
    git_workflows::affected_details::{self, AffectedOptions},
};

let options = AffectedOptions {
    max_hierarchy_depth: 2,
    max_incoming_depth: 2,
    ..Default::default()
};
let details = affected_details::derive(
    &store, report_id, &options, &Deadline::new(30_000),
)?;
let value = details.value();
let canonical = details.canonical_bytes();
```

`AffectedDetails` is opaque, with read-only `value()` and `canonical_bytes()`
and `Serialize`. It has no public constructor or `Deserialize`; caller JSON is
not an authority source for this API. Derivation retains one admitted Store read
view through final encoding and preserves a caller-owned transaction.

The schema kind is `git_affected_details_v1`, with policy
`captured_hierarchy_v1` and algorithm `yoke_git_affected_details_1`. Canonical JSON
is compact, recursively key-sorted and has no trailing LF. `wireBytes` is the
length of that complete canonical object, including its own count and full ID.
`resultId` is a domain-separated SHA-256 of the canonical full object with only
`resultId` omitted, retaining the options, bindings and `wireBytes`.

Source ranges are zero-based UTF-8 byte intervals `[startByte, endByte)` with
character-boundary checks. Line projections are one-based and count LF bytes;
for a nonempty interval `endLine` covers its final byte, while an empty interval
uses the line at its start. CRLF and Unicode bytes are preserved without source
normalization. Output line projections come from these captured byte intervals;
original report rows and IDs remain unchanged. New owner/name proofs cover roots
and entities used by the affected result, alongside the inherited loader's
existing complete report/source validation.

These are structural candidates. Same-name membership does not establish
compiler binding, runtime dispatch, override identity or full Graphify
behavior/schema equivalence. `complete`, `compilerBindingEstablished` and
`runtimeDispatchEstablished` remain false. No MCP tool or request-field addition
is introduced.

Library derivation failures use only `git_affected_details_input_invalid`,
`git_affected_details_capacity_exceeded` or
`git_affected_details_deadline_exceeded`. Recognized, parsed `git affected`
command failures use the same static markers. Domain/options refusals and
derivation or output-capacity failures occur before any canonical result is
printed; the CLI writes the diagnostic to stderr and returns a nonzero exit
status. Unknown flags or malformed CLI syntax use ordinary Clap diagnostics
before the command runs.
