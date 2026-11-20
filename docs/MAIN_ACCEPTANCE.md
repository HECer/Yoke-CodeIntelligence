# Current main acceptance and next gates

## Verified main baseline and dashboard captures (2026-11-20)

PR #38 merged at `21cdc071546139b6278c4a262278dc4bdb8b97b9`.
[CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643170)
and [Rust core](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032)
both passed on that merge. The earlier Explorer startup failures below remain
valid for their original revisions. This passing run exercises the new startup
and session-expiry cases. The intermittent timeout's root cause is still unknown.

Five Windows dashboard runs captured desktop (1440 px) and mobile (390 px)
before, pending and after states. All 30 captures were inspected and paired
with their DOM observations. The configuration views showed the draft, recovery
review and guarded save with visible controls. The statistics views showed
the totals, confirmation, desktop stale-reset message and mobile cleared state.
None of the 30 DOM observations recorded horizontal overflow. The five runs
report real headless browser execution and empty owned process trees before
Job cleanup. The HTTP run also records peer-session survival.

The ZIP bytes matched the SHA-256 digests returned by GitHub. All five source
manifests list the same 1,203 files, whose hashes match the checked-out Windows
bytes for the stated main revision. The receipts report the same native binary
SHA-256, `1feba5035eb0e8a361a88ea7a32cd6ba9b4051f59845f5b7681bc1c7765e7f59`.

| Dashboard flow | GitHub Actions artifact | ZIP SHA-256 |
| --- | --- | --- |
| Native stdio history | [14183775550](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14183775550) | `f116215e93b6b619a6770a89a3f27a63c0296ed33dedcae000c7f33be349405f` |
| Serena stdio history | [14183376458](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14183376458) | `cfb84dc8806bb41e772d5b6ba1316713d94886e2f8edeca35e0d1c0d8fa86d3e` |
| Persistent Python history | [14183541088](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14183541088) | `7ce3240b73c1b5112a80709bd0cdc5ebe9e0363b46cf422751aacafe587a631a` |
| HTTP Session history | [14183651636](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14183651636) | `3f440949ba5ac524543a8027d705ff4261bcde7f58c98e3320bac43b5e2f673c` |
| Serena tool statistics | [14183014504](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14183014504) | `3fc2c8f0e4ed177253d72a3fd4fc1deafc812b9476e9c865116d5ba0f015601c` |

The visual review covers these captured dashboard states. Motion, visible
desktop shell, physical mobile devices, other surfaces and human approval
remain outside this evidence.

### Selected Windows contract results

All eight Windows shard ZIP digests and every retained group log's length and
SHA-256 matched their receipts. The 382 group results were successful across the
eight shards. The following whole-target results are part of that same run.

| Original contract or follow-up | Native target | Passed | Ignored | Artifact |
| --- | --- | ---: | ---: | --- |
| Graphify audio transcription (`graphify:asr`) | `media_asr` | 6 | 0 | [14182225236](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14182225236) |
| Serena project encoding (`serena:configuration:projectKeys:2`) | `source_encodings`, `source_encodings_transport` | 24 + 14 | 0 | [14181159307](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14181159307), [14181243718](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14181243718) |
| Serena project initial prompt (`serena:configuration:projectKeys:19`) | `session_prompt_runtime` | 11 | 0 | [14181490915](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14181490915) |
| Serena project activation command and timeout (`serena:configuration:projectKeys:23`, `:24`) | `project_activation` | 27 | 0 | [14181490915](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14181490915) |
| Persistent Debug Adapter Protocol | `debug_adapter` | 7 | 1 | [14181089359](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14181089359) |
| Explorer startup and MCP HTTP review | `explorer_http`, `mcp_http_review` | 10 + 2 | 0 | [14182225236](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14182225236), [14181381664](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43762643032/artifacts/14181381664) |

