# Integration development evidence, 2026-10-19 local

This owned evidence records the combined Wave 14 / GitHub App checkpoint and its
subsequent CI/test repairs. It is not release or universal-parity acceptance.
`index.json` binds the selected raw artifacts and their precise scopes.

The frozen baseline at `6dea3919` executed all 255 native targets: 2,272 passed,
11 failed and 37 default ignored; zero doctests were registered. Source hashes
were unchanged. Ten failures occurred while constructing real Windows symlinks
(OS 1314), and one fixture incorrectly required EOF rather than also accepting
a reset/aborted TCP connection after cancellation. No security fixture is skipped
or weakened to hide the unavailable host prerequisite.

The nine-file repair delta preserves the existing non-Windows refusals, limits
Windows-only private implementation code to its actual platform, pins CI to Rust
1.94.1, fixes socket-closure observation and explicitly selects real passive
profiles for the older live navigation gates. Its actual format, Clippy,
library/worker and installed-server results are separate from the baseline.
The baseline source map plus `repair/source-delta.json` reconstructs the physical
source hashes checked by the repair run. Status/evidence metadata follows it.

The native browser listener was visually inspected at desktop and mobile sizes,
including Unicode search, directed call evidence and empty search. It stopped by
its owned marker; SQL records, CAS and source stayed unchanged. Its META CSP
console diagnostic remains disclosed; the native HTTP CSP header carries the
frame-ancestors directive. This is an owned fixture, not a live App installation.

All original red/intermediate attempts and the complete raw target/build logs
remain locally in ignored `validation/integration-20261018`. The committed bundle
contains the complete 255-target receipts and selected raw logs, not a claim that
missing platform, provider, proprietary, benchmark or deployment gates passed.
Historical parity ledgers and their false full-contract certifications are kept.

The [registry coordination follow-up](registry-coordination/index.json) adds an
explicit host permission for a pinned read-only SQLite/WAL transaction on Unix;
Windows retains its stronger sharing guard. The default policy serialization
and strict no-sidecar path are preserved. Five local SQLite tests pass; all nine
affected CLI/session targets run with 55 passes and one actual Windows symlink
setup failure (OS 1314). This follow-up is not an accepted whole-native run.
Actual Unix execution of the new path remains a hosted validation gate. The
separate debugger/REPL fixture repair replaces Windows-only tool discovery and
shell selection with explicit platform profiles while retaining all assertions.
Those fixtures pass 28 tests with one unconfigured live debugpy gate ignored;
fresh typecheck and TypeScript tests pass 254/3 skipped. Source hashes remain
unchanged within both follow-up validation runs. Only the two fixture files
differ between the registry and portable-fixture source manifests.

The [captured-history mapping follow-up](digest-mapping/index.json) adds explicit
snapshot selectors to the existing verified Git history. File relations retain
Git object/mode facts; symbol groups retain exact original source anchors and
explicit syntax-candidate/ambiguity/unavailable states. Frozen broad validation
passes 515 native tests / 7 ignored plus TypeScript 254/3 skipped and typecheck.
The final row-admission refinement passes 409 native / 7 ignored; the two runs
overlap. Both source freezes and their two-file runtime delta are preserved.
The [actual hosted d4 platform runs](platform-d4/index.json) remain failed with
Windows 2283/6/37, Linux 1944/38/23 and macOS 1912/70/23. No full-native or
platform acceptance, release, deployment or main merge is claimed.
