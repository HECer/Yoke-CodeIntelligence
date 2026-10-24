# Native explicit media capture

`native/src/media.rs` provides explicit inspection of local assets. General native source capture still reports binary files as omissions. Media inspection does not add those files to the code graph, silently derive source text or execute project code.

## APIs

`inspect(&Engine, relative_path, &Deadline)` captures and validates PNG, JPEG, PCM/IEEE-float WAV and selected ISO BMFF containers (`.mp4`, `.mov`, `.m4a`). It returns a workspace-bound `assetId`, exact SHA-256 `sourceHash`/`blobId`, immutable JSON `artifactId`, technical `metadata` and `contentInterpretation: "not_performed"`.

The source path uses the native protected-path, portability and link/reparse policy. Discovery respects the same local ignore walker settings as source indexing, including nested `.gitignore`, `.ignore` and Git exclude rules. Linked ignore files and external/linked Git policy directories fail explicitly. There is no URL input, network request, ffprobe execution or metadata-based OCR/ASR substitution.

Assets must be regular files at most 50 MB. Capture performs two complete byte reads with metadata and deadline checks, verifies equality and preserves the exact bytes in the native content-addressed store. This is a stable double read rather than an operating-system atomic snapshot against a hostile concurrent writer. Obvious credential patterns are refused before persistence. The explicit asset ledger is limited to 1,000 entries and 1,000 MB of declared captured asset sizes. Raw assets and generated JSON are explicitly registered as local artifacts; `ci_read` can page their exact bytes while unpublished or foreign-only blobs remain outside generic artifact access. Persistence refreshes SQLite lock waits to the remaining absolute request budget before transaction admission and commit.

Technical parsing is bounded and does not decode pixels or audiovisual streams:

| Format | Imported metadata and checks |
|---|---|
| PNG | Dimensions, bit depth, color type and interlacing; chunk structure, header constraints and CRC checks. Compressed pixel data is not decoded. |
| JPEG | Frame dimensions, precision, components and progressive header marker; structural marker/segment and end checks. Entropy-coded image data is not decoded. |
| WAV | Encoding, channels, rate, bit depth, frame count and duration; exact RIFF sizes, PCM/float sample alignment and byte rates. Other encodings fail explicitly. |
| MP4 family | Declared major brand, movie time scale and duration; bounded ISO BMFF box sizes and nesting. Codec, scene and speech interpretation is unavailable. Fragmented streams without an available movie-duration header fail explicitly. |
| PDF without worker | Signature/end-marker metadata with `nativeTextAvailable: false`. This is not complete PDF validation or text extraction. |

PNG and WAV containers allow at most 10,000 chunks; MP4 allows 10,000 boxes with nesting depth at most eight. A structurally accepted container is not proof of a successfully decodable stream.

## Native PDF worker

`inspect_with_worker(&Engine, relative_path, absolute_worker_executable, &Deadline)` uses ordinary native inspection for non-PDF assets. PDF parsing runs through the explicitly selected trusted native Yoke executable, with a fresh single-file copy, no project working directory, stripped environment, no stdin, bounded stdout/stderr and `ManagedChild` group/job cleanup.

The hidden native command `media-pdf-worker --input PATH` dispatches `pdf_worker(&Path)` before Engine opens a store. This API installs process-wide limits and must only run in a dedicated worker process. It is unsuitable for a reusable application thread. On Windows a nested job enforces and queries process/job memory limits of 256 MiB, one active process and 20 seconds of CPU time. Linux/macOS implementations set and query address-space and CPU limits; they were not executed on the Windows development host. Unsupported platforms or refused resource limits fail explicitly. Restrictive host limits or high initial address-space use may make the worker unavailable; limits are never silently relaxed.

