# Frozen PDF pages in mixed queries

This schema2 capability extends the schema1 source/Office query contract. It is
under development; a scoped check or this document does not establish complete
mixed semantic extraction, cross-source links or universal parity.

Explicit `media inspect notes.pdf` already captures a registered immutable media
asset. Save its `assetId` and `sourceHash`. A schema2 selection uses the ordinary
five-field native View pin, zero or more Office document pins and at least one
PDF pin. At most four documents, including PDFs, may be selected together.

```json
{
  "schemaVersion": 2,
  "source": {
    "workspaceId": "<workspace hash>",
    "snapshotId": "<snapshot hash>",
    "viewId": "<View hash>",
    "viewArtifactId": "<artifact hash>",
    "baseGraphHash": "<base graph hash>"
  },
  "documents": [],
  "pdfs": [{
    "assetId": "<registered media asset hash>",
    "containerHash": "<original PDF sourceHash>",
    "extractionPolicy": "yoke-native-pdf-text-1:lopdf-0.42.0:direct_page_content_streams"
  }]
}
```

```sh
yoke --root /project mixed-view --native-pdf-text --selection /host/selection.json \
  --query '{"operation":"find","query":"storage","ignoreCase":true,"limit":30}'
```

`--native-pdf-text` explicitly authorizes the current native executable's bounded
PDF helper for this query. A selection cannot supply an executable, arguments or
environment. Missing permission refuses before opening the project Store. The
library takes `SelectionV2` and a separately constructed `NativePdfWorker` in
`load_with_pdf`; that caller explicitly chooses and trusts its native helper.
The executable path is host authority, without binary attestation. Schema1
remains separate and rejects PDF references and this additional flag.

Library hosts may attach a deny-only `with_spawn_observer` callback. It observes
the owned process ID after cleanup ownership is installed and can deny or share
cancellation. It cannot bypass source/protocol/resource checks and is never
selected by JSON. Ordinary inspection and the CLI attach no callback.

Every query loads the registered frozen container bytes, validates their exact
size/hash and re-extracts them through the existing native worker. It never
trusts a saved inspection's decoded text or opens the live asset path. The worker
runs on a read-only temporary copy with the existing bounded pipes, managed
process, shared deadline/cancellation, 256 MiB memory cap and 20-second CPU cap.
The original 10 MB input, 2,000-page, 200 KB per-page, 2 MB total decoded-text and
8 MB worker-output limits remain. No capture, provider, project execution,
SQL/CAS publication or overflow artifact is introduced by mixed queries.

`describe`, literal Unicode `find` and typed `read` retain the original native
View/overlay/Meaning receipts, packet coverage and replay-only source roles.
Search order is native paths, Office graph/segment order, then PDF asset ID and
one-based page number. Pagination is global; match/output overflow refuses the
whole operation. PDF headers retain the original path, container hash/size,
decoded page hashes/lengths, exact extraction policy, warnings and observed
worker resource limits.

PDF matches return a `pdfPageText` source reference:

```json
{
  "kind": "pdfPageText",
  "assetId": "<registered asset hash>",
  "containerHash": "<original PDF hash>",
  "page": 1,
  "contentHash": "<decoded page UTF-8 hash>",
  "startByte": 0,
  "endByte": 12
}
```

Use that exact reference with `{"operation":"read","source":...}`. Offsets are
within the decoded page's original UTF-8 bytes, including any parser-produced
line breaks. They are never binary PDF offsets, XML offsets or native source
coordinates. Page number, all hashes, membership, UTF-8 boundaries and read size
are checked again. Empty direct text remains empty with the original warning;
no OCR text is fabricated.

All asset/registration rows and combined budgets are admitted before bulk native
source reads. Each PDF conservatively reserves its complete container plus
2 MB decoded text in the source budget and 8 MB worker output in the metadata
budget, in addition to its actual registration rows. Headers expose these
reservations, which may exceed actual text/metadata. Original source/Office
reservations and independent replay bounds still apply. Caller read transactions
and SQLite limits are preserved. Existing registered media assets remain
retention roots; no new persisted mixed-view record is created.

The parser covers direct page content streams and supported font mappings.
Content-stream order and heuristic text spacing are not rendered reading order,
layout, OCR or exhaustive PDF interpretation. Form/image exclusions and original
warnings remain visible. Literal matches do not establish semantic identity,
execution truth or cross-source graph relationships.
