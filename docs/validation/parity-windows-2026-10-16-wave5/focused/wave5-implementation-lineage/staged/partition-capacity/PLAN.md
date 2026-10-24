# Capacity proof follow-up

Ignored drafts only. No normal file was changed; no Rust test, Cargo, Node,
service or performance experiment was run. The accepted partition package at
MANIFEST18e8d83be0a8985bb0d711e6321e26d6a6e410961f08d3df698a80b3caf0c2d4
remains immutable. Rustfmt parsing and independent byte arithmetic are the only
performed checks. These are proposed future tests, not acceptance receipts.

## Actual existing boundaries

The accepted CONTRACT/API ceilings remain100000 snapshot paths,100000 graph
entities,1000000 observations,64000000 streamed graph JSON bytes,4096 canonical
path UTF-8 bytes and16000000 ownership charge bytes. Entity count is whole-graph
metadata, before file-entity exclusion/query filtering; it is not a promise to
score100000 documents.

- capture.rs508 already limits a validated captured manifest to100000 paths.
  Native disk capture at1664 refuses its100001st accepted file before publication.
  Its current code opens/reads that file at1642 BEFORE the count check. Therefore
  this inherited guard must not be described as refusing before every file open.
- paths.rs15 already limits normalized syntax to4096UTF8 bytes. A4097-byte path
  cannot become a valid native captured authority through the existing path
  admission. A standalone syntax success is not source authority.
- analysis.rs336 emits one file entity per source. Symbols at428 emit contains
  observations, so a small source with many declarations would encounter graph
  bytes well before100000 entities. Empty files avoid that dominance.
- Every current Observation requires id,snapshotId,source: three64-byte IDs.
  Even empty other strings,target=null,evidenceIds=[] require321JSON bytes.
  One million rows plus commas/brackets require at least322000001bytes, ignoring
  the entire rest of Graph. Exact1M therefore can NEVER be a Ready whole graph
  under64MB. Actual current producer evidence makes the real graph much larger.
- Store.publish currently serializes a complete Graph to a SQLite string.
  These tests do not claim to bound the caller's existing Graph construction or
  capture/index memory. The new guard must bound ADDITIONAL retrieval work.

No earlier ceiling is raised, bypassed or hidden to exercise another limit.
No Store.publish of a handcrafted graph, resealed manifest, forged source size,
unchecked source capability or mutated authority object appears in these drafts.

## Required production order and static failure taxonomy

Only after root's causal RED release:

1. Check the original borrowed deadline and request. Reject cheap source/entity/
   observation count ceilings and path-byte/syntax ceilings from supplied
   metadata before extra Graph loading, serialization buffers or source reads.
   These preflight failures return only hard errors; they grant no authority.
2. Feed the supplied original Graph to the actual fixed64MB streaming
   fingerprint writer. Check typed capacity/overflow BEFORE accepting a write;
   retain a typed stop cause separate from serde's I/O/error text. No to_vec,
   Value clone or post-serialization size check can replace this boundary.
3. Validate exact persisted snapshot/graph/Store/captured scope using admitted
   identities. Only then interpret safely loaded captured marker source.
4. Use the nearest-ancestor scope index and charge path bytes+owner-root bytes+32
   with checked arithmetic BEFORE map insertion. Its16MB charge is a logical
   ceiling; actual allocator overhead is not claimed to equal16MB.
5. A hard count/byte/map refusal stops the real driver before source-body scoring,
   fallback or another run. Budget/deadline errors never become source syntax.

The proposed mechanical hard detail codes are graph_entities,
graph_observations,graph_json_bytes,ownership_bytes and (defensive unreachable
native-overlimit inputs) snapshot_paths,canonical_path_bytes. All remain
RetrievalHardError::Capacity. Cancelled/Deadline/Internal remain distinct.
Unknown serializer failure becomes static Internal("graph_serialization"),
without raw custom/parser contents. No error-string classification is allowed.

