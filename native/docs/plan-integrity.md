# Saved native plan integrity

Approval, new application, syntax/host/container verification and recovery replay
a saved native plan before consuming its effect. A modified or ambiguous saved
plan is refused before that operation publishes approval, starts verification
or writes source. This protects the correspondence between the displayed review,
saved approval and captured effect when plan storage is damaged or modified.
It does not introduce an operating-system boundary against another process with
the same account or unrestricted access to source and state.

## Compatibility and checks

The released plan serialization and identity formula are unchanged. The ID is
the SHA-256 identity of the base snapshot ID, result snapshot ID, ordered planned
files and creation time. The stored key and plan ID must agree. The existing
24-hour approval window must equal creation time plus 86,400 seconds, without
overflow. Loading a valid historical plan is separate from admitting a new
approval or application; recovery still permits valid historical plans.

Both snapshots must belong to the open workspace. The result must be the
derived snapshot for the proven base analyzer epoch, with the original omission
records and derived-source consistency marker. Sorted, unique normalized file
manifests are replayed. Their complete byte delta, original or default mode, and
ordered planned files must agree; unchanged hashes cannot conceal changed file
metadata. Captured source policies independently check membership, host write
scope, byte counts and content hashes.

The original unified diff is reconstructed from exact UTF-8 source bytes, with
the original headers and three context lines. Test-like path warnings are
reconstructed independently. Both must equal the saved review. Supplied plan
objects for syntax verification must also equal the checked saved plan.
Duplicate JSON keys, unknown plan/file fields, malformed values, foreign
snapshots, missing data and inconsistent metadata refuse the operation.

An already completed idempotent application still returns its existing receipt
without applying the plan again. Recovery keeps its existing conditional
per-file restoration and preserves nonmatching foreign writes. No new
directory operation, provider reference completeness or usage proof is implied.

## Bounds and request policy

Plan JSON is limited to 64,000,000 bytes and each selected snapshot row to
32,000,000 bytes before decoding. Each snapshot retains the native capture
ceiling of 100,000 files, each at most 4,000,000 bytes; a plan has 1–2,000 changed files. Distinct selected
before/after source objects share a 128,000,000-byte admission ceiling before
body reads. The reconstructed diff is limited to 16,000,000 bytes.

Preparation counts the exact serialized plan and snapshot bytes and distinct
selected source sizes against the same limits before publishing the derived
snapshot, graph, inherited policy or plan. Escaped JSON bytes count separately
from source/diff bytes. Earlier edit construction can leave CAS blobs on a
refusal, retaining the existing nontransactional publication boundary; source
files remain untouched.

The loader uses one read transaction when it owns transaction admission and
preserves an existing caller transaction. Its temporary SQLite cell limit,
busy policy and Store deadline are restored on success and refusal. The
requested deadline intersects the existing Store deadline and retains both
sets of host cancellation flags; a new request cannot extend an active one.
Approval publication stays inside that effective request policy. The CLI
approval path uses the supplied command deadline.

These are finite data and cooperative request bounds, not an operating-system
memory or scheduler quota. Existing snapshot policy replay and synchronous
diff computation retain their documented implementation boundaries.

## Evidence scope

Owned fault-injection tests first reproduced acceptance of altered application
bytes, cleared test warnings, altered review text, a changed stored identity
and changed recovery bytes. Their prerequisite verification rows are explicit
host fixtures; they do not assert compiler execution.

The regression suite covers analyzer 8/9 compatibility, exact Unicode/CRLF
bytes, ordinary apply/recovery, strict JSON and size limits, complete manifest
deltas, caller transaction and SQL policy restoration, deadline/cancellation,
historical expiry and refusal before an explicitly selected owned verifier
can start. Validation results and retained failures are recorded separately
from the contract. No independent security audit or remote exploit is claimed.
