# Saved Graph allocation addendum

Separate ignored draft authorized by root after the concrete allocation finding.
Every file bound by capacity369292a4411180dfd9f4fdbaed8c112902cdb28b4b259baff15a070a3be9234a
and partition18e8d83be0a8985bb0d711e6321e26d6a6e410961f08d3df698a80b3caf0c2d4
remains unchanged. No normal source, Cargo, Node, service, fixture or behavior
test was executed. Current native27087 validation receives no source changes.

## Concrete gap and ownership

Store::graph at native/src/store.rs492 delegates to load at512. load retrieves an
unbounded String from graphs.value, then calls serde_json::from_str. Bounding
the SUPPLIED Graph before this comparison does not bound a corrupt oversized
SAVED row. It also does not make metadata size/type and payload fetch atomic.

Only the NEW checked retrieval Run must replace that relevant Graph comparison
with this private verified read. Generic Store::graph and native absent/global
retrieval retain their current behavior and byte contracts. The new Run cannot
call query_features::source_scope's generic Graph load first and then claim the
new guard protected it. No marker/source-policy fallback can catch this failure.

Existing source patterns are federation.rs935 and capture_ignored.rs63:
temporarily lower SQLITE_LIMIT_LENGTH to min(prior,MAX+4096), query byte length
and typeof, then fetch bounded raw text and restore prior. The addendum
strengthens this sequence with one relevant caller-owned read transaction.
Do not increase an existing host limit or restore a guessed default.

## Private API seam

retrieval_saved_graph.rs stages ONLY an unavailable verifier:

verify_saved_graph_in(&Store,&Transaction,snapshot_id,expected_graph_fingerprint,
&Deadline)->Result<(),RetrievalHardError>

The transaction must belong to this exact Store connection, even for equal
workspace IDs. The caller's new Run begins the relevant read transaction and
holds its snapshot through authority comparison; the verifier borrows it and
cannot commit it. Expected fingerprint comes only from the bounded supplied
Graph stream, never request JSON. Full immutable Snapshot/Store/captured-source
binding remains the Run's separate requirement. Returning verification success
constructs no source capability and grants no expanded roots.

After root's actual RED release, the implementation must:

1. Check original deadline/Store/transaction binding. Establish the relevant
   read snapshot and use bounded row operations within it.
2. Lower the connection LENGTH limit to min(previous,64000000+4096). The4096
   allowance is SQLite row/key/record overhead, not additional Graph JSON.
3. Query length(CAST(value AS BLOB)) and typeof(value) for the exact row. Check
   bytes<=64000000 and storage type=text BEFORE fetching a Rust payload String
   or decoding it. length(TEXT) is insufficient: embedded NUL or multibyte UTF-8
   must not undercount the stored byte extent.
4. Fetch bounded raw data through the same transaction, recheck exact byte
   length/type, bounded-decode Graph and validate snapshot/member IDs, collection
   ceilings and full fingerprint equality. No unchecked raw Value may stand in
   for a Graph authority. No data beyond the bound is decoded or allocated by
   the payload fetch.
5. Restore EXACT prior connection limits on success, capacity, wrong type,
   decode/integrity error, SQLite error and cancellation. A restoration failure
   is a hard internal state failure; do not silently return verification success.
   Use a scope guard/finally design that also attempts restoration on unwinding.

Oversized row/SQLite host capacity is typed Capacity. Wrong type, invalid stored
JSON and identity mismatch are hard SourceIntegrity/Authority. Known cancellation
and deadline remain typed. Unknown read failure is static Internal, with no raw
SQLite/parser/source text. Expected detail for actual payload read failure is
saved_graph_read; graph byte overflow uses graph_json_bytes. NEVER
UnsupportedPolicy or a fresh fallback budget/deadline.

This changes no generic Store API, public JSON mode or host profile. Snapshot
and capture-proof authority remain independently required; the verifier does
not certify any uninspected stored authority reader.

## Draft tests and causal qualification

Eight drafts are staged, seven ordinary plus one ignored64MB-row fixture.
All begin with a genuine small Engine capture/index and successful verified
baseline before altering owned SQLite rows. Corrupt rows are explicit negative
authority inputs, never resealed graphs or checked-proof constructors.

- Lower host limit16384 survives a valid read exactly.
- BLOB storage is rejected even though graphs.value has TEXT affinity.
- Invalid saved JSON containing a sentinel yields a typed hard/static error;
  real compatibility-auto entry also fails without exposing the sentinel.
- SQLite authorizer denies the separate payload SELECT after two value reads
  for length+typeof metadata. This exercises the actual I/O-error exit and exact
  limit restoration, rather than a numeric helper.
- The same payload-stage boundary sets inherited host cancellation; result must
  remain Cancelled and restore limits.
- A transaction from another Store connection is refused before it can serve as
  this Store's authority read.
- WAL writer commits a shorter corrupt row between metadata and payload SELECT.
  The current explicit read transaction must see its original valid snapshot;
  a NEW transaction must observe hard corruption. The writer must actually fire,
  otherwise this is fixture failure rather than isolation evidence.
- Qualified64MB-row case uses SQLite CAST(zeroblob(64000001) AS TEXT) AFTER a
  genuine publication. No64MB Rust source/Graph/string is manufactured. Byte
  extent is asserted with BLOB length. A payload-read tripwire must remain
  untouched: graph_json_bytes must arise before SELECT payload/decode. The same
  oversized saved row must fail through real compatibility-auto without retry,
  and a stricter16384 host limit must survive SQLite's own capacity refusal.

The authorizer counts value-column read expressions, not output bytes. The
explicit preflight pattern has length+typeof before a separate payload query;
the valid baseline qualifies that schema. These witnesses do not measure all
SQLite/Rust heap allocations. Independent review must verify bounded fetch and
actual Run call-site coupling. A different SQL implementation needs an equally
causal preallocation witness, not weaker generic Err assertions.

Read transactions, authorizers, writer connection and corrupt row updates are
restricted to private TempDir SQLite fixtures. WAL/constructor/setup failure is
never a passing skip or a feature guard RED. The64MB-row case remains ignored
until root reserves>=512MB RAM/1GB disk and chooses its command. No measured
performance result or capacity certification is claimed.

## Root promotion

Root declares private mod retrieval_saved_graph, promotes its unavailable stub
and tests, observes causal unavailable RED, then releases functional work.
The new Run owns invocation in its real read transaction. Direct verifier tests
alone cannot prove call-site protection; compatibility-auto corruption tests are
required. Original369292/18e8 manifests remain intact and referenced separately.