The worker uses pinned MIT [lopdf 0.42.0](https://docs.rs/lopdf/0.42.0/lopdf/) for strict document and content decoding, with an independently written bounded text visitor. It validates the full page tree, references, node types, parent links, duplicate children, cycles and declared descendant counts before emitting text. Referenced font/resources and stream failures propagate. The library's convenience extraction and silent encoding/decompression fallbacks are not used.

Supported direct page operators include `BT`/`ET`, `Tf`, `Tj`, `TJ`, `T*`, the single/double quote operators and ordinary text positioning/state operations. Named Standard, WinAnsi, MacRoman and MacExpert byte encodings and bounded `ToUnicode` mappings are supported. Unicode code spaces determine source-code widths; overlapping/prefix-ambiguous ranges, out-of-range or absent mappings, malformed surrogate sequences and replacement mappings fail explicitly. Default StandardEncoding is allowed only for the built-in Latin Type1 families. Unknown fonts, custom encoding dictionaries, Type3 glyph programs, unsupported content operators and font state from external graphics dictionaries fail explicitly.

The artifact labels `extractionScope: "direct_page_content_streams"` and `exhaustive: false`. Text follows content-stream order with explicit line breaks; large negative `TJ` displacements insert heuristic spaces. Spatial reading order, column layout, accessibility `ActualText`, annotations and appearance streams are not reconstructed. Invoked Form XObjects are not traversed and produce explicit page warnings; image XObjects likewise report that image interpretation/OCR was not performed. These limits apply even when a page also has successfully extracted direct text.

Bounds are 10 MB input, 2,000 pages, 8,000 page-tree nodes, inheritance/tree depth 128, 512 content streams and 1,024 font resources per page, 8 MB per expanded stream/combined page content, 100,000 operations per page/CMap and 1,000,000 page operations total, 256 Unicode code-space ranges and 4,096 mapping ranges per font, 200,000 UTF-8 bytes per page, 2,000,000 total text bytes, 8 MB stdout and 64 KB stderr. Encrypted documents require an explicitly decrypted copy. Expanded compressed streams remain subject to worker memory/CPU limits and the parent's shared deadline.

Native PDF text is labeled `native_pdf_text_not_ocr`. Empty pages remain explicit and report that OCR/image interpretation was not performed. The caller checks the copied input hash, response source hash, schema, page sequence, producer/version, text bounds and declared installed resource limits before storing an artifact. No external OCR, neural model or actual speech recognition is included. A copied input and a constrained process are not a general operating-system sandbox.

Actual Windows tests use the built native binary to extract WinAnsi `Grüße`, non-BMP `ToUnicode` text and mixed-width character codes, test `TJ`/line/quote operations, reject malformed/unknown fonts and encodings, validate nested page trees and reject missing children/count mismatches/cycles, report excluded Form text and enforce the page text cap. They verify the worker response and confirm hidden dispatch does not create a `.yoke` index. A real SQLite writer-contention test verifies that prior asset capture does not extend persistence's deadline. These are native text extraction and storage tests, without simulated OCR/ASR results.

## Supplied transcripts

`ingest_transcript(&Engine, relative_path, input, &Deadline)` accepts strict JSON:

```json
{
  "expectedHash": "<exact asset SHA-256>",
  "producer": "human-editor",
  "text": "Supplied transcript text.",
  "language": "en"
}
```

`language` is optional. The caller must name the exact asset digest and a bounded producer attribution. The asset is captured through the same path/ignore/secret policy; a stale digest, empty transcript, obvious credential, unsupported control character or unknown authority field fails. Text is limited to 2,000,000 UTF-8 bytes.

The immutable transcript artifact records workspace, asset path/hash, raw blob, technical metadata, producer and `origin: "host_supplied_transcript_not_verified_against_asset"`. Producer names are attribution labels, not authentication. Transcript assertions remain `authored_data_not_execution_authority`. Ingestion performs zero model calls and writes no original source or asset bytes. Artifacts can be read through native `ci_read` artifact cursors; binary blobs require the host's explicit binary read encoding.
