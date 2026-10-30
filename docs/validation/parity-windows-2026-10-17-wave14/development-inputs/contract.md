# Processed PDF participation in mixed queries

This is a next-scope design note, not an implementation or acceptance result.
The checked source/Office candidate remains sealed. No competing builds run
while the Meaning-join whole acceptance is active.

## Source authority and reuse

Use existing explicitly captured `native_media_asset_v1` registrations and their
immutable container CAS. No additional persistent PDF graph is necessary for
literal mixed queries. Existing retention already recognizes media assets.
Never trust a supplied or resealed inspection's decoded text as source proof.

Refactor the existing `media::inspect_with_worker` child runner into a shared
Store-free byte-input operation. Both live explicit inspection and frozen-asset
replay must use the same existing strict parser, copied read-only input, managed
process, bounded pipes, deadline/cancellation and Windows job/CPU/memory limits.
Do not parse hostile PDF containers in the parent process or duplicate the
parser. Preserve current media behavior and tests.

Query replay reads only registered frozen bytes. It never reopens the asset's
original path, captures source, starts a provider or executes project content.
Execution of the own native PDF helper is explicit host authority: a typed
library worker capability or a dedicated CLI flag. JSON never chooses an
executable, arguments, environment or network capability.

## Compatibility and coordinates

Keep the existing schema1 source/Office selection behavior unchanged. Introduce
a separately validated schema2 selection with the same native View pin, zero
through four Office pins and one through four explicit PDF asset pins. At
least one document is required; ordinary schema1 input must not silently gain
new fields or worker authority. A PDF pin binds asset ID, original container
hash and the exact supported extraction policy/version.

PDF read references identify asset ID, container hash, one-based page number,
decoded page-text hash and exact UTF-8 byte range within that page's text.
These are derived page coordinates, never original binary-container offsets,
Office XML coordinates or canonical native source ranges. Return page hashes,
original asset path/size/hash, extraction policy, warnings and resource-limit
observations. Preserve original native View/overlay/Meaning receipt and scope
provenance. Direct content-stream order is not rendered reading order, OCR or
exhaustive PDF interpretation; the existing worker limitations remain visible.

## Admission and behavior

Pre-admit all strict asset rows and CAS registrations, workspace/source pins
and combined source reservations before bulk native reads. Reserve the bounded
worst-case decoded text as well as each PDF container before extraction.
Actual decoded text may consume less; conservative reservations are reported
separately. Recheck all actual worker output against its frozen input and
limits. Share one deadline/cancellation token through the entire mixed load.

Reuse globally paged literal Unicode find and typed describe/read. Retain
original text bytes, page identity, deterministic ordering and whole-operation
match/output refusal. Queries publish no SQL/CAS or overflow artifact. Failure
after spawning must terminate/reap the owned worker and leave source/Store
unchanged. Native historical receipt-only scopes remain replay-only.

## Required evidence before implementation/acceptance

1. Exact isolated source baseline after the sealed R9 candidate; preserve its
   full map. Parent integration still requires the actual accepted Wave13.
2. Behavioral CLI RED for absent schema2/explicit PDF flag using the unchanged
   prior executable, followed by unchanged GREEN cases.
3. Actual two-page and non-BMP PDF fixtures, existing DOCX/XLSX and captured
   native source in one mixed selection. Verify global paging and exact typed
   reads against independent original page/container bytes.
4. Delete/change live files after capture and prove frozen reads remain exact.
   Corrupt registered asset/header/container/worker output and prove refusal.
5. Zero/exact source, metadata, decoded-text, match and final-LF output bounds;
   malformed/encrypted/oversized PDF; unsupported page/font mappings; expired
   and inherited cancellation; caller SQL transaction/limit preservation.
6. Prove absent CLI worker permission fails before Store access or spawning;
   a selection cannot grant executable authority. Validate host path/link and
   protected source policies without changing existing Office/native rules.
7. Existing media/Office/document/mixed/comparison/retention regressions,
   TypeScript/typecheck/fmt/all-target Clippy, independent read-only review and
   fresh whole Windows acceptance on one exact integrated source map.

Mixed semantic extraction and cross-source graph links require a later typed
source-space contract. They are not supplied by this literal-query design.
