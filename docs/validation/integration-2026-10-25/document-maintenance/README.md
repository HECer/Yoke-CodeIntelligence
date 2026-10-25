# Document maintenance: scoped Windows execution and current platform evidence

Code Intelligence native **0.5.0**, retained TypeScript **0.3.0**.
The executed native source is
`a27b5084dbb329d40b589a95a41085f3bca1ea50`, on Windows with Rust **1.94.1**.
These results qualify two complete local native groups. They do not qualify
the later dashboard contention correction, all native targets, or a release.

## Executed checks at a27b5084

| Complete native group | Passed | Failed | Existing ignored | Filtered |
| --- | ---: | ---: | ---: | ---: |
| Library | 764 | 0 | 7 | 0 |
| `session_dashboard_management` | 24 | 0 | 0 | 0 |
| **Total** | **788** | **0** | **7** | **0** |

This executes the integrated retirement, archive-purge, cross-maintenance and
dashboard management fixtures. Each group uses a fresh completed producer and
runs without a test-name filter. The seven original library ignores remain;
their exact names and reasons are retained in [summary.json](summary.json).
No real user-data maintenance occurred.

Warnings-denied workspace/all-target Clippy, all 368 native format roots,
`npm run typecheck`, npm **260 passed / 0 failed / 3 original skips**, Python
**130 passed / 0 skipped**, and Actionlint 1.7.12 also pass at this source.
Clippy artifacts are compilation metadata, not additional test executions.

The four producer/test commands, Clippy, two Node commands and format check
constitute **eight main command phases**. Every one proves root exit and an
empty owned Job before forced cleanup. All **1,049 selected native, script,
Cargo and workflow inputs** match before and after. Controller maps also match
within each native phase. The independently reviewed derived controller adds
only the finite dashboard management target; its function/class bodies and
private Windows helper remain identical to the base controller.

Original profiles, thread settings, assertions, ignores, deadlines, 1.2 GB
start admission and 500 MB running reserve remain. External dependencies,
tool binaries, caches and the complete host environment are not frozen.
The controllers require host-wide serialization; their per-root mutex is not
a sandbox against other processes with the same privileges.

## Later source checks and hosted findings

The subsequent PowerShell LocalAppData diagnostic and dashboard contention
correction have source checks, not native runtime acceptance. Python passes
**136 tests / 0 skips** with installed Cargo added only to the child PATH.
Typecheck, npm **260/0/3**, and native formatting pass over **1,134 selected
inputs**, including Node sources, which remain identical across the checks.
The four new Rust tests and original HTTP regression remain uncompiled and
unexecuted locally for this later source.

The source-check wrapper initially attempted to create its exclusive result
file twice after npm had completed successfully. The original partial result
and successful owned-Job receipts remain. Recovery verifies the original
source map before formatting and again after all checks. See
`new-contention-source-checks/recovery-result.json` in the archive.

GitHub reports all eight Ubuntu shards, coverage, the actual rust-analyzer
gate and the release build successful at a27b5084. Windows and macOS acceptance
correctly refuse incomplete coverage after shard failures.
[Exact-source Rust run](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738),
[Ubuntu acceptance](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738/job/121271308230).

The Windows dashboard group records 12 passed / 1 failed, with an HTTP 503
during an active provider operation. The later correction retries ordinary
mutex contention within the original request deadline, preserves poison
refusal and cancellation, and leaves the original HTTP assertions intact.
Static review found one new cancellation-test error, which was corrected
before publication. Neither that review nor source checks establish hosted
success. [Windows shard 3](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738/job/121268360849).

Three separately captured `Write-Output` controls retain the original 20-second
limit: two finish naturally at about 17.81 and 18.23 seconds; one times out at
20.02 seconds before the post-cmdlet marker. This rules out an established
rendering-only explanation. A new failure-only trial varies only the explicit
host `LOCALAPPDATA` key; no result for that new trial is claimed here, and the
production environment remains unchanged.
[Windows shard 0](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738/job/121268360939),
[shard 2](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738/job/121268360723),
[shard 5](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738/job/121268361050).

The private PDF diagnostic now identifies seven failing macOS
`mixed_meaning_composition` cases: exit 1, no signal, resource-limit phase,
`SetAddressSpace`, errno 22, requested 268,435,456 bytes and 20 CPU seconds.
These workers refuse before input processing. This finding applies to those
seven cases; it does not identify every historical PDF failure or establish
an alternative strict macOS memory backend.
[macOS shard 3](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40226322738/job/121268360482).

## Preserved unsuccessful attempts

Earlier Clippy and Node attempts hit the unchanged disk reserve. A later
Clippy run found a test-only `DeviceIoControl` declaration mismatch; a27b5084
repairs the input pointer signature and the subsequent complete Clippy passes.
The first Python attempt omitted Cargo from the child PATH, producing two
failures and one skip; the corrected complete runs pass. These failures do
not count as passes.

The primary release preflight refuses protected existing-image metadata;
those files remain untouched. A separately reviewed isolated checkout has
byte-identical source and a local control bundle. Its fresh release attempt
also stops on the unchanged disk reserve: minimum observed free space
**340,082,688 bytes**, four owned members before forced cleanup, zero after,
and actual root exit afterward. The producer remains incomplete. No candidate
image is read, adopted or executed, and no version or release CLI acceptance
is claimed. The original 4 GB release start floor, 500 MB running reserve,
release profile and 3,600-second build ceiling remain unchanged.

## Retained evidence and open gates

- [summary.json](summary.json): scopes, counts, original ignores and execution receipts.
- [source-inputs.json](source-inputs.json): the 1,049 original selected source byte hashes.
- [raw-index.json](raw-index.json): every archive member's byte length and SHA-256.
- [raw-evidence.zip](raw-evidence.zip): **396** text/source members,
  **55,941,317** expanded bytes; archive **4,701,155** bytes, SHA-256
  `9a36d2914d17e0493a2ef36949699110787b6506b94c4989984e00e1810ddb4f`.

Every archive member was reopened and verified. No native executable, PDB,
library or object bytes are included. Image identities and hashes are retained
only from completed fresh-producer receipts.

Actual fresh release CLI, owning stdio/HTTP Sessions, history acceptance,
desktop/mobile visual inspection, bundled-runtime VSIX, complete current
Windows/macOS CI, strict macOS memory enforcement, physical owner deletion
and reactivation, all-host release proof, the full inventory, main integration
and publication remain open. The project is not complete.
