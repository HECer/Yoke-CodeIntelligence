# Host-selected session analyzer policy

`Session::open` pins the constructor's `Engine.capture_epoch` for new source
captures in the default project and every host-registered project. This includes
projects used only for external queries and projects whose source is read-only.
Project activation, lazy project opening and session resume retain that policy.
The default constructor continues to select analyzer 8.

The host CLI can select this policy with `--analyzer-epoch 9 session ...` or
`--analyzer-epoch 9 serve-session ...`. Use the same selector when resuming that
session label. The ordinary `serve` command also retains its constructor's
selected policy for new captures through the eight native tools.

The selected epoch is a typed host setting. It adds no `HostProfile` JSON field
or MCP argument. Repository files, project names, modes and imported data cannot
select it. A caller that changes an exposed engine's epoch after opening the
session receives an error on the next session source operation; that operation
cannot silently index with another analyzer. Direct host use of an exposed
`Engine` remains subject to that engine's own API.

## Persistent identity and resume

Analyzer-8 sessions retain their entire original host fingerprint and serialized
session record. No epoch field is added to the stored state. Analyzer-9 sessions
wrap the complete existing host fingerprint with the versioned
`host_capture_epoch_v1` marker and the exact `yoke-tree-sitter-9` producer.

Session keys retain their existing workspace-and-label identity. Resuming a
label with another epoch fails the host-binding check before any session record
is written. Resume with the original epoch, or choose another label for a
separate session. Existing session records and source graphs are never rewritten
as a migration step.

## Historical and foreign data

The pinned policy applies to new local captures. Explicit immutable snapshot
queries retain the analyzer proven by their source capture. Guarded derivatives
use their base source's proven policy. A session's current capture setting does
not retag an existing graph.

Native collection archives remain self-contained and readable without the
original workspace or a fresh active-project capture. Federation receiver and
archive-access engines continue to select canonical reconstruction from the
validated receiving receipt. In particular, a V1 foreign receipt retains its
original bytes and reports its receiving epoch as `not_recorded`; a session's V9
setting does not add historical attestation. Current invocation-scoped foreign
grants are still required, and revocation takes effect on the next invocation.
Read-only and query-only project restrictions and collection registration
permissions remain independent of the selected analyzer.

`session_epoch_selection` covers original V8 record bytes, default and registered
V9 captures, same-epoch resume, cross-epoch refusal without record mutation,
mutable-engine drift, native V8 archive access and V1 foreign grant revocation.
This bounded policy does not certify universal parser or language-server parity.
