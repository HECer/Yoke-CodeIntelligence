# Independent received-declaration checks

The separate host-local `service::receiver::verification` contract compares an
accepted digest's advertised Rust declaration projection with an independently
replayed, explicitly selected local capture. Receipt acceptance and the existing
literal [metadata query](digest-query.md) still confer no source authority.
This operation starts no model, network request, compiler, Git command or project
code and persists no verification record.

```sh
yoke --root /host/receiver-state --max-bytes 4000000 digest-inbox verify \
  --profile /host/receiver.json --principal sender \
  --request /host/verification.json \
  --source-root /host/independently-captured-project \
  --source-profile /host/app-scope.json \
  --allow-query --allow-source-verification
```

The receiver principal must currently have `allowQuery:true` for the exact
App/sender-workspace/installation/repository scope. The separate source App
profile must permit that repository under the same App ID. Its signing credential
name is validated but no key is loaded. Both roots must already contain valid
native Stores; the source root and all host-file paths are absolute. Both Stores
open read-only, with no Engine, schema creation or source discovery. SQLite WAL
coordination may still use sidecars. Existing global source/provider/write flags
remain unavailable. No HTTP verification route or default service permission is
added; ordinary query wire bytes and false authority labels remain unchanged.

The closed request selects one received proposal, its exact receipt and one local
snapshot. IDs below are placeholders and must be replaced with observed IDs:

```json
{
  "schemaVersion": 1,
  "appId": 123,
  "senderWorkspaceId": "<64 lowercase hex sender workspace ID>",
  "installationId": 42,
  "repositoryId": 9,
  "repository": "owner/repo",
  "proposalId": "<accepted 64 lowercase hex proposal ID>",
  "receiptId": "<exact 64 lowercase hex receipt ID>",
  "localSnapshotId": "<explicit 64 lowercase hex local snapshot ID>"
}
```

Received snapshot IDs never select local state. The proposal must belong to the
current endpoint/principal/scope, and a changed receipt refuses. The receipt read
ends before the independent native read begins, so the same Store can serve both
roles without a nested transaction. These are two immutable observations, not an
atomic cross-Store snapshot or an attestation of remote ownership.

All advertised symbols must match the locally recomputed projection in their
original order: path, name, kind, comment-stripped header signature, producer,
full declaration fingerprint and exact UTF-8 byte bounds. Projection counters,
analyzer and replay scope must also match. Duplicates, permutations, missing rows
and changed bodies cannot be hidden by a smaller page. A zero-symbol packet
refuses as unavailable rather than becoming an empty complete source result.
Native source, graph, analyzer and capture/Git lineage replay remain mandatory.
`reportedDeclarationFingerprintMatched:true` means that the received fingerprint
matches one recomputed from local declaration bytes. The packet does not contain
the sender's raw declaration bytes, so this is not a direct remote byte comparison.

`sourceHash`, when supplied on a symbol, must additionally equal the locally
verified whole-file hash. It is optional in the existing handoff; its absence is
reported as `reportedWholeFileHashMatched:false`, never inferred. A private
declaration outside advertised ranges can therefore differ while the advertised
projection still matches. Such a result establishes neither whole-file nor
whole-source equivalence. Local file hashes are retained as local evidence.

A claimed verified Git commit/tree pair must match the independently checked
local receipt. `--require-git-identity` additionally requires this correspondence;
an ordinary disk capture cannot satisfy it. Matching Git IDs establish matching
local Git objects, not the sender's snapshot identity, current remote head,
repository ownership, authorship or observed execution. Historical commit arrays
and discussions are not independently verified by this operation.

The opaque `CheckedDigest` has no JSON constructor. Its bounded export retains
the exact received symbol metadata and original JSON pointer/ordinal, separate
receiver and local workspace/snapshot pins, local entity/range/hash evidence and
explicit header-projection granularity. It excludes implementation text and
keeps root `sourceAuthority:false`, `complete:false`, compiler/semantic-identity
flags false, and zero network/model calls and writes. Only the separately checked
local capture supplies the per-row source correspondence. Without an explicit
preview the sender's source-digest hash remains an unverified claim.

The `verify` command optionally accepts an absolute host-selected `--source-preview`
with independent `--allow-source-preview`, `--max-preview-bytes` and
`--max-preview-encoded-bytes` limits (each defaults to 4 MB). After permission,
scope and receipt verification it reads the file, strictly parses bounded JSON,
and hashes the bounded encoding of the parsed value, as the sender does before
adding discussions. A match sets `senderSourceDigestHashVerified` only; it does
not establish sender snapshot identity, remote ownership or source authority.
Duplicate fields, trailing data, null, malformed input and mismatches refuse.
Synchronous file reads and parsing are checked around the deadline but are not
interruptible wall-clock bounds. Interpret does not accept this option.

Frozen `find` performs a case-sensitive literal match over verified path/name/
kind/signature fields. It preserves original row IDs and can operate after both
Stores are gone. It does not run semantic reasoning or grant a source-byte reader,
write permission or model authority. An exported JSON report cannot reconstruct
the checked capability; a new verification must replay the sources again.

Host limits are at most 4 MB of packet bytes, 4,000 symbols and 16 MB of encoded
output including the CLI LF. The native replay retains its existing independent
file/source/metadata/AST/lineage limits. `--max-symbols` and `--max-source-bytes`
can lower work admission; the latter defaults to 32 MB. Omission, work/output
overflow, corruption, cancellation and deadlines refuse the complete operation.
These cooperative bounds are not an OS memory/CPU sandbox.

Independent verification of other packet claims, receiver-side semantic reasoning,
production App installation and full upstream equivalence remain open. This
source comparison does not silently replace those capabilities with literal search.
