# Source-bound Git impact details

`git_workflows::impact_details::derive` computes an unpersisted Rust result from
an existing verified Git report ID, typed `DetailOptions`, and a shared deadline.
`ImpactDetails` has read-only `value()` and `canonical_bytes()`, Serialize, and no
public constructor or Deserialize. `render_html` accepts only that sealed result.
This implementation preparation follows three actual released-API fixture passes
and 34 actual unavailable-feature failures. Its functional runtime acceptance is
still root-owned; no universal upstream parity is claimed here.

Both immutable Snapshot/Graph bindings and both real capture policies with every
member path are admitted in one retained Store read transaction before the
existing report loader reads source. The loader remains mandatory and validates
the original receipts, source bytes, reconstructed changes, declarations and
parallel observations. A caller-owned transaction is never committed or rolled
back. No live file/ref/latest fallback, capture, Git subprocess, analyzer,
provider, network, credential, new publication or retention grant is introduced.
Existing Git report schema, IDs, analyze/load and old digests remain unchanged.

Metadata admission has explicit fixed raw profiles: **32,000,000 bytes per
primary base/head Snapshot and 64,000,000 bytes per primary Graph**, with SQLite
cell limits and length checks before typed loading. A request-wide SQLite cell
envelope of 64,004,096 bytes also bounds inherited/transitive capture-lineage
snapshot/record reads; tighter pre-existing connection limits remain in force
and the prior limit is restored on every exit. Existing released validation retains its own fixed
limits. A legacy report exceeding these new producer bounds can refuse capacity;
this feature does not promise derivation for every unbounded legacy graph.
Those profiles are independent of caller-selected source, line/work, row, quote
and output counters. Inherited loader reads cover complete source unions; the
new producer's `sourceBytes` counts only required candidate/quote source objects,
checked once per distinct hash after each side/path's separate real admission.
It is not an aggregate IO, peak-memory or hard scheduler limit.

Every original change, changed declaration, incoming impact and parallel
observation remains present and ordered. Hunk/declaration/evidence quotes bind
their own base or head snapshot, original path/hash and exact UTF-8 byte range.
Null absent-side anchors stay null; safe empty ranges stay exact empty text.
Inclusive quote line intervals count LF bytes, preserving CRLF/Unicode source
bytes. A bounded prefix reports its actual range and explicit excerpt reason;
exhausted allowances and unsafe complete-source output report explicit omissions.
The existing obvious-secret rule plus unsupported control characters govern
output safety; no fabricated redaction is called exact text. This is not complete
DLP. Repeated occurrences retain their original IDs and consume quote allowance
separately. Coverage describes quote completeness, while overall `complete`
remains false because structural impact and rename identity remain candidates.

Deleted/added paths additionally use `native_line_multiset_dice_1`. Split LF,
remove CR only from CRLF, trim ASCII space/tab edges, drop empty normalized lines,
and preserve all other bytes and multiplicities. Score is
`floor(20000*matched/(baseLines+headLines))`; 6000/10000 qualifies. Raw identical
sources qualify even when empty. A one-base/one-head content group is uniquely
identical content, not proven rename/entity identity. Every qualifying partner
remains visible, including score ties and modified competitors of exact pairs.
Same-path edits stay separate. Logical line counts are per eligible path even
when physical checked source/count maps are reused by hash. Pair count and
projected merge work are admitted before any candidate cross-product scan.

Default limits are 100,000 candidate pairs, 128,000,000 selected source bytes,
200,000 normalized line occurrences, 10,000,000 projected merge units, 200,000
recursive array rows, 4,000,000 canonical output bytes, 16,384 bytes per quote,
and 1,000,000 emitted quote bytes in total. Explicit zero hard limits remain
zero; zero quote allowances omit nonempty ranges and preserve safe empty ranges.
No caller-sized preallocation is used. Row membership and disjoint serialized
fragments are charged before result growth, followed by a bounded exact canonical
writer. `wireBytes` includes itself and the complete ID; `resultId` hashes the
recursively key-sorted full result with only that ID omitted. Arrays and nulls
remain unchanged. HTML escapes all five markup-sensitive characters and has a
separate checked expansion bound of six times actual canonical bytes plus its
fixed wrapper; it does not change the structured bytes or canonical wire counter.

Public refusals expose only static input-invalid, capacity-exceeded or
deadline-exceeded markers. Original errors and chained paths/source/SQL/Git text
are discarded. Deadline checks cover native preflight boundaries, source reads,
line normalization/comparison/sorting, candidate construction, quote work and
canonicalization/hashing/output. The released secret matcher remains a bounded
synchronous scan with checks before and after it, not an OS scheduling guarantee.

The `git impact` CLI uses the bounded derived result. Captured TypeScript
hierarchy/same-name structural seeds are available separately through
[affected details](git-affected-details.md), and explicit current-worktree index
observations through [conflict triage](git-conflicts.md). Area classification,
compiler-bound override identity, broader PR behavior and remote freshness remain
separate. These implementations do not upgrade inventory-wide certification.
