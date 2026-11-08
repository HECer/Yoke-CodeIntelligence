# Private R5-B adversaries beyond the twelve public drafts

Test design only; none is implemented, compiled, run or a qualified Git fixture.
Use the exact CONTRACT.md as the oracle. The public twelve already cover genuine
basic hierarchy/incoming behavior, canonical bytes, lowered budgets and caller
state; these cases target deliberate shapes and corruption that those genuine
fixtures do not exercise. Do not weaken those public oracles to fit observed data.

Private tests call the real source/index/walk/output kernels with controlled
borrowed rows and literal source bytes. Their raw drafts are not checked public
results or Git publication proof. No test-only public constructor or
CheckedGitInputs bypass is needed: pure validation/walk kernels can be tested
below the authorization boundary. Tests of real SQL/CAS/policy/epoch admission
belong beside root's shared hook and require a genuine positive report first.

Use literal 64-hex IDs with deliberate ordering, independently located source
spans and a separate tiny row/witness oracle. Never compute an expectation with
the production expander, sorter, budget helper or serializer under test.

## Library-owned private cases

| Case and setup | Exact assertion / wrong implementation caught |
| --- | --- |
| Global duplicate keys/IDs | Duplicate an unreached entity key with both identical and conflicting rows; also duplicate entity, observation and evidence IDs independently. Refuse input, never silently overwrite a BTreeMap entry. Per-side identity is separate: the same IDs on base/head do not collide. |
| Generic parent integrity before eligibility | Emit a valid class whose direct parent is a captured file entity: it is a valid proof endpoint, not unsupported_owner. For an emitted/reached entity independently change parent snapshot, path or containment and require input invalid before unsupported-language/kind classification. A qualified name/path/containment match never rescues a missing parent key. |
| Scoped integrity, not a wider promise | An original method with missing/not-found parent is unsupported missing_parent; both name offsets missing is unsupported missing_name_anchor. A deliberately unreached optional parent/name defect is not newly guaranteed refusal. Global duplicate-key corruption still refuses even unreached. No parent-chain forest/AST/dispatch certification is added. |
| Name/source corruption | With exact literal Unicode/CRLF source, alter one name offset only, move it outside the entity, split a UTF-8 character, change spelling or use wrong manifest hash/size. Require input invalid. Case and NFC/NFD distinctions stay exact; no unquoting/normalization/qualified-name inference. Check source/cache membership separately for two paths sharing a hash. |
| Eligibility order and constructors | Other language (including TSX), non-method, missing parent, unsupported owner, missing both name offsets, exact source-proven constructor, then expandable: assert first applicable reason. Partial/misplaced present name anchors are corruption first. Exact constructor is excluded without an AST-kind/staticness classifier. Private/static/generic non-constructor same-name members remain candidates. |
| Joined unsupported members | A related type has two same-name overloads with valid names and one direct same-name method missing both offsets. Retain all three member reasons, two eligible expanded/walk seeds and one unsupported reason; an original-owner overload receives no hierarchy reason. A parentless method gets no fabricated related-type reason. |
| Full-lex diamond | Owner A reaches B with observation `1...`, then T with `f...`; reaches C with `2...`, then T with `0...`. Deliberately order entity IDs so C sorts before B. T's witness must be `[1...,f...]`, although its final ID is larger. Reverse every input vector and require the same canonical state/proof order. A direct length-1 edge beats any lexicographically smaller length-2 route. |
| Cycle/self-loop/parallel proof | A->B has two distinct observations; B->A and A->A exist. With depth 2, each monotone direction has A0/B1 only, each root-owner witness stays empty, and every examined edge is retained as a proof ref. No cycle back to A creates own-type member reasons. Do not deduplicate proof by visited target or by endpoint pair. At depth1, B's outgoing edges are unexamined boundary adjacency, not extra proof. |
| Both directions, one incoming origin | A type cycle reaches related member M as both ancestor and descendant for one root. Keep both canonical reason indices in one `(side,root,M)` walk seed. Two roots sharing M retain two independent incoming state families. A member that is an original root anywhere on that side consumes no expanded-new-seed slot but still supplies another root's hierarchy reason/walk. |
| Incoming walk-seed key | One caller is distance1 from expanded seed X and distance2 from expanded seed Y for the same root. Preserve both distances and full witnesses; sharing `(root,caller)` visited state fails. Add incoming self/cycle/parallel edges and preserve every per-state examination. Different sides/roots/phases also consume independent states. |
| Evidence vector and closure | A new proof observation references `[e2,e1,e2]`: output that exact order/repetition and only two unique evidence rows; all three references consume rows. Original baseline impactObservation refs alone do not copy their observation/evidence into new proof tables. Proof endpoints' entities and their captured anchors are included and validated. |
| Invalid or zero-width evidence | Missing evidence ID, wrong side/snapshot/path/hash, invalid UTF-8 endpoints and incorrect line binding refuse. Use exact released LF/CRLF/zero-width conventions, preserving original captured fields rather than normalizing them. This fixture must first qualify against the inherited source-range conventions, so an ambiguous end-line convention is not silently invented. |
| Ambiguous/resolved/unresolved relations | Some(admitted target) with ambiguous/heuristic resolution still traverses and preserves that category. Null-target hierarchy/call rows increment global unresolved counts without label lookup/source read; resolved hierarchy rows with TSX/non-type endpoints increment unsupported coverage. A Some(nonexistent ID) is invalid, not an invented unresolved fallback. Incoming calls/references have no TypeScript/method-only restriction. |
| Whole-graph scan work | No changed seeds but nonempty unrelated/null-target observations still consume exactly base.obs.len+head.obs.len edge visits; new source bytes/states/expanded seeds remain zero. Same snapshot ID on both sides still counts each side's vectors/scan. Exact edge budget succeeds and one less refuses; filtering before accounting fails. |
| Multiplicative budget state | Independently enumerate original rows, distinct roots, new unique side/member pairs, hierarchy states and `(root,walkSeed,current)` incoming states. Duplicate original row indices survive grouped roots. Lower each counter separately to its exact required total and one less; do not infer state credit from global entity-table storage. |
| Polynomial wide diamond | k levels of two types each, complete bipartite links between adjacent levels and A->level1. For k=12 at hierarchy depth12: ancestor states25, descendant root-only state1, examined hierarchy edges46. There are 2^12 paths but only26 hierarchy states and46 proof refs; initial scan adds another46 edge visits. Assert these structural counts, one witness/state and exact budget refusal, not elapsed-time-only success. |
| Recursive rows and wire preflight | Independently count every final array element, including five fixed limitations, repeated evidence/witness IDs, changed-row indices and reason refs. Objects/scalars are not row units. Charge before output growth; verify final count equals charged count. Empty output tables still need the limitation rows and wire bytes. A long captured targetLabel must refuse before cloning when output credit is too low. |
| Cooperative cancellation in work | Flip the shared cancellation flag at a deterministic bounded scan/adjacency/witness-sort/encoding boundary. Assert the producer's own deadline class and no returned draft/result, not only an outer run_bounded error. No new Deadline may reset the inherited budget. |

