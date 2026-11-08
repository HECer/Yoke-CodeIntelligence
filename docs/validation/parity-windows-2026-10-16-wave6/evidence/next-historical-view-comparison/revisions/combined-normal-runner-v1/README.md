# Prepared combined normal H18 and Windows acceptance

**PREPARED AND UNEXECUTED.** Final R5 library/CLI promotion and its final combined normal map are not supplied by this tooling. The root scheduling decision SHA `63f0b106fa4b0696d0b9612b244c57f0dc749804328b043a2447fbd85d2c97cf` requires those steps first. Old `normal-root-runner-v1` artifacts remain frozen and are checked unchanged by the new admission helper.

The difference from that earlier preparation is limited to final context admission and H18 disk scheduling. The helper replaces the fixed R3-only map/count/receipt with **explicit caller pins plus a separately SHA-pinned root review context**. H18 retains exactly the same 51 names, six targets, build jobs, test threads and selection. With limited G: storage it uses the original three two-target batches and only `acceptance_wave4.py`'s existing plain-file, exact-name EXE/PDB cleanup after a completed batch (and before the first batch). It never deletes a tree, cache, arbitrary artifact or fingerprint. No cleanup or quarantine operation is invoked during preparation.

The full wrapper retains the original five phases (`format`, `clippy`, `python`, `typescript`, `native`) and `finalize`, using unchanged runner SHA `b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15`. Its original per-executable inventories, all integration targets in two-target batches, three C: ACL fixtures, ignored-test treatment, Cargo arguments and phase coverage remain intact. No new counts are estimated or inherited from image gates, H18 or historical acceptance.

## Final admission, supplied and reviewed by root

Root freezes the final map and completed promotion receipt after both features plus CLI are applied. Root then records the separately reviewed whole-own-package fingerprint invalidation for the final normal context, **before the image-to-normal Cargo switch**. Root must keep exclusive Cargo ownership and the normal source context through H18 and every acceptance phase. These wrappers only verify the retained plan/receipt and never invoke quarantine. They do not treat dependency metadata as binary provenance proof.

Root supplies seven explicit arguments, identical for both scripts:

```text
--expected-map target/parity-validation-20261015/<final-map.json>
--expected-map-sha256 <reviewed-final-map-sha256>
--promotion-receipt target/parity-validation-20261015/<completed-promotion.json>
--promotion-receipt-sha256 <reviewed-promotion-receipt-sha256>
--reviewed-context target/parity-validation-20261015/<root-reviewed-context.json>
--reviewed-context-sha256 <reviewed-context-sha256>
--tag-prefix combined-<fresh-lowercase-tag>
```

Every input path is a normalized repository-relative path contained in `target/parity-validation-20261015`. Freeze copies of cache plans/receipts there when their original retained location is elsewhere. Only create-only output bundles under that evidence root and fixed fresh D:/C: fixture roots are allowed. Neither wrapper writes source/image files or a caller-selected arbitrary output path.

The closed root context schema is shown by `CONTEXT-TEMPLATE.json`, which intentionally has `approved: false` and null/placeholder values and cannot be used as an accepted context. The final reviewed context must contain:

- The exact caller `expectedMap` and `promotionReceipt` path/SHA pairs, final `sourceFiles`, and a pinned `promotionManifest`. The completed receipt must have `complete: true`, `normalFiles`, `afterMapSha256` and `manifestSha256` matching this context. These are factual promotion results, not permission fields fabricated by the preparer.
- `sourcePins` equal to the final map. Mandatory explicit entries are final `native/src/main.rs`, `native/src/lib.rs`, all three Cargo files, and the six H18 target files. Additional R3/R5 implementation pins are allowed and must match that same final map.
- `imageGates` for at least the roles `R3`, `R5-library` and `R5-CLI`. Include every positive selected-test gate needed for the reviewed promotion, not merely one convenient test per feature. Each entry binds its recorded image root, source map, before/after maps, selected-gate receipt, raw log, runner helper, image main/lib hashes, preserved origin-evidence pins and explicit origin limitations. Pin additional qualification, causal RED, format/clippy and other supporting receipts in `originEvidence`; those are retained evidence, not silently relabeled as successful test gates. The tool checks selected-gate success/source identity, unchanged recorded maps, raw/helper hashes and the Cargo manifest/target paths. It does not reread or assume an old image still has its prior bytes, nor infer binary provenance from those labels. The root review is responsible for the completeness and qualification of this historical promotion evidence.
- `normalCachePlan` and `normalCacheReceipt`, pinned frozen copies from the reviewed cache-origin tool. They must record the normal root/target, final lib/Cargo hashes, complete own-package invalidation including debug (release may also be included), zero remaining selected own-package fingerprints, unchanged source anchors and no Cargo or normal-source writes by that tool. A subset invalidation is refused. Their timing and exclusive context ownership remain root responsibilities; the wrappers do not falsely claim a retrospective lock over other processes.

## Root execution order

After independently reviewing final tooling and all concrete admission inputs, root runs:

```powershell
rtk proxy python -X utf8 target/parity-validation-20261015/next-historical-view-comparison/revisions/combined-normal-runner-v1/run_h18_combined.py <the-seven-explicit-arguments>
rtk proxy python -X utf8 target/parity-validation-20261015/next-historical-view-comparison/revisions/combined-normal-runner-v1/run_combined_windows_acceptance.py <the-same-seven-arguments>
```

Those placeholders are documentation, not executable commands. No gate has been invoked by the preparer. H18 uses a fresh `D:/Yoke-Test-Scratch/<prefix>-h18`, writes all three raw logs, exact-name results, cleanup records, full before/after Git-visible maps and a receipt to `<evidence>/<prefix>-h18`. Full acceptance uses `<evidence>/<prefix>-full`, separate `<evidence>/<prefix>-pipeline`, fresh `D:/Yoke-Test-Scratch/<prefix>-full-<phase>` roots and `C:/Users/Public/Yoke-Test-Scratch/<prefix>-full-acl`. It revalidates the complete H18 raw/name/source/context closure first. All source and context pins remain checked before and after gates. Any evidence or fixture collision requires preserving prior results and preparing a fresh tag; no automatic retry or overwrite occurs.
