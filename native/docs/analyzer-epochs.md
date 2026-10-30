# Bounded analyzer epoch migration

The active native analyzer remains `yoke-tree-sitter-8`. Explicit `AnalyzerEpoch::V9` selects additional syntax-local PowerShell, Verilog and Common Lisp rules with its own producer and cache identity. Grammars remain pinned at their existing versions. This bounded policy does not certify universal language parity or compiler/runtime binding.

## Explicit V9 language rules

PowerShell ASCII command names use case-insensitive candidate lookup. Value variables, including expandable-string variables, remain distinct from function references; verbatim string text creates no variable observation. Explicit `$function:Name` and `${function:Name}` references can point to a lexical function candidate. Invoking a variable keeps a dynamic unresolved call rather than treating the variable's spelling as a function name. Unicode case equivalence, aliases, modules, scope-drive mutation and dynamic dispatch remain unproven.

Verilog task declarations become function-like structural entities with procedure scopes, alongside existing function declarations. Tasks retain their original source spans and `task` syntax in their source signature. This does not establish elaboration, package import, overload, macro, signal/port or simulator semantics.

Common Lisp `flet` and `labels` declarations produce local function entities. Function lookup is separate from value lookup. FLET declarations are visible in the enclosing FLET body but not in their own definitions; LABELS declarations are visible recursively. Escaped symbol spelling preserves case; ordinary ASCII symbols use case-insensitive lookup. Function designators remain references rather than calls. Known ordinary required, `&optional` and `&rest` lambda lists filter impossible positional-argument candidates. Keyword, destructuring and reader-dependent lambda lists remain unknown and receive no invented arity. Package/readtable semantics, macro expansion, optional initializer evaluation, method dispatch and dynamic execution remain outside this bounded policy.

These rules only change explicit V9 extraction. V8's existing helpers, serialized extraction data and whole-graph golden fixtures retain their original behavior. New optional resolution metadata is omitted entirely from V8 serialization.

## Original data and replay

The authentic fixtures in `native/tests/fixtures/analyzer8` preserve original serialized snapshots, whole graphs, extraction caches, capture proofs, source hashes, editor overlays, guarded derived snapshots, graph-view archives and V1 federation receipts. They were captured from the existing analyzer-8 implementation; the original Windows executable and original stores are also preserved outside the repository. Tests compare entire extraction and graph serialization against those saved originals. The original executable separately captured 172 extension classifications; analyzer-8 fallback classification is frozen independently of future backend catalog changes.

`analysis::extract_at` and `assemble_at` take an explicit epoch. Scoped resolution and embedded source units use the selected producer throughout extraction. Assembly rejects mismatched or unknown analyzer producers. `AnalyzerEpoch::cache_key` retains the released V8 tuple exactly and separates V9 entries. Existing V8 derived IDs retain their original tuple; V9 derived IDs include the producer epoch. No historical graph is retagged or rewritten.

`capture::analyzer_epoch_for_snapshot` obtains replay semantics from exact capture proofs and immutable snapshot identities. Default, configured, observed-ignore, derived and editor-overlay paths are covered. An unsupported custom producer receipt cannot fall back to the active analyzer when its graph is missing. Existing archival graphs remain archival data; this does not claim an available extractor for historical analyzer 4, 6 or 7.

Typed internal reader variants admit an explicit V9 constructor's own source proofs without changing the default active-V8 policy. `editor::capture_at` binds the epoch into the overlay ID and uses the owned explicit Engine constructor. Version selection is not an MCP argument or a source-controlled execution permission.

## Federated sources

V1 signed manifests, receipt bytes, archive bindings, remote IDs and replay IDs remain unchanged. V1 never recorded the receiving analyzer. Historical provenance therefore still reports `receiverEpoch: not_recorded`; compatibility is reproducibility under the frozen V8 rules, not attestation that an old receiver executed that analyzer. Unknown or nonreplayable historical graphs cannot become canonical facts through producer labels or extension matches.

New explicit V9 imports use the separate strict record kind `native_federation_receipt_v2`:

```json
{"schemaVersion":2,"receiverAnalyzer":"yoke-tree-sitter-9","receipt":{"...":"strict original receipt core"}}
```

Its `federated-source-v2` ID binds the declared receiving epoch. The source replay key continues to bind the original scope, sender and nonce. Repeating an already accepted V1 envelope through a V9 request returns and validates the original V1 remote ID and leaves its bytes unchanged. Unknown schemas, unknown epochs, widened V1 fields and conflicting receipt versions fail closed.