For cycle counts above, keep the incoming and member-expansion fixture separate
from the bare hierarchy oracle; otherwise incidental root/file/member seeds
change totals. Likewise label the wide-diamond counts as hierarchy-kernel facts,
not whole genuine-report totals. Output tests should compare all proof refs and
ordered state witnesses, not simply the set of reached types.

## Root-owned shared-hook cases required at integration

- **Both-side policy before source:** qualify a real positive result; remove an
  early base blob while denying an unchanged/unreached head path through its
  *real* captured scope. Require the head policy refusal before any inherited
  source-CAS read; bounded publication metadata CAS is a separate event. Repeat
  with equal side snapshot IDs without removing either side's policy obligation.
- **Caller graph limits before inherited load:** lower one graph cap below its
  genuine complete count and break the early source. Capacity must win before
  source opening. Preserve fixed inherited maxima and raw metadata envelopes.
- **Retained view through encoding:** after first publication SQL read, mutate a
  relevant row via a second genuine connection. Either the original retained
  report/snapshot/graph view persists or the supported boundary refuses; no mix.
  Assert the owned read guard and exact SQLite limit survive until final encoding.
- **Epoch proof:** qualify V8 and V9 genuine Git sources, plus a captured disk
  working-tree head. Report each own proven producer even if active ANALYZER or
  graph/entity labels differ. A malformed present Git receipt cannot fall back;
  unsupported V4/V6 remains input-invalid for this producer. Do not call an
  acquisition-origin epoch proof an extraction proof. Test mixed admitted sides
  independently rather than adding an undocumented same-epoch rule.
- **Caller state and compatibility:** preserve caller transaction/marker and
  exact prior SQLite length cap on success, input/capacity/deadline and consumer
  encoding error. In autocommit only the locally owned read transaction drops.
  Preserve existing load/render/digest/report/R5-A canonical bytes and original
  persistent logical/CAS state. SQL WAL bytes are not the logical-state oracle.

These are implementation responsibilities and finite oracles, not additional
approval packets or evidence that runtime RED/acceptance already happened.
