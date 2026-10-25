# Native platform repair: local Windows evidence

Code Intelligence native **0.5.0**, retained TypeScript **0.3.0**.
The executed product source is
`a9608c7f1c544ad720e2662d23670f9bf00b4560`, on
`x86_64-pc-windows-msvc` with Rust **1.94.1**. These results qualify the named
local scopes; current hosted and Unix acceptance remain open.

## Changes under review

Unix process cleanup requires both the actual raw-child exit receipt and
independent absence of its original process group. An exited root is reaped
before unnecessary signalling. After reaping, the retained group ID is used
only for observation. EPERM never proves absence, and surviving descendants
still refuse completion. Complete-output capture now propagates checked cleanup
failure before publishing output. The original two-second cleanup ceiling stays
in force. Darwin race tests include owned teardown before assertions; their
actual Unix execution is still pending.

The repository-discovery fixture separates filesystem marker lookup from
platform-specific protected-name enumeration. It preserves protected descendant
refusal and false registration/source grants. Fixed history and ingress phase
observations retain the original deadlines, cadence, counts and assertions.

Windows diagnostics retain markers around the original `Write-Output` calls,
add a separate direct-console control, and observe bounded, membership-verified
Job metadata through retained handles. Failure-only CI diagnostics now include
every failing Windows shard, including a Python failure before native tests.
The historical hosted PowerShell stall has no demonstrated root cause yet.

## Executed checks

Each native group ran in full, without a test-name filter. Existing ignored
tests were not promoted or changed.

| Native group | Passed | Failed | Existing ignored | Filtered out |
| --- | ---: | ---: | ---: | ---: |
| Library | 703 | 0 | 7 | 0 |
| `repository_discovery` | 8 | 0 | 0 | 0 |
| `github_app_ingress` | 20 | 0 | 0 | 0 |
| `process` | 7 | 0 | 0 | 0 |
| `agent_repl` | 22 | 0 | 0 | 0 |
| **Total** | **760** | **0** | **7** | **0** |

| Additional check | Result |
| --- | --- |
| Workspace/all-target Clippy, warnings denied | Exit 0; metadata only |
| Complete native formatting check | Exit 0 |
| `npm run typecheck` | Exit 0 |
| `npm test` | 260 passed, 0 failed, 3 original skips |
| Complete Python suite with installed Cargo in child PATH | 126 passed, 0 failed, 0 skipped |
| Actionlint 1.7.12 | Exit 0 |

Clippy recorded 370 fresh native artifact observations over 368 distinct Cargo
targets. These are compilation metadata, not additional test executions.
The seven library ignores and their original reasons are retained verbatim in
[summary.json](summary.json) and the raw test output.

## Execution and source receipts

Five fresh no-run producers, their five corresponding test executions, and
Clippy used the reviewed private controller archived under `control/`.
All **1,044 selected source inputs** and controller inputs match before, after
and at the final check of all **11 phases**. Node/package/worker and Python
input maps are separately retained with their checks.

Every phase proves root exit and an empty owned Job **before forced cleanup**;
post-cleanup accounting is also zero. Native admission remains 1.2 GB free,
with the separate 500 MB running reserve. Suspended creation, assignment before
resumption, finite log/output bounds, image identity pins and the original
profiles, thread settings and deadlines remain in effect. Only a freshly
produced, admitted hashed test executable is selected for execution. No old
plain executable or protected executable/PDB is adopted.

Controller preflight actually exercised owned Python success, timeout and log
capacity refusal, plus write/delete denial while holding a text-file read pin.
These are controller checks and do not qualify native application behavior.

The initial Python suite used a PATH without Cargo and recorded two failures
and one skip. The complete repeat with the installed toolchain in the child
environment passed all 126 tests; both outcomes are retained. A Node log-display
wrapper hit a console-encoding error after the successful test and receipt had
already completed; the original UTF-8 log and receipt were reopened and checked.
An isolated metadata-only cast probe did not reproduce a suspected Clippy
warning and caused no product change. None of these observations is Unix or
hosted acceptance.

## Retained evidence

- [summary.json](summary.json): actual counts, image receipt metadata and scope flags.
- [source-inputs.json](source-inputs.json): selected source byte lengths and SHA-256.
- [raw-index.json](raw-index.json): every raw archive member's length and SHA-256.
- [raw-evidence.zip](raw-evidence.zip): **507** text/source members, **70,482,655**
  expanded bytes; archive **5,547,148** bytes, SHA-256
  `585b370bdd7b6384e46510c178e4e72a654c0d13c442a1c5c9c7857120b13f33`.

Every archive member was reopened and checked against the index. No native
executable, PDB, library or object bytes are archived. Image hashes and physical
identity values are retained only as execution receipts.

This checkpoint does not establish the complete default native suite,
doctests, Darwin/Linux execution, new hosted CI, strict macOS worker memory
enforcement, live history/browser workflows, bundled-runtime VSIX acceptance,
the full inventory, main integration or a release. Those gates remain open.
