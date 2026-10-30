# Wave6 combined evidence preservation v2

This immutable sibling preserves v1 and the exact generic parser bytes at
SHA256 `35e097ea51dee414c88c68d645ad43ca6bc2d71c6335152f05107e99a22f99f3`.
The parser is still hash checked and compiled unchanged; only its global
`plain`/`read` functions and the helper's matching aliases are replaced.

The admission delta is limited to IO: each call still checks lexical containment,
strict canonical boundary identity, every ancestor's lstat/type/reparse status,
and then strict canonical identity of the last existing prefix once. An existing
full path is that final prefix. Missing tails never use non-strict Windows
resolution, which can swallow access/sharing errors. Parent traversal is refused
up front; no stat, canonical path or byte result is cached. Read admission binds
before-path, opened-fd, completed-fd and after-path identities. It requests only
`min(before.st_size + 1, MAX_FILE + 1)` bytes, retaining the unchanged 256 MiB
cap, exact-length check and one-byte growth sentinel. This avoids a 256 MiB read
allocation request for every tiny file.

The sole existing-function delta narrows `entries`: only the fixed
`directory == boundary == EVIDENCE` enumeration is checked under ROOT; ordinary
descendants retain their supplied boundary. The sealed v1 already allowed its
evidence-root enumeration; the earlier claim of a v1 boundary-self bug was
retracted. No parser, command, native count, selected gate, source map, root
control, raw classification, historical limit, output inventory, Git-ignore
partition, fsynced journal or create-only application rule changes. Additional
selected bytes are only this revision's helper/probe artifacts and the pinned
original v1 helper for review lineage.

Measured bounded evidence is retained, with deliberately separate scopes:

- `guard-probe-results.json` records 32 alternating paired calls on four actual
  shallow/deep evidence files: old guard 38.0639 ms, strict guard 24.0276 ms
  (1.584x). Its exact tested preimage is retained as `guard-only-before.py`
  (`cf60be4406464649e9bc5175f8c9522d0d7eb57b0420d4dcb05ff6f7e6fd2f15`).
  Seventeen bounded checks include actual symlink and discovered 8.3-alias
  refusal, strict missing-prefix failure propagation, identity and type checks,
  local/core alias injection and actual direct EVIDENCE enumeration.
- `read-probe-results.json` binds the final helper and that guard-only preimage.
  Twelve paired tiny-file reads (three actual paths, four alternating rounds)
  measured original core 623.5573 ms, same-guard large request 628.0615 ms and
  final bounded request 26.3081 ms (23.87x for the request-size change).
  Payload hashes match. Six bounded IO checks cover the exact request size,
  growth/shrink, oversize, completed-fd identity and empty-file sentinel.

These are per-operation observations, not full-preservation duration, total
memory or a new runtime/acceptance claim. No full v2 plan or application was run
by the preparation author. The successful root-owned v1 plan remains untouched;
root can compare its admitted selection against a future v2 plan, excluding only
the deliberate helper/tool/staging evidence differences. Probe scripts retain
fresh fixtures under this ignored directory and exclusively create their result
files; replay them only in a fresh ignored copy, preserving these originals.

This ignored preparation creates no normal files until root runs `apply` with an
explicit reviewed plan SHA256. It runs no acceptance gates, imports no acceptance
runner or orchestration code, and performs no source, cache, fixture, cleanup, or
status-metadata changes. Only the exact SHA256-pinned generic
`next-wave5-preservation/receipt_core.py` is executed as a parser.

`plan` requires actual finalized success from the five original phases, replays
their receipt/raw consistency with `validate_bundle`, checks the original H18
names and orchestration receipts, and requires the full current normal map
`11f688628aa77bf3f50c440cd954aa49e0588800aeabb838bc17d09efe9ef30f`.
Counts come from those actual receipts; there is no preset new native test count.
H18, inventory, historical failures, overlapping focused gates and duplicate R5
gate roles are retained separately and never added to full acceptance totals.
Universal parity and release certification remain false.

The single deduplicated `evidence/` tree retains the complete accepted full run,
H18 and pipeline trees, original runner and reviewed wrapper tooling, root
decision/context, every explicit context `{path, sha256}` input (including origin
records), and all direct `r3-*`/`r5-*` focused evidence trees and JSON/log files.
Four explicit control pins additionally retain the actual V4 cache helper, final
normal promotion applier, root final gate bindings and independent promotion
review. These programs are copied as bytes and never executed by this helper.
Pinned JSON inputs are opaque after copying: their described source maps, cache
move lists and historical artifact lists are not traversed. Active source image
trees and cache payload files cannot enter this bounded evidence selection.
The five older indexes are hash checked as files only. Their previously accepted
3,327 artifact pins are not re-audited or reported as newly verified here.

After root observes all five phases and finalize succeed, prepare a plan with:

```powershell
rtk proxy python -X utf8 target/parity-validation-20261015/next-combined-preservation-v2/preserve_combined.py plan --tag-prefix combined-r3-r5-20261016-1315 --reviewed-context target/parity-validation-20261015/next-git-impact-details/revisions/combined-context-prep-v1/actual-contexts/combined-r3-r5-20261016-1315.json --reviewed-context-sha256 6475705a81b2dbc053082b17d0a8445617b4a0724c8f565277601b897c17a88b --plan-out G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence/target/parity-validation-20261015/next-combined-preservation-v2/PLAN.json
```

Root reviews that concrete plan and explicitly supplies its digest:

```powershell
rtk proxy python -X utf8 target/parity-validation-20261015/next-combined-preservation-v2/preserve_combined.py apply --plan G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence/target/parity-validation-20261015/next-combined-preservation-v2/PLAN.json --reviewed-plan-sha256 REPLACE_WITH_EXACT_REVIEWED_DIGEST --allow-new-evidence-bundle
```

Apply reconstructs the entire deterministic plan before its first write, then
exclusively creates `docs/validation/parity-windows-2026-10-16-wave6`. Each attempted
write is preceded by a flushed/fsynced entry in a fresh ignored JSONL journal.
An existing destination or journal is refused; partial outputs are retained for
review and never adopted, merged, deleted, replaced or rolled back. Every copy is
hash checked. A full destination inventory must equal **all** exact planned
additions, including raw logs ignored by the repository's `*.log` rule. The
plan binds Git's ignored-output partition in advance with `git check-ignore`.
The final full normal Git-visible map must equal the before map plus every
planned Git-visible addition; no missing output is silently filtered away.
The root may update concise repository status separately after this bundle.

Preparation validation is limited to AST parsing and low-impact synthetic smoke
checks. This document makes no claim that the in-progress full run or a future
preservation application has completed.

The reproducible bounded smoke command is:

```powershell
rtk proxy python -X utf8 target/parity-validation-20261015/next-combined-preservation-v2/smoke_preservation.py
```

It creates only a fresh retained `smoke-*` directory in this ignored staging
area, checks five historical index byte hashes, and runs neither full planning
nor application. The smoke source is included in the eventual tool closure.
