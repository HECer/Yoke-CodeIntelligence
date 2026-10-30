# Hosted platform follow-up at 553e5dc1

Code Intelligence native **0.5.0**, retained TypeScript **0.3.0**.
These findings concern commit
`553e5dc162a0352b05e0be3abe91860924b4f922`. The PR checkout is merge
`1944cb38da18958e24875516ecfedfa4e7c6e915`; its parents include that head and
main `e18afea4ee894331f06856904e90300aaf800f94`. Both head and merge have tree
`41eecbe6dfccc88d82422a483fd96780c76ee936`.

Both [PR Rust CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40231606053)
and [push Rust CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40231599084)
complete Ubuntu acceptance: all eight shards, coverage, the actual
rust-analyzer gate and release build. Windows and macOS acceptance correctly
fail coverage after their failing shards. No release qualification is claimed.

## Dashboard and cleanup results

The PR Windows receipts execute three complete groups without name filters:

| Group | Passed | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| Library | 768 | 0 | 7 |
| `session_dashboard` | 13 | 0 | 0 |
| `session_dashboard_management` | 24 | 0 | 0 |
| **Scoped total** | **805** | **0** | **7** |

All four new mutex-contention regressions pass. The original pending-provider
HTTP test also passes, on both Windows runs and macOS. This supersedes the
previous execution-pending statement for these named tests; it does not make
the whole Windows suite pass.

macOS also passes the twelve cleanup regressions, complete `runtime` 14/0,
`process` 5/0 and `github_app_ingress` 20/0. The original Darwin unreaped-exit,
signal-race and surviving-descendant checks are executed. The macOS library
overall remains **473 passed / 1 failed / 6 ignored / 0 filtered** because of
the history fixture below.

## Windows failures and the LocalAppData result

Both Windows shard-0 Python runs fail the two original actual PowerShell
tests at their original 20-second limits. Both shard-2 native groups fail
`windows_runtime_environment_supports_managed_known_folders`: process 6/1.
The push shard-5 REPL group additionally fails
`explicitly_selected_shell_runs_and_cell_errors_preserve_namespace`: 21/1;
the same complete group passes 22/0 on the PR runner.

All fifteen PowerShell diagnostic artifact digests are verified. They contain
35 trials. Direct-console controls pass in 0.143–0.233 seconds.

| Run / shard | Original final folder trial | Captured `Write-Output` | With `LOCALAPPDATA` |
| --- | --- | --- | --- |
| PR / 0 | 18.047 s, pass | 17.949 s, pass | 17.809 s, pass |
| PR / 2 | 20.031 s, timeout | 20.034 s, timeout | 20.023 s, timeout |
| Push / 0 | 15.867 s, pass | 15.911 s, pass | 15.856 s, pass |
| Push / 2 | 20.036 s, timeout | 20.027 s, timeout | 20.020 s, timeout |
| Push / 5 | 11.997 s, pass | 12.133 s, pass | 12.187 s, pass |

`LOCALAPPDATA` is not sufficient to repair the failure. Both failing shard-2
hosts retain the pre-`Write-Output` marker as their last observation, including
the assignment control. Each timeout still has one PowerShell root and one
console host before forced cleanup. Both Jobs become empty afterward;
post-cleanup emptiness is not natural completion. Successful trials have exit
0, full output and empty Jobs before forced cleanup.

Those markers have no individual timestamps. They locate the last observed
boundary but do not measure its duration separately from startup. Windows
checkout CRLF bytes explain the probe hash: `801926b9…` equals the exact
553e5dc1 blob `22273c1e…` converted from LF to CRLF.

## All macOS failing groups

All eight PR macOS ZIP digests and every retained group-log hash are verified.
The 1,086 selected source-input hashes match across the eight shard maps;
23 relevant source, fixture and workflow hashes match the recorded Git blobs.
There are **69 failed cases in twelve groups**:

| Shard | Group | Failed cases | Observed boundary |
| --- | --- | ---: | --- |
| 0 | `mixed_meaning_claims` | 5 | Closed PDF address-space refusal |
| 1 | `mixed_pdf_view` | 8 | Closed PDF address-space refusal |
| 2 | Library | 1 | History fixture sleep exceeds the original deadline |
| 2 | `session_prompt_memory` | 4 | Spawn/direct-limit refusal and missing response |
| 3 | `mixed_meaning_composition` | 7 | Closed PDF address-space refusal |
| 4 | `media` | 5 | Closed PDF address-space refusal |
| 4 | `session_prompt_runtime` | 6 | Spawn/direct-limit refusal and initialization symptoms |
| 5 | `mixed_meaning_queries` | 6 | Closed PDF address-space refusal |
| 5 | `session_config_prompt_catalogs` | 9 | Spawn/direct-limit refusal |
| 6 | `mixed_meaning_provider` | 11 | Closed PDF address-space refusal |
| 7 | `session_config_templates` | 6 | Spawn/direct-limit refusal |
| 7 | `source_acquisition_cli` | 1 | Closed PDF address-space refusal |

