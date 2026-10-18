# Opaque API and ownership sketch (not compiled source)

This sketch implements CONTRACT.md's owned semantics; it grants no release to
write normal Rust files. Names may be mechanically adjusted for Rust lifetimes,
but authority, outcomes, counters and fallback behavior must not change.

## A: retrieval_scopes.rs and its tests

```rust
// All fields private, no Deserialize or caller construction.
pub(crate) struct CheckedRetrievalRun<'store, 'input, 'deadline> {
    // Exact borrowed Store instance; checked snapshot+graph references/hashes;
    // held captured read scope; shared borrowed Deadline; path-keyed verified
    // source cache; one typed debit ledger. No exposed reset/refund constructor.
}
pub(crate) struct CheckedScopePartition<'store> {
    // Store instance binding, workspace/snapshot/graph/source-scope binding,
    // canonical marker path+hash bindings, policy/partition IDs,
    // immutable scope descriptors and complete bounded ownership index.
}
pub(crate) struct ScopeHandle<'partition> {
    /* private immutable owner-partition borrow + ordinal */
}
pub(crate) struct CheckedScope { /* private immutable descriptor */ }
pub(crate) struct PolicyFallback { /* static code + bounded diagnostics */ }
pub(crate) enum ProbeOutcome<'store> {
    Ready(CheckedScopePartition<'store>),
    NotApplicable(PolicyFallback),
    UnsupportedPolicy(PolicyFallback),
}
pub(crate) enum RetrievalHardError {
    InvalidRequest, Authority, SourceIntegrity, DeniedSource,
    Cancelled, Deadline, Capacity, OutputCapacity, Internal,
}

// Begin once per auto/explicit-balanced invocation. Validates the real Store
// workspace, full immutable snapshot and graph, and captured source scope.
// Check initial deadline/request validation before expensive work.
fn begin<'s, 'i, 'd>(
    store: &'s Store, snapshot: &'i Snapshot, graph: &'i Graph,
    deadline: &'d Deadline, output_limit: usize,
) -> Result<CheckedRetrievalRun<'s, 'i, 'd>, RetrievalHardError>;

fn probe<'s>(
    run: &mut CheckedRetrievalRun<'s, '_, '_>,
) -> Result<ProbeOutcome<'s>, RetrievalHardError>;

impl CheckedScopePartition<'_> {
    // Reject a different Store instance even when its workspace ID matches.
    // Recheck immutable snapshot/graph/source-scope binding before consumption;
    // getters do not substitute for this validation.
    fn validate_for(&self, run: &CheckedRetrievalRun<'_, '_, '_>)
        -> Result<(), RetrievalHardError>;
    fn id(&self) -> &str;
    fn physical_package_count(&self) -> usize;
    fn scopes(&self) -> impl Iterator<Item = &CheckedScope>;
    // Only actual checked captured paths; never normalize/accept a new root.
    fn owner(&self, canonical_captured_path: &str)
        -> Result<ScopeHandle<'_>, RetrievalHardError>;
    // Reject a handle obtained from another partition; no panic/index trust.
    fn scope(&self, handle: &ScopeHandle<'_>)
        -> Result<&CheckedScope, RetrievalHardError>;
}
impl CheckedScope {
    fn id(&self) -> &str;
    fn root(&self) -> &str;                 // "." or captured canonical root
    fn kind(&self) -> ScopeKind;            // package / workspace/root unassigned
    fn labels(&self) -> impl Iterator<Item = &PackageLabel>;
    fn declarations(&self) -> impl Iterator<Item = &ManifestBinding>;
    fn memberships(&self) -> impl Iterator<Item = &WorkspaceMembership>;
}
```

