# History live worker: specific Job admission

The preceding CI driver admitted its internal `--worker` entry using Windows
and disk preconditions, but did not independently check its assigned Job before
opening the selected native image. The normal parent already created the worker
suspended and assigned it before resume. A direct internal entry lacked that
containment check.

The parent now creates a fresh random named Windows Job; an occupied name refuses.
The worker requires that exact job name and queries its own membership with a
non-inheritable, query-only handle. Failed open/query, a false membership result
and failed handle closure all refuse before native path/hash/process use. The
query handle closes before continuing. The parent requires the worker admission
receipt as part of completion. Original suspended assignment, kill-on-close,
180-second deadline, output capacities and 500,000,000-byte disk floor remain.
Same-account host code retains its existing authority.

The Windows API contract uses [JOB_OBJECT_QUERY](https://learn.microsoft.com/en-us/windows/win32/procthread/job-object-security-and-access-rights),
[OpenJobObjectW](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-openjobobjectw)
and [IsProcessInJob](https://learn.microsoft.com/en-us/windows/win32/api/jobapi/nf-jobapi-isprocessinjob).
No Job assignment or control rights are requested by the worker's query.

**16 Windows admission/evidence tests pass**, including the preceding11, without
reading or executing a native Yoke image. Four new functions exercise malformed
names, unavailable/foreign Jobs, query failure versus a true output parameter,
failed query-handle closure and direct-entry ordering. The fifth creates only an
empty temporary Windows Job: the actual API refuses the unassigned Python caller,
then closes both query and creator handles. That API test is explicitly skipped
on other operating systems; it assigns no process and does not certify a live
assigned native worker.

The direct-entry regression against preceding source `74b44255` reaches a blocked
native-hash callback before any actual image read. This is a simulated admission
counterexample, not native execution. The corrected test refuses at Job open.
See [RED receipt](red-result.json), [diagnostic](red-diagnostic.txt),
[Python output](python-tests.txt) and [result](result.json).

Local typecheck and npm260/0/3 existing skips also pass. Published log renderings
use UTF-8/LF and remove trailing whitespace; original local hashes are separate.
Native Rust source and the workflow remain exact to preceding `74b44255`.
No native build/CLI/live run occurs below the unchanged disk reserve. The source
comparison of actual CLI/HTTP fields found no interface mismatch, but supplies no
runtime certificate. Actual hosted archive acceptance, native browser/current
VSIX and full STATUS remain open; earlier counts keep their own source scopes.