The ASR target checks the owned worker transport, resources and result replay.
Actual selected faster-whisper inference has its own earlier
[owned CPU trial](validation/integration-2026-10-23/native-asr/README.md).
Encoding coverage is limited to the explicitly selected [codecs and consumers](../native/docs/source-encodings.md).
[Prompt rendering](../native/docs/session-prompt-runtime.md)
and [activation commands](../native/docs/project-activation.md) use
independent host grants. Upstream template parity and deployment policy remain
open. The ignored DAP case is the selected live debugpy gate.

## Earlier merged source: 2026-11-12

[PR #37](https://github.com/HECer/Yoke-CodeIntelligence/pull/37) merged normally
as `9fc652a44ec8867d25371d39e76fd7bb58499c7e`, with original main `3d58d07b`
and qualified head `c4ed8a8f` as its ordered parents. Its tree
`afaa25929a617ee81235ba779f2ff97e1873fa74` exactly matches that qualified head.

Before merge, both push and PR events qualified the source: 74 latest records,
71 successes and three expected conditional skips across the 37 check names.
The PR Rust core attempt 1 failed Windows Explorer readiness and two macOS
knowledge indexing cases; one failed-job rerun passed unchanged source. Both
rerun artifacts have complete group/log proofs and the same inventories and
partitions as the failed attempt. The original failures remain evidence and
their precise causes remain unknown; no assertion or deadline was relaxed.

The real combined VSIX artifact `13897523218`, SHA-256
`b0b89f1d08c6096b5e2cb67e53b1ad0c1816fd3f315dbf8e0e00acfe667667ce`,
qualifies all five owned phases on `c4ed8a8f`. Its TypeScript phase executes the
bundled CLI in the actual VS Code host using pinned Node 22.16.0, refuses
workspace executable/runtime overrides, checks protected-buffer exclusion and
exact unsaved source/symbol evidence, preserves saved bytes and stops. Both
original Rust host scopes remain. All five jobs exit naturally with empty
owned process trees before checked cleanup and handle closure. This is actual
host execution, independently of the local API-substitute tests below.

### Post-merge result: not a complete main pass

[Main CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43127421987)
passes, including the real combined VSIX/host job. However,
[main Rust core](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/43127421645)
fails the Windows shard 6 Explorer group and its required aggregate. Across the
37 main checks there are 34 successes, two failures and one conditional skip.
The [metadata snapshot](validation/main-2026-11-12.json) preserves all identities
and conclusions. Metadata alone is not a new independent artifact/visual audit.

The failed shard artifact `13904586684`, SHA-256
`20ab47d61cc3ca2b016669f39ff3cb11325cb264538ee34aef33269d284e54f0`,
independently verifies 97 ZIP members, all 96 log lengths/hashes and 1,203 source
inputs against main (42 exact Git-byte matches and 1,161 LF/CRLF-only matches).
It has 47 successful whole Cargo groups and one failed Explorer target: six
passed, two failed, no ignored or filtered cases. The rebuild/reload and absent
host-grant cases both fail before readiness with `Request deadline exceeded`.
That error does not distinguish initial selection/capture from HTML export.

The current follow-up adds only fixed startup phase contexts and two refusal
regressions, with all original deadlines, assertions and runtime grants intact.
Fresh native qualification is required. The phase evidence is diagnostic,
not a causal timeout repair or a claim that current main passes all gates.

Local typecheck and the complete Node suite pass (272 passed, zero failed,
three existing skips). Rustfmt accepts both changed Rust files; the eight
original integration cases/helpers retain their exact normalized bytes. The
two new native cases are authored but have not been compiled or executed
locally. Hosted compilation and whole-target execution remain required.

## Earlier verified baseline: 2026-11-11

[PR #7](https://github.com/HECer/Yoke-CodeIntelligence/pull/7) merged normally on
2026-11-08 as `3d58d07b382040e135a47e36fb24e81dc36185d7`. Remote `main` retains
the qualified tree `401254215948957f61a77034135358b6c24b5cc6` and original
feature histories. The local integration checkout is not a release receipt.

Both post-merge workflows completed successfully on that exact commit:

- [CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/42469372002):
  Node 22.16/24 platform checks, owned Windows launcher and combined VSIX job.
- [Rust core](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/42469372178):
  all platform shards/aggregates and the selected credential-backed TLS gate.

These workflows report 36 successful checks and one conditional history-browser
skip across 37 checks. The [metadata snapshot](validation/main-2026-11-08.json)
retains every job identity, source commit, conclusion and URL. This snapshot is
workflow metadata, not a new independent raw-artifact or visual audit. The real
VSIX job's declared scope is explicit/bundled Rust hosts and a separate pinned
TSGo query. Bundled TypeScript bytes alone do not qualify TypeScript host mode.

Earlier timed-out/failed runs remain historical evidence. In particular, passing
runs do not explain the intermittent facade or Python-launch timeout causes.

## Earlier implementation checkpoint: packaged TypeScript host

The follow-up adds a fifth independently owned VSIX phase using a new
`scripts/vscode_typescript_host.cjs` fixture. It requires the extracted
TypeScript CLI, an explicitly selected and byte-pinned Node 22.16.0 executable,
real VS Code activation, refusal of workspace runtime/Node/CLI overrides,
protected-buffer exclusion, exact unsaved TypeScript source hashes, symbol
lookup in that overlay, unchanged saved source and stop. The original Rust
fixture remains unchanged. Final acceptance requires all five source-bound
phase receipts and independently observed natural process-tree exits.

Local typecheck passes. The complete Node suite reports 272 passed, zero failed,
three existing skips; its six fixture tests use a labeled inert VS Code
API substitute. Separately, 60 admission/lifecycle/inventory tests pass,
including missing/forged TypeScript results and failed final cleanup. These
local tests are not actual editor-host acceptance. Fresh hosted compilation,
packaging and all three real host scopes must still pass on the follow-up head.
No native/VS Code payload execution is claimed locally for this change.

### First hosted TypeScript attempt

At `7fd7a6f4`, the real TypeScript host refuses at its workspace-path
precondition: prepared settings retain `D:`, while VS Code reports `d:`.
This is a fixture comparison defect before extension activation, not evidence
of a TypeScript runtime failure or success. Both Rust host receipts pass with
natural owned-tree exits; the separate TSGo workflow steps also pass.

The correction normalizes only the initial ASCII drive letter. Different
drives/directories and opaque Unicode spellings still refuse, as do all prior
source/hash/symbol, authority and lifecycle failures. The original failure is
preserved in artifact `13894754487`, SHA-256
`9a69f3305a54ffed4676e5009c63887af40269939241ff5255fdeacdc57e05fc`.
Fresh complete hosted qualification remains required. The Python admission
implementation/tests are unchanged from their prior 60-pass local run.

## Acceptance still open

- Reconcile each of the original 353 contract identities against current source
  and evidence. The retained ledger and R1-R9 residuals are not completed by CI.
- Qualify selected real backend runtimes separately. All 80 catalog declarations
  are not runtime certification; real Roslyn/GraphQL and authenticated Google
  export retain their own explicit prerequisites and host permissions.
- Complete the remaining strict resource/DAP and full platform/feature scopes.
  A passing finite fixture does not establish unrestricted upstream equivalence.
- Complete motion and visible-shell acceptance and review visual surfaces beyond
  the five Windows dashboard flows documented above.
- Produce and verify the intended release artifacts and public publication to
  `HECer/Code-Intelligence`. A merged implementation or CI-built test VSIX is not
  a release, deployment or publisher receipt.

The dated sections of [remaining work](REMAINING_WORK.md) and [status](STATUS.md)
retain their original source checkpoints. Their old pending/main statements
must be interpreted with this baseline, rather than treated as new failures or
silently rewritten as universal acceptance.
