# Code Intelligence 0.5.0: Windows prerequisite replay

This descriptor supersedes the earlier acquisition checkpoint's binaries and
source manifest. The default run's ten failures occur while creating real
Windows symlink fixtures (error1314); their product safety assertions were not
reached. No test was removed, ignored or weakened.

The helper has not been elevated or executed by the agent. It changes no Windows
rights or developer-mode settings, does not elevate itself, and refuses changed
source or binary hashes. It runs exactly one non-ignored case per test process
with a 60-second bound and writes receipts only to the selected new directory.

If replay is needed, review the helper/descriptor and run from a PowerShell
terminal that you explicitly start as Administrator:

```powershell
& 'C:\Users\Lomi\AppData\Local\YokeValidation\tools\powershell-7.6.7-win-x64\pwsh.exe' -NoProfile -File 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\docs\validation\integration-2026-10-19\history-jobs\run-elevated-reparse-checks.ps1' -Workspace 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence' -Descriptor 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\docs\validation\integration-2026-10-19\history-jobs\windows-reparse-gates.json' -OutputDirectory 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\history-jobs-windows-reparse-20261019'
```

Use a new output directory on a repeat. This does not run other ignored host or
live-provider gates or establish hosted/universal/whole-STATUS acceptance.