Scope ID is `identity(("yoke-retrieval-scope-1", workspace_id,
snapshot_manifest_hash, canonical_root))`. Partition ID is the identity of the
following canonical image: schemaVersion=1, producer=`yoke-retrieval-scopes-1`,
workspaceId, snapshotId, snapshotManifestHash, graphHash, markerBindings,
scopeDescriptors, ownership, diagnostics and diagnosticCounts. Arrays sort by
marker path, scope root, owned path and diagnostic `(path,code,selector)`;
labels/memberships sort by ecosystem and declaration path. Marker bindings are
exact path/hash pairs; descriptors contain ID/root/kind, labels, declaration
bindings and workspace memberships, with no runtime pointer. Ownership contains
every checked `(path,scopeId)` pair. The diagnostic image uses a fixed canonical
prefix of at most64 rows/256,000 serialized bytes plus exact totals, computed
independently of the caller's response budget. The actual returned prefix may
be shorter; response clipping does not change partition ID. All marker bytes/
selectors remain bound by marker hashes even when a diagnostic is omitted.
The policy version fixes the computation.
These IDs are correlations, never source-access capabilities. Runtime Store and
validated source-scope bindings remain private and are rechecked independently.
Names are data, not keys. No
getter returns mutable authority fields or an owned Value that can be fed back
as a checked partition. Source cache values are immutable verified path/hash/text
entries; cache reuse does not grant new paths. Public descriptors are bounded
data and cannot reconstruct authority.

The run owns methods to read/debit markers and admit/read whole source units,
to debit complete document units, to charge selector/ownership work and to build
bounded metadata. C cannot create a second ledger after probe. `probe` may only
classify typed manifest syntax/schema issues as UnsupportedPolicy; observed
authority/integrity/capacity/deadline/internal errors propagate unchanged.

## B: retrieval.rs + private retrieval_balanced.rs and scoring tests

```rust
pub(crate) enum NativeScopeMode { Global, ManifestBalancedV1 }
// Pure shared preflight. C calls this before select/index; find repeats the
// same decode. No auto JSON value or permissive null/non-string default.
pub(crate) fn native_scope_mode(args: &Value)
    -> Result<NativeScopeMode, RetrievalHardError>;

// Existing public find signature remains; strict native scopeMode decoding is
// added. Absent/global strips only that recognized field and uses the original
// no-probe global path with unchanged bytes.
pub fn find(store: &Store, snapshot: &Snapshot, graph: &Graph,
            args: &Value, deadline: &Deadline) -> anyhow::Result<Value>;

// Crate-private host-selected entry, not a JSON "auto" mode.
pub(crate) fn find_compatibility_auto(
    store: &Store, snapshot: &Snapshot, graph: &Graph,
    args_without_scope_mode: &Value, deadline: &Deadline,
    output_limit: usize,
) -> anyhow::Result<Value>;

// Internal driver owns probe dispatch. C must not catch errors and call find
// again. This path alone performs allowed same-run global fallback.
fn rank_balanced(run: &mut CheckedRetrievalRun<'_, '_, '_>,
    partition: &CheckedScopePartition<'_>, options: &CheckedRetrievalOptions)
    -> Result<Value, RetrievalHardError>;
fn rank_global_after_probe(run: &mut CheckedRetrievalRun<'_, '_, '_>,
    options: &CheckedRetrievalOptions, fallback: &PolicyFallback)
    -> Result<Value, RetrievalHardError>;
```

Extract pure whole-unit scheduling and pure scored-row fusion inside B's private
module so their literal fixtures can exercise actual production functions.
Production units/rows originate only in the checked run. Tests may supply plain
cost/order rows to these pure arithmetic functions; such rows cannot publish or
read sources. Keep source admission, document admission and ranking separate;
do not compute fixture expectations through these helpers.

Scored rows preserve original Entity references/IDs and source hashes. No body
normalization, invented ranges or package-label anchor is allowed. Keep graph
arcs canonical and repeated; do not mutate the immutable Graph. Do not change
legacy global floating-point accumulation order while factoring helpers.

## C and root

C owns `trail_adapter.rs`, `protocol.rs` and profile tests. Session ci_retrieve
adds the exact native mode enum and prevalidates it before select. Ordinary
native tool inventory remains eight. Trail/Graft uses `find_compatibility_auto`
then retains current definition conversion and its final output admission.
Compute the supplied output ceiling from the actual envelope budget; no new
public query parameter is needed. Descriptions disclose auto selection and the
intentional additive metadata. Six-tool argument schemas do not change.

Root owns lib module registration, release/test promotion, eventual docs/evidence
and centralized Cargo. Suggested tests: A `monorepo_scopes.rs`; B
`monorepo_admission.rs`/`monorepo_retrieval.rs`; C `monorepo_profiles.rs`.
One owner per normal file. Independent review checks the actual boundary and
literal oracles before the frozen full matrix.
