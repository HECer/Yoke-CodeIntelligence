# Retain prepared management refusal data after deadline expiry

The verified prior macOS dashboard-management group fails its existing
queued-expiry assertion: status408 arrives but notApplied is absent (parsed
value Null). This checkpoint identifies a concrete source path that can discard
the known result, not a measured complete explanation of that hosted failure.
The mailbox returns a correctly classified timeout, then HTTP serializes it
under the same deadline. If the thread resumes after expiry, bounded encoding
rejects it and HTTP substitutes an unconfirmed plain-text response.
The retained artifact's digest, selected log length/hash and all three relevant
old source inputs are reverified. [Prior failure proof](prior-macos.json).

The client now validates the request and prepares both at-most512-byte timeout
representations before enqueueing or waiting. The original atomic transition
alone selects the never-started or unconfirmed result. A private delivery field
carries those already admitted bytes; normal replies retain original bounded
deadline serialization. Matching JSON/status cannot mint this proof. If
preparation expires, nothing is queued and only a fixed notApplied=true refusal
with null request ID is available. HTTP keeps its original nonblocking408 write
attempt and original operation deadline. It does not retry or guarantee delivery.

The existing queued/running unit tests retain their classification/cancellation
assertions and additionally require exact JSON delivery under an expired encode
deadline. Two new native tests require prequeue refusal without publication and
refusal of expired success or JSON-shaped normal results. The original actual
HTTP queued-expiry test and100-ms budget are unchanged. These modified/new native
tests have not been compiled or executed locally; actual hosted confirmation is
pending. No native Yoke image is opened or executed below build admission.

The separate host permissions, typed actions, stale/owner checks, queue state,
operation deadlines,20-ms response reserve, response cap, completion/shutdown
ordering and browser code remain. This changes error-result transport, never
execution authority or a success result after expiry. [Product contract](../../../../native/docs/session-dashboard.md).

Local checks pass:368 native format roots, Python103/0/0 with warnings as errors,
typecheck and npm260/0/3 existing skips. Exact source/log hashes are
recorded in [result.json](result.json). Log displays use UTF-8/LF and trim trailing
whitespace, with original UTF-16 capture hashes recorded separately.
Format or Python success never certifies
native Rust typing, actual HTTP delivery, browser/current VSIX, complete native
matrix or full STATUS acceptance. Those and other platform/runtime/memory/timing
requirements remain open.
