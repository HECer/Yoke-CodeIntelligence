# Explicit dependencies of owned Windows PowerShell fixtures

Code Intelligence native **0.5.0**, retained TypeScript **0.3.0**.
Implementation and local checks concern
`6c5224111dc5c6dfdca716896ec6a8851a615d8a`.

## What changed

The owned positive fixtures now explicitly import the fixed OS Utility manifest
before their existing payloads. They require its `Write-Output` cmdlet. The
original scripts remain exact suffixes; original assertions, 20-second operation
deadlines, Job/resume behavior, output and Unicode/quote/semicolon handling stay.
Import happens inside that same operation budget. Missing import emits a closed
error and exits 125 before payload entry; there is no fallback to cold lookup.

This changes **test preparation**. Generic production `run_capture`, authorized
`HostShell` forwarding and their environment rules remain byte-identical.
There is no global script prelude, argument rewriting or module-path override.
A generic prelude could invalidate caller `param` or `using` statements.

The canonical raw probe and all its commands also remain byte-identical.
Windows shard 0 now runs and publishes that probe regardless of positive-fixture
success; failing Windows shards continue to record it. A raw timeout remains
`diagnosticComplete:false` and `nativeAcceptance:false`. Each probe starts a
fresh process, but processes share host caches. This does not certify pristine
machine cold startup or claim the former cold-success scenario passed.

The [earlier lookup analysis](../../integration-2026-10-25/powershell-command-lookup/README.md)
motivated this dependency declaration. At head 34410e04, the subsequent
[imported-original control](hosted-prepared-original.json) also passes on the
affected 5.1.26100 host in 0.274 seconds, with exact output,
original markers, exit 0 and empty nested Jobs before force. Uninitialized lookup
still times out at 20 seconds. Fixed order/shared caches limit exclusive causal
attribution; this is not a general production shell repair.

Two independent contract reviews and a source review found no substantive
objection to this scoped fixture correction. Historical unchanged-test reports
are not a guarantee about arbitrary host module-discovery latency. Full matching
hosted process/REPL execution remains required.

## Adversarial checks and local results

The added checks cover missing import refusing before payload entry, a foreign
module path failing to replace the selected system manifest, unchanged raw plans,
exact script suffixes and existing path/authority/namespace assertions. Original
Python assertion ASTs and native assertion/deadline tails are compared with the
baseline; their expressions remain unchanged.

| Check | Actual result |
| --- | --- |
| `npm run typecheck` | Exit 0; owned root/tree exit before cleanup |
| `npm test` | 260 passed, 0 failed, 3 original skips; natural owned-tree exit |
| Complete Python suite | 150 tests passed, exit 0; controller remains incomplete |
| Actionlint 1.7.12 | Exit 0; natural owned-tree exit |
| All 368 native format roots | Exit 0; natural owned-tree exit |
| Workspace/all-target Clippy, `-D warnings` | Completed, no warnings; metadata only |
| Fresh `process` no-run build | Cargo 0; producer incomplete; no image admission |
| Local complete `process` / REPL execution | Not run; matching hosted execution pending |

All 1,135 selected inputs remain identical across the final text/Node checks.
Clippy and the no-run producer preserve all 1,050 selected native/script/Cargo
inputs and private controls. External tools, dependencies, caches and the entire
host environment are not frozen.

In both Python collection and the native producer, owned `vctip.exe` survives
the original drain. Checked Job cleanup proves emptiness and root exit afterward.
This does not retroactively prove natural completion. The native producer refuses
admission before reading any selected test image. No image from that incomplete
producer is adopted or executed; Clippy observations are not runtime tests.

Cargo also reports rebuilding ordinary `target/debug/yoke.exe` and PDB outputs
while producing the integration test. The private controller's protected-name
check fences selected-image admission, not every ancillary Cargo output. This
was overlooked during the primary-checkout attempt. No further primary-checkout
integration producers are used; future local ones require an isolated output
namespace. These reported binary outputs are not inspected or used as evidence.

The first source-check pass preceded removal of an extra EOF blank line and a
diagnostic step-label correction. Both passes are retained; the complete repeat
qualifies the committed source. No profile, ignore, deadline or lifecycle guard
is relaxed to obtain a pass.

## Evidence and remaining gates

- [summary.json](summary.json): exact scopes, assertion/production-source proof,
  incomplete producer, lifecycle and ancillary-output metadata.
- [raw-index.json](raw-index.json): all 141 text/source members and their hashes.
- [raw-evidence.zip](raw-evidence.zip): 16,174,609 expanded bytes; archive
  1,444,919 bytes, SHA-256
  `25925de6efd88dabf66f355cb51b34cfce89cdc89aa4c6c86c8126b2e2cbeec4`.

Every archive member was reopened and checked. Artifact packaging reads and
includes no native executable, PDB, library or object bytes. The guard limitation
and unsuccessful producer remain explicit. Current hosted acceptance, the raw
PowerShell startup limitation, strict macOS memory and history gates, CLI/Session
and browser workflows, bundled VSIX, full inventory, main and release remain open.
