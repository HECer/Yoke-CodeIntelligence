# Unix document test helper: CI correction

The Rust CI run [40096315149](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40096315149)
for PR head `d90a4e3587e55b8eebdfabc4647e355d91230e1f` checked out actual merge
`50a29da933e17daab3beccab626c452beafe45fc`. Both
[Ubuntu](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40096315149/job/120879209584)
and [macOS](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40096315149/job/120879209374)
stop in all-target Clippy with `function d is never used` at
`native/src/host_documents.rs:534`. All calls belong to the five existing
Windows-only tests. The helper itself lacked their platform condition.
[Diagnostic excerpts](failed-ci-excerpts.txt) retain both failures.

This correction adds only `#[cfg(windows)]` to that helper inside `#[cfg(test)]`.
The function body, five tests, cross-platform schema test, runtime code, workflow,
warning policy, test counts, timeouts and permissions are unchanged. No warning
suppression or additional skipped test is introduced.

Local checks and exact source comparison are recorded in [result.json](result.json).
Published log renderings use UTF-8/LF and remove trailing whitespace; original
local log hashes are retained separately in that result. PowerShell's rendering
of npm notice stderr as NativeCommandError does not change either actual exit0.
Native compilation and execution are not attempted because local free space is
below the existing 500,000,000-byte reserve. Therefore the Unix correction still
requires its own hosted Clippy result; the Windows archive live gate, complete
native/platform acceptance and full STATUS remain open. Earlier native counts
keep their original source scopes.

The six Node jobs for the preceding PR head all report success in
[CI run 40096315163](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40096315163).
That is evidence for the preceding head, not a passing result for this correction.
