# Windows PowerShell phase observations

The actual historical Windows failures remain unresolved. Verified D179 push
artifacts show the managed-known-folder test reaching its original20-second
limit with incomplete output and the selected-shell REPL cell exceeding its
original request deadline. The C136 PR artifact repeats the REPL failure, with
actual merge checkout15fbf432 rather than head c1366e8d. All selected archive
sizes/digests and group-log bytes/hashes are verified. Full source input maps are
not independently certified. See `prior-failures.json` for exact source scopes.

Five initial local inert environment comparisons complete in0.17-0.22s. Adding
managed Windows environment names gives no observed improvement. Four separate
final owned process trials compare one/two Job assignments, primary-thread or
Toolhelp snapshot resumption and plain output/managed-folder commands. Every
root is created suspended and assigned before resume. Each observed primary
thread has a positive previous suspend count, every root exits0, both streams
reach EOF, and owned Job accounting reaches zero before forced cleanup. Actual
local legacy PowerShell version is recorded in stderr. These trials finish in
about0.2s. They do not reproduce or explain the hosted20-second failure.

`scripts/host_powershell_probe.py` runs only fixed owned literal commands through
the selected system PowerShell path. The environment matches the native host
allowlist; no credential, profile, module-path or arbitrary-command input is
added. There is no Yoke image read/execution. The diagnostic requests no model/
network operation or host-setting change; the selected runtime remains a trusted
host process. Each trial keeps20s, bounded65,536-byte stdout/stderr retention, an owned
cleanup budget and an explicit500MB disk admission. Snapshot enumeration opens
and resumes only threads whose owner matches the newly created own process.
Observed threads/entries and logs have finite capacities. Cleanup uncertainty
is saved, stops further trials and cannot become a complete diagnostic. An
exit0 root with descendants left for forced cleanup is also incomplete.
`diagnosticComplete` only classifies these observations; `nativeAcceptance`
is always false. No OS sandbox, formal timing or native runtime certification
is implied by completion.

The workflow invokes this diagnostic only after the original native test step
fails on Windows shards5/7. Its separate JSON artifact does not match native
coverage inputs. Original45-minute jobs, shard coverage, warning gates, process
limits and native acceptance steps remain. The diagnostic has its own two-minute
step limit and does not restart or replace any native group.

Two existing Rust tests gain private marker files without changing original
assertions or20s operation limits. The process test records entered/before-folder/
after-folder/finished. The REPL test separately records Python-cell and shell
progress; its markers live outside the analyzed project. Paths cross JSON and
PowerShell literal boundaries with explicit quoting. At most64 marker bytes
are inspected, and only fixed numeric stages are printed. Stage0 means no
recognized marker was observed, including missing/partial/unreadable writes;
it does not prove that the process or script never started. Marker file writes
are diagnostic fixture work inside the original request window. They add no
production runtime behavior or permission.

**The changed Rust tests were not compiled or executed locally.** Disk remains
below1.2GB native build admission; protected/old executables are not substituted.
Actual hosted phase/Job observations and a passing original native suite remain
required. Current platform/history/browser/VSIX, full STATUS, main and release
acceptance remain open.

Local checks pass113 Python tests, zero failures/skips, including10 new tests
(eight pure/fault cases and two actual owned Windows cases). Coverage includes
credential/environment exclusion, source-revision and disk admission, preserved
existing output, failed assignment, failed termination despite empty accounting,
descendants hidden by forced cleanup, persisted cleanup uncertainty and actual
Unicode/apostrophe/semicolon phase paths. The initial whole Python run lacked
Cargo in its process PATH, causing two host-precondition failures and one skip;
the corrected full run above is separate and nonadditive. Typecheck, npm260/0/
3 existing skips, changed two-file rustfmt and diff checks pass. See `checks.json`.

Reproduce only the separate Windows diagnostic with a fresh output directory:

```text
python -B scripts/host_powershell_probe.py --output validation/owned-powershell-observations
```

`owned-jobs.json` is the actual final local report. `environment.json` retains the
separate earlier environment comparison; its outer assigned worker also exits
before forced cleanup. These are independent scopes, not native test totals.
