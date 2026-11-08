# Code Intelligence 0.5.0: pending Windows reparse checks

The frozen full default run executed all testable targets and reports
2,389 passed, 10 failed and 39 ignored across 263 result groups. Every failure
occurred while creating a real Windows symbolic-link fixture with error 1314;
the underlying safety assertions were not reached. These are failed prerequisites,
not passing security checks. No test was removed, ignored or weakened.

The current agent token has no `SeCreateSymbolicLinkPrivilege`. The repository
instructions prohibit bypassing permissions. The prepared helper changes no
Windows rights, developer-mode setting or test assertion and does not elevate
itself. It has been parsed but **has not been executed as Administrator**.

To replay exactly those ten cases, review
[`run-elevated-reparse-checks.ps1`](run-elevated-reparse-checks.ps1) and
[`windows-reparse-gates.json`](windows-reparse-gates.json), then run this command
from a PowerShell terminal that you explicitly start as Administrator:

```powershell
& 'C:\Users\Lomi\AppData\Local\YokeValidation\tools\powershell-7.6.7-win-x64\pwsh.exe' -NoProfile -File 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\docs\validation\integration-2026-10-19\digest-history-acquisition\run-elevated-reparse-checks.ps1' -Workspace 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence' -Descriptor 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\docs\validation\integration-2026-10-19\digest-history-acquisition\windows-reparse-gates.json' -OutputDirectory 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\windows-reparse-elevated-20261019'
```

Use a new output directory on a repeat. The helper checks the exact frozen
source manifest, current source bytes, native CLI and each test-program hash
before execution. A changed build/source tree refuses; the descriptor must then
be regenerated against a newly verified checkpoint. Each selected case runs
alone with a 60-second limit and owned process-tree cleanup. A successful exit
with zero tests or an ignored case is rejected. Output logs and receipts stay
in the explicitly selected directory.

This does not run the default ignored live-provider gates or establish whole
platform/upstream acceptance. The overall STATUS goal remains open.
