# Google Workspace and complete baseline evidence

The [checkpoint](checkpoint.json) binds 33 passing scoped Rust tests, format,
all-target Clippy with warnings denied, retained typecheck and TypeScript
260 passed / 3 existing skips. The immutable source inventories and real logs
are indexed by [exact bytes and SHA-256](index.json). This README explains that
index and is not one of its indexed raw files.

`r7/r1` retains the initial owned-process fixture failure. Its Windows working
directory assertion compared distinct path spellings; `r7/r2` checks the physical
canonical directory and passes all 33 tests. Its Clippy failure is a needless
borrow in that fixture. `r7/r3` rechecks format, the changed process test and
all-target Clippy successfully. Exact reconstruction verifies that only that
borrow changed between r2 and r3; other passing tests are not counted twice.
Runtime inputs remain unchanged after validation; later changes are documentation.

The actual pinned gws 0.22.5 client separately refuses empty selected state with
an explicit absent ADC path, exit 2, and creates no export. No account credential
was inspected/copied. This qualifies its negative credential-selection contract;
resource-key transport and a successful authenticated Google export remain open.
No model call or analyzed-source execution is requested by the adapter.

The complete earlier native baseline at `6951ff9` attempted all 310 registered
groups on 927 unchanged inputs: 2,904 passed, ten OS1314 symlink-setup failures,
53 ignored. Tools/CLI remain unchanged and examples compile. The additionally
linker-blocked `digest_history_jobs` passes all thirteen original tests after
changing only its output suffix under the normal test profile. Its first
alternate-build command missed the package selector; both failed and corrected
attempts are retained. The original locked output/processes are untouched.

The [failure assessment](baseline/assessment/report.json) and
[ignored-case inventory](baseline/assessment/ignored-cases.json) preserve the
remaining Windows/runtime/capacity gates. The baseline predates R7; scoped checks
do not establish a whole current-head Windows or hosted pass. Full R1-R9 scope,
upstream equivalence, production deployment and release acceptance remain open.

The first retained test run in the sparse R7 checkout lacked benchmarks/extensions;
after extending the checkout, the complete suite passes. Its actual PowerShell
log is retained byte-exact, including its original encoding. The typecheck exit
was observed in the tool transcript; no stdout log was reconstructed.
