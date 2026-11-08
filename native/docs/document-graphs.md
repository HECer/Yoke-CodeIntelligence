# Immutable Office document graphs

`document build` explicitly captures a DOCX or XLSX using the existing media path and ignore policy. It produces a separate `native_document_graph_v1` record. Canonical source snapshots, graph IDs, and captured binary containers remain unchanged. Parsing performs no formula calculation, model request, network access, or document execution.

The checked graph distinguishes three source spaces:

* **opaqueContainer:** original ZIP bytes, identified by SHA-256; these bytes are never represented as a UTF-8 source snapshot.
* **partXml:** exact decompressed XML part bytes with their own hashes and byte ranges.
* **decodedText:** UTF-8 paragraphs or cached cell values, joined by one explicit LF separator. Segment ranges use global offsets in this derived text artifact. Search operates within individual segments.

XML origins identify enclosing elements; they do not claim a character-by-character mapping through XML entity decoding. Shared-string cells retain both the worksheet cell reference and the shared-string value origin, plus the actual workbook sheet declaration. Formula metadata describes source formula text; displayed cell values come from the captured cache and can be stale.

`load` reconstructs all graph facts and text from the exact captured container before accepting registered artifacts. A caller cannot deserialize a checked graph. Search, read, traversal, and the offline explorer accept this checked type. A read selects exactly one source space: decoded text requires `segmentId` and forbids `part`; XML requires `part` and forbids `segmentId`. UTF-8 boundaries and range limits are checked.

The native CLI offers `document build`, `load`, `query` (`find`, `read`, `trace`), `explorer`, and `archive`. Query arguments are supplied with `--args`. Search supports literal Unicode matching, case selection, offset, and limit. Traversal supports incoming, outgoing, or both directions, bounded depth and visited-node capacity. The explorer downloads the document graph separately from canonical source graphs.

Archives contain `document-graph.json` and create-only hash-addressed objects for the original container, decoded text, and referenced XML parts. `document query-archive` and `explorer-archive` work without opening the original workspace. Archive loading reconstructs every fact from the archived original container and rejects changed objects, invented facts, unknown fields, and unsupported schemas. Archives establish captured-byte consistency, not author identity or execution authority. Existing human files are never overwritten.

Retention recognizes the exact typed record only after full graph and inspection reconstruction, then retains its verified container, inspection, graph, XML, and text artifacts. It does not accept arbitrary opaque artifacts through a kind allowlist.

Current limits include the media container limit of 50 MB, 32 MB graph metadata, 20,000 extracted segments, 25,000 nodes, 100,000 observations, and 2,020,000 derived text bytes including separators. Individual reads are limited to 256,000 bytes; query JSON output is limited to 4 MB. Office ZIP/XML admission imposes additional part, expansion, structure, and parsing limits. Exceeding a limit produces an explicit error rather than a complete-result claim.

This standalone document contract covers native DOCX paragraph text and XLSX cached cell values. It does not imply complete Office layout, drawing, comment, revision, formula-dependency, or mixed source/document View parity.

The explicit [mixed query capability](mixed-views.md) combines checked Office
graphs with one pinned native source View for shared literal search and typed
reads. It preserves original native UTF-8, XML and decoded-text coordinates.
