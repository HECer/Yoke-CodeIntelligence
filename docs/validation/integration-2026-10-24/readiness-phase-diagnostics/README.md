# Readiness and scheduling failure diagnostics, native acceptance pending

The verified c11511ee Windows shard zero has43 whole groups with42 zero exits
and one failed group. Its editor HTTP integration target records8 passes and
two failures, both at the existing3-second endpoint-announcement wait before
the slow-header/body request tests begin. Actual checkout and warnings-denied
all-target Clippy pass at that old source. These are separate historical scopes,
not a current-head native certificate.

The fixture now records only an atomic startup stage: worker entry, Engine open
entry/completion, announcement write and announcement sent. On the unchanged
3-second receive failure, it reports that stage, elapsed time, channel failure
kind and whether the worker has finished. No root path, token, endpoint, source
or request data enters these new diagnostics. Normal handshake bytes and request
behavior are unchanged. The engine remains inside the worker, so no setup work
is moved outside the original readiness budget.

Two other existing macOS failures gain failure-only scalar diagnostics. The
65-ms history test retains its40-ms fixture sleeps, exact(1,1) request-count
assertion and200-ms elapsed assertion; failure additionally reports elapsed,
configured delay and remaining host time. The partial-header ingress test
retains120-ms request timeout,25-ms drip, at-least-two sends and750-ms elapsed
assertions; failure reports sent count, elapsed and response byte length.
No assertion, test function, ignore, Cargo profile or workflow changes. These
changed Rust tests have not been compiled or executed locally.

The [prior failures](prior-failures.json) retain exact archive/log hashes and
matching selected old source inputs. They establish the failures, not their
unmeasured startup/scheduling causes. Current native fixes keep their separate
pending acceptance. Three changed files pass rustfmt; Python103/0/0 with
warnings as errors, typecheck and npm260/0/3 existing skips pass separately.
Results are in [result.json](result.json); none substitutes for native execution.
Log displays use UTF-8/LF and trim trailing whitespace, with original capture
hashes recorded separately.

The separately verified [Linux memory diagnostic](linux-memory.json) at c11511ee
compiles the owned C fixture and observes exact256-MiB limits plus denial of
512-MiB malloc/mmap in both direct and inherited trials. Cleanup proves group
absence. This is owned C/API data only; nativeAcceptance remains false. No actual
macOS diagnostic artifact was available at inspection.

All ten previously requested superseded CI cancellations are now terminal and
confirmed. Every selected job was queued/unassigned/without steps immediately
before each request; mixed, running and completed runs were retained. The
[confirmation](queue-cancellations.json) is not a test pass or current-head
cancellation. The new/current native acceptance is still required.

Local disk remains below the1.2-GB full-native-build admission floor. A read-only
Git inventory finds only18930KiB of loose objects versus about124MB missing;
no Git compaction, native image read/execution or protected artifact operation
is attempted. Native matrix, Windows startup, macOS/runtime/memory/timing,
history live, current VSIX and full STATUS acceptance remain open.