Public federated graph and symbol search reconstruct the whole canonical graph from the signed immutable sources using the receipt-selected policy. Matching an unsigned stored graph hash alone is insufficient. Archive validation shares this source reconstruction; the archive bridge's internal storage loader does not itself grant canonical graph authority. Import, archive, retention and public reads share the same final assembly and coverage contract.

The strict retention validator verifies V1 or V2 receipt identity, signature, original source membership, bounded exact CAS contents, inline graph hash and whole reconstructed graph hash. It returns only verified source CAS hashes. Root retention tracks validated foreign search namespaces separately from native snapshots. Preservation does not create a current host grant, publish a foreign snapshot as native source, or authorize executing source code.

## Host-selected captures and migration boundary

The saved-source host entry is `capture_with_options_at(epoch, root, store,
options, deadline)`. `Engine.capture_epoch` defaults to active 8 and selects new
disk captures, extraction and parse-cache keys together. Default, configured and
explicit ignored-source captures preserve their original V8 identities; explicit
V9 captures have separate identities and producer proofs.

The CLI accepts `--analyzer-epoch 8|9` for indexing, ordinary queries, Git and
graph workflows, freshness and session commands. For example:

```sh
yoke --root /project --analyzer-epoch 9 index
yoke --root /project --analyzer-epoch 9 serve
yoke --root /project --analyzer-epoch 9 session --id experimental --requests requests.json
```

The eight-tool query protocol does not accept an analyzer argument. Historical
snapshot selection recovers the policy from the exact capture proof. Ordinary
guarded changes retain their base snapshot's proven epoch; editor overlays use
the selected host policy. Disk recapture uses
`capture::capture_epoch_for_snapshot_with_deadline` to follow the validated
source-acquisition lineage. Scope-only derivatives may retain arbitrary producer
IDs; their inherited capture scope does not attest an extraction epoch. A derived
or overlay graph extracted with V9 from a disk-V8 base therefore reacquires disk
source with V8 while its independently proven extraction replay remains V9.
Verified Git source receipts select their own recorded capture policy, including
through later scope-only descendants. Unknown acquisition roots, foreign scope
lineage, malformed receipts and exhausted origin chains are refused. The strict
`analyzer_epoch_for_snapshot` extraction selector is unchanged. Freshness refuses
a changed host policy until an explicit index establishes that policy's baseline,
and does not publish its observed source capture as an index.

Sessions pin the host epoch across all registered projects and reject a resume
or mutable-engine drift under another policy; original V8 session bytes remain
unchanged. See [session analyzer policy](session-analyzer-epochs.md). HTTP target
profiles do not yet expose this selector; the CLI rejects the global flag for
commands whose independently configured target would ignore it.

Git captures have a distinct producer proof. `git_workflows::validated_source_epoch` and `capture::analyzer_epoch_for_snapshot_with_deadline` verify the exact immutable manifest, inherited capture scope, commit object, complete committed tree, included blob contents/modes and original epoch-bound Git snapshot identity before selecting an installed extractor. Both install the caller's bounded deadline. The original `native_git_source_v1` receipt schema and `native-git-source-1` identity tuple remain unchanged; that tuple already includes the analyzer. Explicit historical labels 4 and 6 remain readable without claiming an available extractor.

The host-only `capture_commit_at` API selects V8 or V9 extraction and classification while retaining the verified origin's path policy. Git comparisons and worktree cache seeding use the engine's selected policy for working, base and head captures. Default Git workflows still use active8; explicit9 never compares a newly captured9 working graph against an implicitly captured8 base. Existing Git snapshots retain their recorded proof-selected policy.

Offline capture chains use a distinct `git` proof node rather than interpreting Git captures as guarded patches. This strict node preserves the original receipt, inherited scope binding, exact commit object and exact committed source bytes. The included bytes let a later guarded derivative validate an older Git ancestor after the original workspace is gone. Every node rechecks SHA-256 content bindings and Git object/tree identities. The existing 64 MB serialized capture-proof budget and collection payload limits still apply; this is not a claim that peak memory is 64 MB. Native8 and explicit9 graphs are rederived from current archived sources; known historical Git graphs remain explicitly `historical_host_registered_unreplayed`.

The global version must stay on 8 until all producer paths and historical readers have been independently reviewed, including explicit handling or refusal of Git/custom graph recapture. New language semantics must branch on the selected extraction policy and keep V8 fixture replay byte-identical. Future epochs require their own explicit policy and proof changes; they are not accepted by a wildcard reader.
