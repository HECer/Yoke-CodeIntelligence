# Windows owned-process cleanup and diagnostics

The native Windows owner retains its root process handle independently of
fallible spawn wrappers. It assigns the suspended root to its own kill-on-close
Job before provider execution. Existing parent Job limits remain in force.

Checked cleanup first retains synchronization handles from a bounded Job
membership list, requests owned Job termination even when enumeration refuses,
and requires root exit, zero active accounting and every retained member handle
signaled within the original cleanup budget. The list stays capped at 4,096;
assigned and returned counts must agree. Member PID handles grant SYNCHRONIZE
only. Termination uses the owned Job/root handles, never a recycled member PID.
Missing/partial membership or unavailable state remains CleanupUnproven.

## Refusal observations

List-query failure or incomplete/over-capacity admission appends a bounded
memberSnapshot record to the existing typed refusal prefix:

- query_succeeded, assigned, listed, capacity and returned_bytes;
- the original Win32 query_error, captured before subsequent API calls;
- accounting_after_list as the raw total/active/terminated count tuple or a
  numeric OS error;
- root_exited_after_list from the already retained root handle or a numeric
  error. Err(None) represents an error without a numeric OS code.

Failed-query buffer fields are initialized or partial data, not an admitted
member list. Accounting and root observations happen after enumeration and are
not atomic with it. They cannot establish which member changed or repair
missing membership. No PID, command, path, source, argument or environment value
is added to the record. Its fields have fixed numeric/boolean types; only two
fixed list-refusal labels call the private formatter.

The Win32 [process-list contract](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_process_id_list)
distinguishes requested and returned identifiers, including nested Jobs.
[QueryInformationJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-queryinformationjobobject)
defines the optional returned-byte count and failure error retrieval. These
observations do not substitute for the original handle-based exit checks.

## Qualification and unresolved work

A real terminate-only Job alias deterministically fails query admission with
Win32 error 5. The new regression checks the typed refusal, bounded counters and
query/root/accounting failures. The same test fails on old production code.
All original six Job tests retain exact Git source as a prefix; their previous
CRLF working checkout reconstructs exactly from that prefix. The production
module is normalized from CRLF to LF with its diagnostic edits. Original
fixture literals, assertions, ignores, deadlines and profiles remain intact.

Current qualification passes 697 Rust functions in 11 whole groups with 8 existing
ignores, all-target Clippy, format checks for all 361 rootsting, Python 47, typecheck and
npm 260 with 3 existing skips. Actual CLI/default-row and explicit VS Code 1.143.1 host
checks also pass. Source/Job/hash evidence is linked below.

A previous benign Git probe observed assigned2/listed1 with capacity4096, but
does not prove the cause of the historical Git fixture failure. That underlying
intermittent refusal remains open; this change adds diagnosis and no retry,
larger list, extended deadline or weaker completion condition.

Separately, the fully buffered run_capture success path currently leaves cleanup
to Drop, which discards its Result. A real-error propagation regression and
repair are pending. CapturedOutput.output_complete describes stream transfer;
it must not be treated as proof of every descendant exit. This checkpoint does
not repair that path or establish an OS/provider sandbox or whole-Windows pass.
See ../../docs/validation/integration-2026-10-23/owned-member-diagnostics/README.md.