Every one of the **43 PDF failures** records exit 1, no signal, resource-limit
phase, `SetAddressSpace`, errno 22, requested **268,435,456 bytes / 20 CPU
seconds**. The worker refuses before PDF input processing. The separately
checked push macOS shard 0 has the same five closed PDF refusals; its receipt
is [push-macos-zero.json](push-macos-zero.json).

The 25 prompt-family failures contain sixteen explicit spawn `EINVAL22`, four
direct owned-memory-limit refusals, one generic executable-unavailable error,
two missing-response assertions and two initialize-first responses. The
generic and downstream cases do not independently prove a memory cause.
No conforming replacement for the strict macOS address-space contract is
established, and the contract is not weakened to obtain passing tests.

The remaining history failure supplies these fixed observations:

```text
size_pre_sleep:      5.75 us, remaining 64.958709 ms
size_post_sleep:   155.738458 ms, remaining 0 ns
deadline_check_ok: None
contents_entry:    None
counts:           (1,0), expected (1,1)
elapsed:          155.783375 ms
```

The first requested 40-ms sleep returned after about 156 ms. Refusal before
the body is consistent with the original 65-ms deadline. Forcing the body
request would violate that deadline. The production traversal is unchanged;
the original 65-ms, two 40-ms delays, `(1,1)` and `<200 ms` gate remain and
still fail on this host. No complete macOS acceptance is claimed.

## Next Windows boundary control and local checks

A separate failure-only one-minute step runs two fresh children with the
original 20-second trial and two-second cleanup limits. One brackets public
cmdlet lookup, then invocation of the returned command object. The other first
brackets import of the fixed `$PSHOME` Utility manifest, then performs the same
lookup and invocation. Direct-console Stopwatch scalar markers distinguish
these phases; lookup can include autoload. Original canonical, captured and
LocalAppData commands, production environment and all original assertions stay.

The public APIs include Windows PowerShell 5.1 reference assemblies. Microsoft
documents [GetCommand](https://learn.microsoft.com/en-us/dotnet/api/system.management.automation.commandinvocationintrinsics.getcommand?view=powershellsdk-7.4.0),
[saved-command invocation](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_command_precedence?view=powershell-5.1#use-the-call-operator)
and [explicit module import](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/import-module?view=powershell-5.1).
Cmdlet-only lookup and command-object invocation differ from canonical string
resolution. Fresh children still share host caches and run in fixed order;
relative durations alone cannot establish exclusive causality.

Both new controls actually pass locally with original output and empty nested
Jobs before forced cleanup: 0.253 and 0.241 seconds on PowerShell 5.1.19041.6456.
This does not establish behavior on the affected 5.1.26100 hosted runners.
Static review finds no substantive source objection; hosted results are pending.

Typecheck and npm **260 passed / 0 failed / 3 original skips** pass, with root
exit and empty owned Jobs before cleanup. Actionlint 1.7.12 also passes.
All **142 Python tests** pass with exit 0 in both local full runs, including
the six new diagnostic tests. However, the owned compiler helper `vctip.exe` remains
after the original drain in both runs. It is removed by the owned Job cleanup,
and root exit plus empty Job are proved afterward. Both controller receipts
therefore remain **incomplete**; the Python test result is not a successful
whole-tree lifecycle receipt. No timeout, profile, system policy or guard is
changed. All 1,134 selected inputs remain identical across these checks.

## Retained evidence and remaining scope

- [artifact-index.json](artifact-index.json) binds 28 retained GitHub artifacts to
  exact original ZIP sizes and SHA-256 digests.
- [raw-index.json](raw-index.json) records all **1,270** retained text members.
- [raw-evidence.zip](raw-evidence.zip) contains **7,018,939** expanded bytes;
  archive **1,805,178** bytes, SHA-256
  `d3048eb18f995c5270ef4486199815fde2340e209e33a1fe46be4d8d4c702a5c`.
- [archive.json](archive.json) contains the archive totals and image exclusion.
- The separately inspected push-macOS-zero artifact is indexed in its own JSON;
  its complete raw ZIP is not included in the main archive.

Every retained member is reopened and verified. Artifact inspection reads and
archives no native executable, PDB, library or object bytes. Protected existing
Yoke images remain untouched. Full Windows/macOS CI, the strict
macOS worker capability, history timing gate, local release storage path,
fresh release CLI/owning Sessions/browser, bundled VSIX, the broader inventory,
main integration and publication remain open. This checkpoint is not project
completion or release acceptance.
