# Distinguish HTTP completion paths in the backpressure fixture

The verified macOS shard 2 at source `52b746dcb313e3f0f26a3d4538456cb1285f6c5e`
fails the original backpressure assertion with `status=None`, 131,072 accepted
prefill bytes and 257.353791 ms elapsed. Its library result is 477 passed,
two failed and six original ignores. [Job](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40312779978/job/121537817742).
The retained [artifact index](prior-library-index.json), exact library streams
and complete shard receipt bind the actual PR merge and original source hashes.

`status=None` means no returned `Failure`; it does not identify the HTTP status.
`respond` can select HTTP 408 after the original deadline expires during
serialization, attempt one nonblocking timeout response and return `Ok`.
Ordinary response completion also returns `Ok`. Elapsed time is sampled after
the result is inspected, so this observation does not identify either path.
Accepted prefill bytes and an earlier `WouldBlock` do not prove the later queue
occupancy. No production deadline or socket cause is established by this sample.

Source `771833bb5180f3d51e092e64de5fd22c783c920a` adds a private test-only success
receipt at the two existing `Ok` sites. It records the selected HTTP status,
serialization success and serialized byte count. The fixture copies those
scalars before consuming the result and formats them only on assertion failure.
They identify the chosen response path; they do not prove delivery to the peer.
Non-test success remains `()`. There is no observer, extra clock sample, socket
operation or production branch change.

The original 250-ms response deadline, 4-MiB body, 8-KiB buffer requests,
64-MiB fill cap, observed `WouldBlock`, thread count, three-second bounds,
peer-shutdown cleanup and `assert!(timeout)` remain intact. An `Ok` result still
fails that assertion. The [static restoration check](static-restoration.py)
removes only the explicit diagnostic additions and restores the entire original
source exactly; this is a source comparison, not compilation or timing evidence.

The separate library failure records a requested 40-ms sleep returning after
188.18625 ms against the same 65-ms deadline. No contents request follows; the
fixture fails `(1,0)` versus `(1,1)`, while elapsed 188.242334 ms remains below
its original 200-ms bound. This is the known real-clock acceptance limitation,
not evidence of a refreshed production deadline.

The same notification includes shard 4's 11 typed PDF address-space failures and
six template failures (five generic spawn `EINVAL`, one explicit memory-limit
refusal). Shard 7 has six prompt-runtime failures: two generic spawn `EINVAL`,
one explicit memory-limit refusal, two `Initialize first` responses and one
missing response. The last three remain unattributed. All 280 logs across these
three partial shards were size/hash checked; [scoped receipts](hosted-failures.json)
do not claim complete platform coverage or assign every generic spawn error to
one API. The explicit PDF observations request 268,435,456 bytes and report
`ResourceLimits/SetAddressSpace`, errno 22, before successful worker operation.

Scoped Rust 1.94.1 formatting and diff checks pass. At the exact clean new source,
typecheck passes and npm reports 260 passed, zero failed and three original skips.
All 1,135 selected source files and seven controls remain unchanged; both owned
Node trees exit naturally before forced cleanup. [Checks](checks.json) retain
the selected Node streams, phase receipts and before/after maps. The driver is
the previously checked guard with only its source commit and fresh run ID
changed. The large private Git listing and other Git subprocess receipts are
not included in this selected public packet.

The changed native library, warnings-denied all-target Clippy and actual macOS
diagnostic remain unrun locally. This patch is diagnostic work; the backpressure
test, macOS worker capability, full platform, project and release gates remain
open.
