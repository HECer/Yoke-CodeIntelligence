# Checked mixed source/document queries

`mixed-view` selects one explicitly pinned native source View and one through
four checked DOCX/XLSX document graphs in the same workspace. It replays the
source View and its original overlay/Meaning receipts, reconstructs documents
from original containers, and returns an opaque `CheckedMixedView`. Serialized
Views, projections or document JSON cannot construct this capability. Queries
start no provider, capture, network request or project execution and write no
SQL/CAS record or overflow artifact. Original live files may be absent.

```powershell
yoke --root C:/Work/project mixed-view --selection C:/Host/mixed.json
yoke --root C:/Work/project mixed-view --selection C:/Host/mixed.json --query '{"operation":"find","query":"storage","ignoreCase":false,"offset":0,"limit":30}'
```

The strict host selection is bounded to 64,000 bytes:

```json
{
  "schemaVersion": 1,
  "source": {
    "workspaceId": "WORKSPACE_SHA256",
    "snapshotId": "SNAPSHOT_SHA256",
    "viewId": "VIEW_SHA256",
    "viewArtifactId": "VIEW_ARTIFACT_SHA256",
    "baseGraphHash": "BASE_GRAPH_SHA256"
  },
  "documents": [{
    "documentGraphId": "DOCUMENT_GRAPH_SHA256",
    "artifactId": "DOCUMENT_ARTIFACT_SHA256",
    "containerHash": "CONTAINER_SHA256",
    "decodedTextHash": "DECODED_TEXT_SHA256"
  }]
}
```

Replace placeholders with stored 64-character lowercase SHA-256 identities.
Source pins come from the explicit source View; document pins come from
`document build` results. Document graph IDs are unique and sorted for output.
Parent source and document identities keep their original recipes. Selection
does not infer semantic equivalence or a cross-source graph relationship.

Queries are strict JSON. Omitted `--query` is `{"operation":"describe"}`.
`find` searches primary captured source files in path order, followed by document
segments in selected graph-ID order. It matches literal Unicode text within
each individual source file or decoded document segment. Case matching defaults
to sensitive. One global offset and limit apply across all source spaces.
Offset is at most 100,000; limit is 1 through 2000 and defaults to 30. A response
reports the full bounded match count and explicit pagination/truncation. A
newline joining document segments is not searchable as a cross-segment match.
Historical source objects needed only for receipt replay are not added as
search inputs.

Every result includes `sourceProvenance`: the original View's completeness,
limitations and overlay descriptors, selected Meaning graph identities, original
packet input/contract hashes, source bindings, receipt provenance and coverage.
Meaning provider attestation remains unverified. Source-scope descriptors label
the one `primary_search` snapshot and each `receipt_replay_only` scope, with
original workspace/kind/capture consistency and file/byte counts. Receipt replay
does not add historical source files to the literal search space. All these
metadata fields participate in output admission; a large provenance envelope
may itself require a larger allowed output budget.

Each result's `source` is a typed read reference. Submit that reference unchanged
as `{"operation":"read","source":REFERENCE}`. Supported reference kinds are:

| Kind | Exact coordinate space and required fields |
| --- | --- |
| `sourceUtf8` | `viewId`, `snapshotId`, `path`, `contentHash`, `startByte`, `endByte`; original captured file UTF-8 bytes |
| `documentDecodedText` | `documentGraphId`, `segmentId`, `contentHash`, `startByte`, `endByte`; global decoded-text artifact bytes within the selected segment |
| `documentPartXml` | `documentGraphId`, `part`, `contentHash`, `startByte`, `endByte`; original decompressed UTF-8 XML part bytes |

Reads are at most 256,000 bytes and require exact UTF-8 boundaries. Binary
container offsets are refused. Document results retain container and asset
identities plus original enclosing XML origins. XML origins do not imply
character-by-character mapping through entity decoding. Shared-string origins
and cached formula values retain the standalone document contract; cached
values may be stale. Original CRLF bytes in native files remain unchanged.

`--max-documents` defaults to and cannot exceed 4. `--max-metadata-bytes` and
`--max-source-bytes` default to and cannot exceed 64,000,000 each. Metadata
includes document registration/asset/artifact-registration rows, document graph/inspection artifacts
and the native adapter's proof envelope. Source reservations include original
containers, decompressed XML, decoded text and all native source scopes needed
for replay. Document objects are deduplicated by hash and exact size; overlap
between document and native reservations is conservatively charged separately.
The initial native adapter replays the same pinned View on both sides, so its
two proof envelopes are conservatively charged. This avoids altering its
existing source authority and fixed per-side limits.

All selected document pins and registered metadata/source sizes are admitted
before bulk native source reads. A private observer can further deny a native
scope after the native source-policy checks; it grants no source authority.
Combined source reservations are enforced before bulk native reads. Original
document loading reconstructs all facts afterward. Metadata materialization
retains the document metadata and native adapter's separate hard bounds.
`--max-matches` defaults to and cannot exceed 100,000; exceeding it refuses the
whole query, including when the requested page is smaller. Results borrow
original text and XML origins during bounded output admission. CLI
`--max-bytes` is 1024 through 4,000,000 and includes the final LF.

An existing initialized workspace store is required. Global capture, snapshot,
analyzer, language-server and write configuration are refused. Deadlines and
host cancellation apply throughout; refusals produce no partial stdout or
saved result. `complete: false`, `modelCalls: 0` and `sourceWritten: false`
remain explicit. This schema1 capability covers native source and DOCX/XLSX.
Schema2 adds [frozen PDF pages](mixed-pdf-views.md) through an explicit native
worker capability. Mixed-space semantic extraction, cross-workspace joins and
general cross-source graph links remain open; these query contracts do not
establish full R4 or universal parity.
