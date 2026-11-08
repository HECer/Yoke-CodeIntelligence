# A partition tests and unavailable type draft

Ignored staging only. No normal source/tests were modified. No Cargo, Node,
feature test, provider, live service or benchmark was run. Rustfmt successfully
parsed these files; this is not a compiler or behavior receipt. Root controls
promotion, causal RED, functional release and the full frozen Windows matrix.

The accepted CONTRACT.md/API.md/FIXTURES.json remain unchanged. No upstream
ordering, quality, performance, broader ecosystem or full-contract certification
is claimed. Existing archive/refactor files remain frozen.

## Staged files

- retrieval_scopes.rs: sealed run/partition/handle, immutable descriptor and
  verified-data types, static typed error taxonomy, signatures and pure getters.
  Every constructor, probe, source/document cost/read/admit and binding/lookup
  body returns Internal("implementation_unavailable"). No authority check
  returns success, creates a dummy CapturedSourceScope or silently falls back.
- retrieval_scopes_tests.rs: 28 private-module tests using actual captured
  npm/Cargo/pnpm source. They never construct checked types or forge source
  sizes. Pure descriptor getters are exercised only after a real positive
  begin/probe. Negative assertions match specific categories/outcomes; a generic
  unavailable Internal error cannot satisfy them.
- monorepo_scopes.rs: three public retrieval integration drafts for real scope
  descriptors, physical-root deduplication and exclusion retention, original
  anchors after live changes, and explicit malformed-source policy privacy.
- PROMOTION.json: intended normal destinations and ordered gates. No file is
  promoted by this directory or its manifest.

## Shared opaque boundary

The concrete lifetime adjustment is begin<'store: 'input,'input,'deadline> and
ScopeHandle<'partition,'store>, whose private owner is the borrowed partition.
The exact Store instance is retained privately. Callers receive immutable
snapshot()/graph()/deadline()/output_limit()/usage() getters, without a Store
accessor or ledger reset/refund API.

source_cost(path) returns SourceCost with source_bytes, incremental_bytes and
is_cached getters. read_source(path) returns an owned Clone VerifiedSource with
private path/hash/Arc<str>; cloning data grants no source access or budget.
document_cost(&Entity) and admit_document(&Entity) bind full original graph
metadata and loaded source. VerifiedDocument owns the verified source and an
original Entity reference, with checked body range. Returning owned immutable
data avoids holding a mutable-run borrow across later source admissions.

RunUsage is immutable Copy data: marker_files_checked, marker_bytes_charged,
body_input_bytes_charged, input_bytes_charged, document_bytes_charged,
selector_work, ownership_bytes, verified_path_count. B separately reports whole
physical admitted source bytes/counts, query exclusions and omissions.

All hard-error payloads are &'static str. Display includes a static category and
code; it does not carry raw parser diagnostics. C can use
RetrievalHardError::OutputCapacity("compatibility_envelope").into() for final
definition-conversion admission. It must not catch/restart B's auto driver.
Malformed manifest source is UnsupportedPolicy; failed proof, source hash,
scope, cancellation/deadline and capacity remain hard. B's driver alone owns
same-run global fallback using this cache, ledger and borrowed deadline.

## Literal coverage and causal ordering

The first private positive test requires a genuine constructor, partition,
physical roots, exact path-derived IDs and original borrowed inputs. Root must
observe its unavailable RED before releasing any authority-bearing body.
The other tests are deeper future gates, not independent RED evidence until
their setup reaches the assertion under review.

The real-capture cases cover:

- P1's five physical roots/seven scopes; mixed Cargo/npm one share; duplicate
  names, deepest ownership, nested virtual barrier and retained exclusions.
- Cargo root membership/exclude/default-members/path dependencies; npm array
  and exact packages-object forms; nearest same-ecosystem workspace; pnpm
  negative-final order independence and compatible duplicate declarations.
- Component-exact selector matching, final **, case, missing members,
  unsupported selectors/roles/repository hints/YAML features.
- JSON/YAML/TOML source syntax/schema, duplicate keys and malformed-source
  outcomes; later bad blob wins over earlier malformed source.
- Foreign Store even with equal workspace ID; foreign equivalent-partition
  handle; full snapshot/graph change; changed proof; stale partition against a
  genuinely published new capture consumed through the ORIGINAL Store.
- only_dirs, ignored membership and explicit read-only dependency authority.
  The ignored-grant fixture explicitly chooses existing analyzer epoch9.
- Per-canonical-path charging even for shared CAS, cached marker zero incremental
  debit, exact original Entity bodies and overlapping document debits.
- Exact1MB/4MB marker bytes and +1; raw1024/1025 paths even for shared CAS;
  exact256/257 scopes; raw2048/2049 duplicate selectors; 512/513 selector bytes;
  256000/256001 aggregate selector bytes; 256/257 package-name UTF-8 bytes;
  depth64/65 and nodes20000/20001.
- Selector work: actual255-package fixture with64 long unique selectors fits;
  128 exceeds16M below every other relevant input limit.
- One already-spent4MB malformed probe plus28MB source reads reaches32MB;
  the next1MB source refuses. C separately owns the true auto-entry control
  with29MB global input and an independent no-probe global result.
- After a successful probe, inherited cancellation is set before read/admit;
  typed Cancelled must preserve usage and the exact borrowed deadline.
- Repeated partition ID does not depend on caller output budget, and probing
  does not change any logical SQLite table rows.

Depth counts root container as1. Parsed nodes count every scalar/container value,
including root, while key spellings count toward source bytes. The nodes fixture
is root object + name scalar + padding array +19997 scalar elements=20000.
This mechanical convention is explicit rather than inferred from a decoder.

## Remaining proof obligations

These drafts do not yet exercise every ceiling causally. In particular, the
100K path/entity,1M observation,64MB graph JSON,4096-byte canonical path and
16MB ownership-map exact+1 checks need additional bounded production guard tests
and/or genuine published large captures after the first setup gate. A mutated
unpublished graph or forged snapshot is an authority test, not capacity evidence.
The owned parse preflight must enforce limits before allocation; source syntax
cannot turn a resource refusal into UnsupportedPolicy.

B owns literal scheduler/ranking/graph arithmetic, actual32MB source/document
admission and output assembly. C owns strict native pre-select decoding,
legacy inventory/schema bytes, actual auto fallback and final output-envelope
admission. Their tests do not replace A's source/proof checks. Final independent
review must verify all guard call sites and no-reset fallback, and root must
fill the listed capacity proof gaps before certifying this owned v1 slice.
