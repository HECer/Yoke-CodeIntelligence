# Reinstall scoped SQLite deadline waits before publication SQL

The verified prior macOS digest-handoff group fails the original assertion that
a60-ms contended send returns within one second. The test sets the caller's
built-in busy timeout to777ms and holds a separate immediate writer transaction.
This checkpoint retains that test, its assertions and budgets unchanged.

The source/publication policy previously updated Store deadline data without
reinstalling its callback before direct SQL. SQLite's
[busy-timeout contract](https://www.sqlite.org/c3ref/busy_timeout.html) explains
that setting a built-in timeout clears a prior custom busy handler. Its
[callback contract](https://www.sqlite.org/c3ref/busy_handler.html) allows only one
handler per connection. Updating the old callback's private data cannot reactivate
it. This is a concrete source defect; the complete hosted timing cause is not
measured by the existing failure log.

The scoped policy now intersects the requested deadline with the Store's current
ceiling and calls the existing Store deadline check before running direct SQL.
That check reinstalls the owned deadline-aware callback. It still restores the
saved numeric busy timeout, SQL length limit and prior Store deadline on success
or refusal. Scoped consumers receive that effective deadline for their own
precommit checks; existing plan/approval callers already select the same
intersection. Final checks preserve typed owned-process cleanup uncertainty.
Error priority otherwise uses the effective inherited deadline. It adds no
network/write authority or transaction over remote work and changes no native
test timeout, Cargo profile, OS setting or workflow.

Three new native unit tests check callback reinstallation and restored numeric SQL
settings on both success/failure, expired inherited-ceiling refusal before the
closure executes, and forwarding the shorter Store ceiling to a scoped consumer.
They are not compiled or run locally. The existing actual
digest contention test remains unchanged and awaits hosted confirmation.
The internal policy callback now receives the effective deadline; thirteen
publication-policy call sites and the worker's direct source-policy consumer
are adapted. Two plan/approval callers already hold the same intersection.
The adapters forward the effective deadline without changing serialized
contracts. Current full native typing/regression qualification is open.

An actual [owned C-API probe](inert-sqlite-probe.py) loads only the SQLite3.50.4
DLL supplied with this Python installation, creates two private fixture
connections and holds its own WAL writer lock. Without callback reinstallation,
the nominal60-ms observation invokes the custom callback zero times and waits
about0.948s. With reinstallation it invokes the callback22 times and returns
SQLITE_BUSY after about0.061s. Both restore busy_timeout=777 and all owned
connections close before the fixture is removed. No Yoke image, native Rust
Store, bundled Rust SQLite, existing credentials or analyzed code is used.
[Actual observations](inert-sqlite-probe.json). This is API-mechanism evidence,
not native/macOS or service acceptance.

Local checks pass:368 native format roots, Python103/0/0 with warnings as errors,
typecheck and npm260/0/3 existing skips. Two Python attempts overlap and are not
added. TypeScript/Python runtime inputs remain unchanged through the final Rust
callback propagation. The [prior failure proof](prior-macos.json) rechecks the
artifact digest, selected log and exact old guard/test inputs.

Two already tracked source files retain their original CRLF bytes; only each
callback argument changes. Default git whitespace checking flags those CR
endings; a process-local cr-at-eol check passes, with no Git configuration change.
Log displays use UTF-8/LF and trimmed trailing whitespace, retaining original
capture hashes. All source/log checks are in [result.json](result.json).
The native changed/new tests, current
matrix, remaining platform/runtime/memory/timing, archive live, current VSIX and
full STATUS acceptance remain open. Local native compilation/execution stays
closed below the1.2-GB full-build admission threshold.