## Draft tests and independent oracles

retrieval_capacity.rs is a required private generic serialization boundary,
explicitly unavailable. It returns a digest only, never a checked run or source
grant. A later implementation must actually call it from begin BEFORE the
stored Graph read; independent review must verify that call site.

retrieval_capacity_tests.rs contains7 ordinary drafts:

- Exact64MB lazy JSON serialization uses a1000-byte constant chunk and at most
 1002bytes of tail, without allocating a64MB string or node vector. Literal
 length=n*1003+tail+4. Independent Python incremental SHA256 pinned
 ec27c9e8186e18955923e326ec81571b342e471d31277a4a57695fd87911c858
 for63808chunks+572tail bytes. Production never calculates this expectation.
- Exact+1 returns typed graph_json_bytes capacity.
- A poison serializer AFTER byte64000000 must never be invoked. An eager
 to_vec would reach it and return the wrong error. This tests real streaming
 stop behavior, rather than mirroring a numeric checked_limit helper.
- Deliberate serializer failure is static Internal, never policy fallback.
- A genuine Engine-saved small Graph has the same digest as model::identity.
- A genuine observation demonstrates the schema lower bound/dominance. This
 is not a passed1M count guard test.
- Existing canonical syntax accepts a literal4096UTF8 path and refuses4097;
 it explicitly grants no source authority.

retrieval_scopes_capacity_tests.rs contains one small saved-positive control and
five ignored qualified real integration drafts. It is loaded inside A's private
module, so types remain sealed. SQLite's already-enabled rusqlite hooks provide
an actual I/O tripwire: count+allow on the valid control must observe graphs-table
access; deny+count on the real early-refusal input must observe ZERO accesses and
the specific capacity code. No production test hook/environment override is
introduced, and the database authority remains genuine. This observes extra
stored-graph loading, not every allocation in caller/capture code.

## Reachable genuine fixtures

| Guard | Real positive control and negative | Dominance and causal witness |
|---|---|---|
|100K source/entity counts|99999 empty extensionless short files plus one empty p.rs give100K paths/100K file entities. Change p.rs to one real fn: same100K paths,100001 entities. Actual Engine capture/index only.|Modeled entity array~42.9MB, no baseline observations; actual JSON<64MB is a required fixture precondition. Valid begin must pass equality. Entity+1 must return graph_entities before saved-graph access. Adding a100001st source instead proves EXISTING capture refusal and no new snapshot publication; not a fake retrieval proof.|
|64MB Graph|One real empty-file entity calibrates immutable Entity/header byte lengths. Create64000 files at relative path119/120bytes, then use one real source's endByte digit width for the remainder. Reindex normally: assert JSON exactly64000000, then64000001.|Five Entity string fields repeat path, so an added ASCII path byte contributes5JSON bytes. Three coverage counts changing1->64000 contribute12bytes. Actual JSON size is checked, not assumed. Counts stay64000, no observations, ownership below16MB. Exact+1 must refuse before saved-graph access; kernel poison proves streaming rather than eager traversal.|
|16MB ownership|Two actual package roots each190bytes;38275 empty source paths initially196bytes, with200filenames extended by1byte. Two marker paths are203bytes. Literal charge38275*418+2*425+200=16000000. Rename one more actual file byte:16000001.|Graph modeled~53MB and actual<64MB required;38277paths, entities<100K, only2package markers. Positive probe is Ready with usage exactly16MB. Negative probe is ownership_bytes with usage<=limit. Corrupt only empty BODY CAS after valid index; real balanced driver must still refuse ownership before encountering body SourceIntegrity. Marker CAS stays genuine.|
|4096UTF8 path|Qualified Windows job actually creates/captures a4096byte Unicode path plus two small package roots. begin/probe/read must return its original source. A4097byte rename is refused by existing capture.|Windows long-path creation/traversal support is a fixture qualification, never an automatic pass/skip. No valid native4097 source exists to test a downstream Ready. Syntax-only and source-authority evidence remain separate.|
|1M observations|Optional very large genuine20-file Rust job: each file has49998calls plus2contains=1000000. Calibration10calls=>12observations must pass first. At equality expect graph_json_bytes; add one real call/reindex:1000001 must return graph_observations.|NEVER Ready at1M. Both metadata counts are genuine Engine output. Zero extra graphs-table reads required on both hard refusals. Earlier constructor/SQLite size/deadline failure is qualification failure, not feature RED or count certification.|

