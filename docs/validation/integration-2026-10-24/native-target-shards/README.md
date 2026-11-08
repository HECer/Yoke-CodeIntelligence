# Whole-target CI scheduling: configured, acceptance pending

The earlier Windows [job120870346935](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40093333004/job/120870346935)
is terminal/cancelled. Its log starts at12:17:43UTC and cancellation occurs at
13:02:47UTC, still during compiler-move tests. Before cancellation it records
three real failures: the explicit REPL shell operation exceeds its20-second
deadline; two selected-shell source-hook assertions fail. Those failures are
not fixed or discarded by this scheduling change. [Diagnostic excerpts](prior-failures.txt)
retain the actual failures. The relevant implementation/test inputs were exact
between that product and preceding head504963a6.

Current metadata declares368 targets. The default contract becomes369 whole
groups:365 test executables, three compile-only examples and one library doctest
invocation. The [owned scheduler](../../../../scripts/native_shards.py) distributes
these groups over eight jobs per platform using deterministic source-file-size
weights. No individual test function is selected or filtered. Example targets
with test=false are compiled without running them; library doctests remain a
separate invocation. Default disabled tests and ignores retain Cargo's behavior.
See the [Cargo target contract](https://doc.rust-lang.org/cargo/commands/cargo-test.html#target-selection).

Each job retains the45-minute CI limit. Windows uses the original single libtest
thread, Unix two; internal concurrency adversaries, test assertions,20/30-second
operations, profile settings and locked Cargo inputs remain. The controlled
runner still removes only ambient TLS overrides at the executable boundary;
compile-only examples receive the same cleaned environment. Each shard has an
additional44-minute controller deadline and the existing500MB admission floor.
Logs have16MB per-file admission and32MB per-shard admission bounds. These are
application checks, not OS disk reservations or a new process sandbox.

Platform jobs retain the original `native-rust (OS)` identities. Before the
existing actual rust-analyzer gate, release build/upload and Windows live driver,
the checker requires all eight same-platform receipts at the exact checkout and
selected source hashes, each assigned group exactly once, actual zero exits and
the preserved log lengths/hashes. Missing, failed, stale, duplicate, substituted
or changed-source receipts refuse. Selected source inventory is explicitly not
a complete toolchain/build-input inventory. Function counts are never inferred
from target counts. The existing coverage/parity certificates remain unchanged.

The workflow execution layout changes; the earlier statement that its complete
prefix remained byte-identical belongs to the historical host-history gate,
not this checkpoint. Warning/permission gates and original test/profile inputs
are retained. Additional kinds, feature-gated targets and non-ASCII Cargo target
names require explicit reconciliation and fail closed. Unicode source paths
and exact source bytes remain supported.

**11 scheduler tests pass**, including actual Cargo metadata without any build,
plus synthetic failure/continuation and artifact-log tampering checks. All16
preceding host-history admission tests pass separately. Neither count is native
Rust execution. The source-hook tests gain failure-only diagnostics while keeping
their original assertions and deadlines. Local rustfmt/typecheck/npm260/0/3
existing skips pass. [Result](result.json), [plan](plan.json) and recorded logs bind
these scopes. No native Yoke image is opened or executed below the local reserve.

Actual partitioned CI execution, complete native/platform/live/archive/browser/
current VSIX/main/release and full STATUS acceptance remain open. The preceding
six-job Node and three-platform Clippy successes retain source504963a6; they are
not new hosted results for this checkpoint. No system setting, privilege, old
failed artifact, protected binary, default provider or model call changes.
