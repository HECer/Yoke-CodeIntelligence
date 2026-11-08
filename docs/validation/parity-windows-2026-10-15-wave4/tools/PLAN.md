# Staged wave4 preservation plan

Status: **not authorized or executed for wave4**. These files are ignored development staging only. No concrete successful wave4 full-freeze tag, source digest, runner digest or preservation plan has been selected here. Root must review this script and the eventual concrete plan before any normal-file copy.

## Intended inputs

1. An explicit canonical workspace, normally `G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence`.
2. Its explicit evidence root, `target/parity-validation-20261015`.
3. A concrete, finalized `wave4-frozen-...` tag produced only after five fresh phases succeed: format, all-target Clippy, native, Python and TypeScript/typecheck.
4. Independently reviewed SHA-256 digests of that exact source freeze and runner. Matching a receipt's own unreviewed runner claim does not approve code. Candidate runner code is parsed as AST data and is never imported or executed.
5. A fresh, nonexistent direct `docs/validation/parity-windows-...wave4...` destination. Existing destinations are never merged, repaired, adopted or overwritten.
6. A fresh direct plan file in this ignored staging directory. The plan captures every selected input hash, discovered wave4 raw attempt, destination path and tool hash.
7. A later explicit root authorization of the exact plan SHA-256 and the `--allow-new-evidence-bundle` gate. Preparing a plan performs no preservation.

Historical immutable contracts stay fixed:

| Bundle | Artifact pins | Index SHA-256 |
|---|---:|---|
| Wave 1 | 160 | `156852befe63b51cd99d66f9b9a857157659715f31ffb0a653720373677d91a0` |
| Wave 2 | 210 | `0e2e504fb30ec388ae46efe53dd4639de55926753ffc8b9b5da74fc55c71633e` |
| Wave 3 | 416 | `98646f7e98060d475129f4db26e353f37f57059658f639382888a20240208e97` |

All **786 artifact pins**, each index, and every advertised or existing index digest sidecar are checked before planning, after planning, before copying through complete revalidation, after copying and after index creation. The advertised wave3 sidecar is mandatory. Its index/sidecar are outside the 416-artifact map; they are checked separately. No old bundle is copied over or edited.

## Acceptance verification

The accepted tag must have a finalized summary and exactly five successful phase receipts. Every result must be a fresh execution, exit zero, contain zero failures, and use identical explicit source and runner identities. All phase source-before/source-after manifests must equal the freeze. `sourceChanges` and `validationErrors` must be actual empty lists. Every artifact/log hash and receipt/summary pin is checked independently.

Native verification replays raw Cargo metadata, normal default `--list`, ignored `--list`, per-executable runtime names, ignored status, selected/filter counts and executable paths. Inventory commands contribute zero runtime tests. New integration targets are derived dynamically from metadata, all batches including the final partial batch must cover them exactly once, and bins/examples/benches/doctests remain explicit. The ordinary library partition on D: must exclude exactly the reviewed ACL literal names; each such fixture must pass exactly once on C: through cached `--exact`, without rebuilding. Partitions must be disjoint and cover every registered name. The required worker remains ignored rather than being counted as passing.

Normal files are enumerated by Git tracked plus normally unignored inventory, without a fixed 803-file or other historical count. All current normal bytes must equal the accepted freeze before preservation. Python totals come from one raw summary plus the same number of unique successful result names, with no fixed eleven-test rule. Node totals come from its unique complete raw framework summary, with skipped tests preserved as skipped. Native counts and target totals come only from the accepted replay, never from focused or failed attempts.

Every other immediate `wave4-*` input is snapshotted separately: frozen attempts under `failed-attempts/`, focused gates under `focused/`, and standalone parser/review artifacts under `oracles-or-review/`. These are opaque historical bytes, never imported runner code or accepted-suite evidence. Missing/malformed raw receipts remain explicitly unverified rather than being promoted. Their counts are nonadditive. Unknown file kinds and unbounded trees fail closed instead of being silently omitted.

## Review and application

After actual full acceptance, root supplies concrete values; these placeholders are not an executable approval:

```text
python -B preserve_wave4.py plan --workspace WORKSPACE --evidence-root EVIDENCE --accepted-tag SUCCESSFUL_WAVE4_TAG --reviewed-source-sha256 SOURCE_SHA --reviewed-runner-sha256 RUNNER_SHA --destination docs/validation/NEW_WAVE4_DESTINATION --plan-out STAGING/FRESH_PLAN.json
```

Root inspects the resulting source, runner, five receipt results, dynamic inventories, historical pin counts, complete discovery/copy list, byte estimate and tool digests. Only a separately authorized invocation may copy:

```text
python -B preserve_wave4.py apply --plan STAGING/REVIEWED_PLAN.json --reviewed-plan-sha256 EXACT_PLAN_SHA --allow-new-evidence-bundle
```

Application reconstructs the entire deterministic plan and compares its bytes before the first new destination write. Every copy uses exclusive create, with input rehash and destination verification. It never deletes, moves, overwrites, cleans up or retries an existing output. An interrupted partial destination is preserved; a new reviewed destination is required. The reviewed plan is included in the bundle. Index and sidecar are created last, all copies and historical pins are rechecked, and the final normal inventory must equal the accepted manifest plus exactly the visible planned evidence additions. The final baseline read is independently bound again to the root-reviewed source digest, so a replaced manifest cannot authorize changed normal files. Ignored raw logs are still directly pinned even when Git excludes them.

## Remaining assumptions and limits

- This is a trusted local preservation tool. Hashes and `fresh_execution` labels establish evidence consistency under the reviewed local runner/storage; they are not signatures or proof against a compromised host.
- Root must finish successful receipt generation, stop other source/evidence writers and keep the workspace quiescent through plan review/application. Python path checks are repeated and reparse paths are rejected, but this script does not claim the native installer's held-handle protection against a malicious concurrent namespace retargeting actor.
- The current explicit runner command/package/known-kind policy remains strict. A new workspace package, target kind, benchmark harness, new command flags, changed Python output/skip policy or ACL partition protocol requires independent review and a verifier update; nothing is silently dropped.
- The acceptance runner still hardcodes eleven Python tests today. This preserver independently parses dynamic Python totals, but the runner itself must be separately reviewed/updated if that suite grows. No runner is edited or executed by this task.
- Bounds are 256 MiB per raw file, 20,000 files per tree/plan, 20,000 total file/directory nodes per scanned tree, 64 directory levels, 20,000 immediate evidence-root entries and 1 GiB planned payload. Directory entries are charged before sorting; empty fanout consumes the same node budget. Aggregate plan file/byte limits are charged as each copy is admitted, stopping discovery before later trees after exhaustion. There is no multi-file atomicity, power-loss durability, read-only filesystem ACL promise, deletion of failed outputs, or automatic post-freeze metadata reconciliation.
- Successful preservation records only an unreleased Windows checkpoint. It never certifies universal parity, deferred platforms, client installation coverage, release readiness or a performance SLA.

## Validation scope

Staging unit tests exercise synthetic owned receipts, hostile paths/JSON, dynamic counts, exact ACL partitions, copy/index guards in mocked-admission ignored fixtures, and read-only replay of the already secured wave3 bundle. That historical replay is parser validation, not a new execution of any of its 1,399 native tests and never wave4 acceptance. Synthetic output tests write only disposable fixtures below this ignored staging directory; they do not copy actual historical/wave4 evidence. No actual preservation or Cargo run is performed by the staging tests.