All big cases assert their actual counts/byte preconditions before evaluating
the feature guard. A changed extractor/header model must repair/calibrate the
fixture and obtain genuine RED; it cannot weaken the capacity assertion.
The two file rename operations verify resolved parents remain inside their
private TempDir before moving. Only owned fixtures may be used.

## Runtime/memory qualification, not measurements

No durations or peaks have been measured. Planning reservations below are
conservative job allocation requests, not certified limits or performance SLAs.
The source files mostly share an empty CAS blob, while distinct paths still
produce separate capture/cache/FTS records and logical debits.

| Job | Bounded source layout | Proposed isolated root reservation |
|---|---|---|
|ordinary stream+small controls|64MB streamed in1000byte chunks; small actual Graph; no64MB output buffer|At least512MB available RAM; allow60s per stream test initially. Do not interpret that timeout as a benchmark.|
|100K paths/entities|100000 NTFS files,~43MB graph per capture, up to2published graphs; metadata/cache dominates|At least2GB available RAM/3GB free temp disk; allow several minutes. Use one isolated job and monitor actual cost.|
|64MB exact/+1|64000 files in two directories, two64MB saved graphs; source contents<=100KB|At least2GB RAM/3GB disk; several minutes. Actual serializer/header preconditions may fail before guard testing.|
|ownership exact/+1|38277files under two roots, two~53MB graphs; source bodies empty|At least2GB RAM/3GB disk; several minutes. Temp root length must keep ordinary paths supported, or qualify long paths explicitly.|
|4096UTF8 path|40nested Unicode components plus3files|Small memory/disk; qualify Windows filesystem/toolchain long-path support first.|
|optional1M observations|20files,~4MB source; current producer creates1M evidence rows too; graph JSON may approach~0.9GB; two saved versions/WAL/cache multiply cost|At least8GB RAM/12GB disk; potentially many minutes, no SLA. Root may leave UNEXECUTED if constructor budget/storage dominates. Never add it blindly to default matrix.|

The current hard source-capture per-file4MB, aggregate512MB, AST1M nodes/file
and resolution200K uses/file remain in force. The optional call fixture uses
~200KB/50000calls per file, but calibration and actual constructor receipts are
mandatory. No environment check silently returns success from an ignored test.
Root must record qualified/executed/setup-failed/unexecuted states explicitly.

## Promotion and honest closure

1. Finish current wave; root explicitly releases this separate ignored package.
2. Register the unavailable private stream module and its tests; root observes
   exact64MB positive and poison-tail RED against the unavailable body.
3. After implementation release, require all stream kernel tests and the small
   actual saved-graph/tripwire control. Independently review begin's real call
   site and checked-before-allocation/insert order.
4. Promote the real integration module only as qualified jobs. Run count,
   graph-byte and ownership pairs separately; record source manifest, exact test
   names, constructor preconditions, actual typed guard failures and resource
   observations. Keep the accepted original partition manifest unchanged.
5. Qualify long-path capture separately. Record the1M graph-byte dominance proof
   now, but leave its count runtime certification open until a genuine large
   constructor and both typed refusal tests actually execute.
6. Root decides any remaining claim boundary from actual evidence. These drafts
   alone close no capacity guard, upstream full contract, performance, Linux/macOS
   runtime or provider/quality certification.
