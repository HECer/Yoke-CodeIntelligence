# Host-install platform test correction

The hosted Ubuntu job for `d819d7100b06f239fae291083908e17e18d3ae35`
failed with 2,381 passed, 36 failed and 36 ignored across 302 result groups.
All 36 failures reached the explicitly Windows-only guarded host-install backend.
The original raw log and every failed test name remain in this directory.
Cargo stdout/stderr interleave, so neighboring `Running` lines cannot reliably
identify a result group. The totals come from actual test-result lines.

Successful installation/recovery tests now require Windows. All 41 original
test bodies are unchanged, as checked against `d819d71` after newline
normalization. The nine successful CLI lifecycle tests are Windows-only;
the two existing permission-precedence tests still run on every platform.
Two new Unix tests require API refusal and refusal of all six CLI operations,
without target/state mutation, source-store creation or private output.
No runtime installer function, permission gate, recovery journal or filesystem
guard changed. No Unix installation capability was added.

Fresh local Windows results:

| Check | Result |
| --- | --- |
| Host-install integration and CLI | 20 passed, 0 failed |
| Host-install library and recovery | 139 passed, 0 failed, 1 ignored |
| Format, native build, all-target Clippy `-D warnings` | Passed |
| Python contracts | 17 passed |
| TypeScript typecheck | Passed |
| TypeScript tests | 260 passed, 0 failed, 3 skipped |

The ignored Rust entry is the existing parent-spawned recovery worker, which
requires an explicitly selected owned temporary fixture. It was not newly
ignored by this correction. Each native run checked 752 unchanged input hashes;
the retained checks checked 754 unchanged runtime input hashes. Only these
scoped native tests were rerun, not the complete native suite. The new Unix
test bodies have not been executed locally and await fresh hosted verification.

[index.json](index.json) records the sizes and SHA-256 hashes of 88 raw
artifacts, including runners, frozen inputs, source variants, logs, original
test-body proof and the prior PR description. This explanatory README is
metadata outside that raw-artifact index. The separate unvalidated R8 draft
is preserved only on local branch `work/r8-app-history-integration-20261020`
at `c0cb4f0`; it is not part of this correction.
