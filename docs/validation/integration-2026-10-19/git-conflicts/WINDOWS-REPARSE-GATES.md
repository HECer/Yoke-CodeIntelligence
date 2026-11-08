# Code Intelligence 0.5.0: scoped Windows prerequisite replay

Both failures in this scoped run occur at real Windows symlink-fixture creation
(error1314), before their product safety assertions. No test was removed,
ignored or weakened. Eighteen separate CLI admission cases include real junctions;
those cases do not replace file/directory symbolic-link coverage.

The helper has only been parsed, not elevated or executed by the agent. It
changes no Windows rights or developer-mode settings, does not elevate itself,
and refuses changed input/source/application/test-binary hashes. It runs only
the two original non-ignored cases, one per process with a 60-second bound.
This descriptor covers the conflict checkpoint; later source changes make it
stale by design. The earlier full-run descriptor remains historical.

If replay is needed, review the helper/descriptor and run from a PowerShell
terminal that you explicitly start as Administrator:

```powershell
& 'C:\Users\Lomi\AppData\Local\YokeValidation\tools\powershell-7.6.7-win-x64\pwsh.exe' -NoProfile -File 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\docs\validation\integration-2026-10-19\git-conflicts\run-elevated-reparse-checks.ps1' -Workspace 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence' -Descriptor 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\docs\validation\integration-2026-10-19\git-conflicts\windows-reparse-gates.json' -OutputDirectory 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\git-conflicts-windows-reparse-20261019'
```

Use a new output directory on a repeat. Other default/ignored/live-provider and
hosted/full-native/whole-STATUS acceptance are separate.
