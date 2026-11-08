# Windows PowerShell command lookup: observed boundary

Code Intelligence native **0.5.0**, retained TypeScript **0.3.0**.
This observation is from head
`0d0b43e3ac41cf25c8f2d6e57ec3100df7a68d4d`,
[Windows shard 0](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40239823861/job/121309035581).
The two original actual Python PowerShell tests still fail at 20 seconds.

The new diagnostic artifact is verified before decoding: ID **12616881652**,
ZIP **1,373 bytes**, SHA-256
`614c8d41193e8d1575ef0d385026b2bf01728d847e3eb0f4917b2a76dd2ffed5`.
Its reported probe hash matches the exact Git blob converted from LF to CRLF.
See [artifact.json](artifact.json) and [host-report.json](host-report.json).

| Fresh control | Observation | Completion |
| --- | --- | --- |
| Cmdlet-only lookup | Last marker `before-command-lookup-ms:0`; no return from `GetCommand` | Timeout at 20.032 s; root and console host remain until force |
| Fixed Utility import, lookup and command-object invocation | Import 320 ms; lookup 5 ms; invocation 24 ms | Exit 0 at 0.572 s, exact output and both Jobs empty before force |

This locates the stalled boundary before `GetCommand` returns. Lookup can
include autoload. The successful control changes module state and invocation
style; fixed order and shared caches also limit exclusive causal attribution.
It does not yet establish that the normal string-name `Write-Output` command
works after import, or justify a general production bootstrap.

The next separate one-minute diagnostic prepends only the same fixed Utility
manifest import to the **exact original final-folder script**. That script
remains a byte-identical suffix, with its original known-folder checks and
ordinary `Write-Output 'owned-windows-runtime'`. This control has no
`GetCommand` or command-object invocation. It retains the baseline environment,
two snapshot Jobs, 20-second deadline and two-second cleanup. Existing cold
controls and the production launcher are unchanged.

The next control actually passes locally in **0.242 s**. Exact stdout is
`owned-windows-runtime\r\n`; all original before/after folder and output
markers are present, exit is 0 and both Jobs are empty before forced cleanup.
Local PowerShell is 5.1.19041.6456; the affected host is 5.1.26100.33648.
The hosted original-command result remains pending.
[Local report](local-report.json).

Five new tests preserve the exact script suffix, prior plan, Job/resume
identity, baseline environment, mutually exclusive modes and occupied output.
Typecheck, npm **260 passed / 0 failed / 3 original skips**, Actionlint 1.7.12
and the actual new control complete with natural owned-tree exit.
All **147 Python tests** pass with exit 0; the owned `vctip.exe` compiler helper
still survives the original drain. Forced Job cleanup proves root exit and
emptiness afterward, so the Python lifecycle receipt remains **incomplete**.
No guard or timing requirement is relaxed. The 1,134 selected inputs match
before and after. [Check receipts](local-checks.json),
[raw text/source-map evidence](local-evidence.zip), [member hashes](local-index.json).

The [prior hosted follow-up](../hosted-platform-followup/README.md) records the
805 scoped Windows passes, remaining Windows/macOS failures and macOS memory
and history gates. This addition changes diagnostics only. It does not fix
those failures or establish complete platform, CLI, VSIX or release acceptance.
