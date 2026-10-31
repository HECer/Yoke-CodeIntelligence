# Owned Windows VS Code launcher lifecycle

`test-vscode.ps1` retains its explicit host-selected Code/native/extension paths,
private workspace and editor profile, bundled-runtime mode, and original
`extensions/vscode/test-host.cjs` assertions. It now selects a Python 3.13+
application (optional `-PythonExecutable`) and passes a create-only, bounded
host specification to `vscode_owned_job.py`. No PowerShell execution-policy
change is required or performed.

The helper accepts only an absolute local selected `Code.exe`, exact fields and
the original fixed argument sequence. Relative/UNC/ADS/device/traversal/aliased
paths, links, duplicate/unknown JSON fields, non-JSON constants and expanded
deadlines refuse. Unicode and spaces remain separate arguments through Windows
`list2cmdline`, without a shell. The Code executable must be outside the private
fixture. The helper is a host test tool, not permission to execute analyzed code.

Code starts hidden and suspended, joins a fresh named Windows Job, then resumes.
The Job has kill-on-close and no breakaway grant. Only the selected standard
input/output/error handles are inherited. HOME/profile/cache/temp environment
paths are private; ambient Electron/Code IPC and portable-profile overrides are
removed. This requires the 64-bit Windows Job backend.

Acceptance requires all of:

- successful suspended assignment and a zero root exit;
- an empty inherited tree observed before forced cleanup, allowing at most one
  second for accounting to settle within the original deadline;
- checked Job termination, zero remaining members and all owned handles closed;
- the unchanged extension-host result, separately admitted after process proof,
  with Rust/bundled scope and all original assertion flags true.

A passed marker cannot override a nonzero exit, live descendant, deadline or
incomplete cleanup. There is no PID/taskkill fallback. Cleanup attempts every
remaining handle even if one close fails. A partial receipt refuses success.
`job.json` contains only fixed scalar categories and counts, not arguments,
capabilities, profile contents or output logs.

The host deadline remains at most 180 seconds. Cleanup has its own ten-second
drain. Checks retain a 500 MB disk reserve; the new private fixture bound is
128 MB / 20,000 entries / depth 32. Source/spec/path/stat/fixture scans remain
cooperative parent operations, not a proof of interruptible filesystem I/O or
protection from another process with the same account privileges.

## Separate hosted lifecycle probe

The additive CI job runs `python scripts/vscode_owned_job.py --ci-probe` only on
explicit 64-bit hosted Windows at the exact `GITHUB_SHA`. It creates four private
standard-library Python scenarios:

1. a root and child prove membership in the specific Job, close their query
   handles, and exit naturally;
2. a nonzero root must refuse acceptance;
3. a zero-exit root writes a passed marker but leaves a child alive: acceptance
   must refuse before checked cleanup removes that child;
4. a deadline must refuse and clean up its owned tree.

Unicode/space fixture paths also exercise actual Windows argument transport.
The probe runs no Code executable, Yoke image, VSIX or project worker. Its text
receipts are uploaded separately as `vscode-owned-job-windows`. Four successful
probe assertions mean the four expected positive/negative scenarios behaved as
required; they do not mean four successful extension-host sessions.

## Qualification still required

Pure admission tests and source/parser checks can run locally. The actual probe
has not run locally; its kernel and process-tree behavior needs hosted evidence.
The existing extension test source and native packaging/importer limits remain
unchanged. This change does not produce or qualify a current native image or
VSIX, attest an editor binary, install an extension, or accept a release.

The next VSIX stage must still bind a fresh matching release producer, the
original native and combined native+TypeScript packagers, package manifests and
checksums, a reviewed selected editor, and both explicit and bundled real
extension-host executions. Their original source/authority/protected-buffer
assertions must pass with process proof. Combined-source parity, platform,
main/publication and release gates stay open.

The [fresh VSIX CI profile](host_vscode.md) now implements that next stage behind
the same-run lifecycle probe. Its hosted production and real-editor outcomes
remain unqualified until their own complete receipts are verified.
