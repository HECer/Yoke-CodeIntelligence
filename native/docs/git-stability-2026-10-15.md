# Windows Git workflow stability investigation

The handoff reported one deadline failure in
`deleted_and_renamed_commit_sources_keep_base_symbols_and_exact_original_paths`
during the preceding whole-suite run, followed by an isolated pass in 2.85 s.
The original whole-suite log was not available in the checkout. That report is
historical evidence, not a newly reproduced failure or a proven production cause.

On 2026-10-15, the unchanged Git workflow target was exercised with the default
Rust test concurrency on a Windows host with 32 logical processors:

| Invocation | Result | Test execution time |
| --- | --- | --- |
| `rtk cargo test -p yoke-code-intelligence -j 2 --test git_workflows -- --nocapture` | 17 passed, 0 failed | 31.79 s |
| Direct invocation of the freshly built test executable, default concurrency | 17 passed, 0 failed | 31.95 s |
| Same executable, `--exact deleted_and_renamed_commit_sources_keep_base_symbols_and_exact_original_paths --nocapture` | 1 passed, 0 failed | 3.38 s |

During the second concurrent run, the deleted/renamed test completed 30.28 s
after process start. This includes repository setup before the request's 30 s
deadline starts; it is not a measurement of request duration. The same binary's
isolated result demonstrates considerable sensitivity to concurrent fixture
load. Another Cargo build was running during that direct concurrent run, so this
does not isolate CPU scheduling, allocation, storage, or process-start overhead.

These initial checks reproduced no failure. A later combined targeted run,
with other agents' Cargo builds still active, reproduced four failures in the
17-case `git_workflows` target after 42.98 s. They all reported
`Git metadata process exceeded deadline/output capacity or failed IO`:

- `historical_git_source_proof_rejects_missing_tree_membership`
- `immutable_git_report_renderings_and_digests_remain_bound_after_live_changes`
- `typed_git_records_and_exact_commit_artifacts_are_known_retention_roots`
- `unicode_refs_resolve_to_exact_commit_without_argument_interpretation`

The deleted/renamed case passed in that run. The same combined invocation passed
the new epoch-selection target (2 tests), immutable Git epoch regressions (9
tests), and Git admission review target (6 tests). Full output was retained by
RTK as recall `10ad95b82de7` during this session. That local recall identifier is
session evidence, not a portable validation artifact.

The identical built `git_workflows` executable was then invoked with
`--test-threads=4 --nocapture`, while competing builds remained active. All 17
tests passed in 44.51 s. This isolates fixture concurrency as one effective
control; the generic process error does not independently identify its timeout,
output-capacity or I/O flag. CPU scheduling, allocation, storage and process-start
costs were not separately profiled.

The semantic/admission test harness now permits four real Git fixtures at once,
using an RAII permit held through fixture cleanup. Request deadlines begin after
fixture acquisition. The Rust runner still uses its default concurrency. This is
a test-resource bound, not a production request limit or a concurrency benchmark.
The already queued normal-runner retest completed before coordinator cancellation:
`rtk cargo test -p yoke-code-intelligence -j 2 --test git_workflows -- --nocapture`
passed all 17 tests in 47.92 s with the changed fixture harness. No request timeout
was increased. The coordinator's final combined/whole-suite gate remains separate.

No production deadline was increased. The request's shared deadline, process
cleanup, output limits, immutable source proofs, and refusal of hooks, filters and
implicit fetch remain unchanged. Future failures should retain the complete
command, test output, concurrency, request phase and elapsed time before choosing
a further repair. These targeted checks do not replace whole-suite validation.
