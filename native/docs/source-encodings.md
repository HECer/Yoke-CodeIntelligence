# Selected source encodings (qualification in progress)

This is a bounded native codec increment, not certification of the complete
encoding identity, all standard codecs, all language servers or the project.
Earlier source `272ec171` has source-bound macOS results for all 19 core and
eight transport cases in [run 41031519894](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41031519894),
tested merge `79872a66` with an identical Git tree. Those results include ASCII
and UTF-8-SIG and retain that source identity; they do not qualify Windows-1252.
The Windows-1252 increment is integrated afterward, with 21 core and ten
transport declarations. Native compilation and execution of the new increment
remain unrun pending guarded hosted qualification. The parent owns serial
native/CLI/stdio/HTTP/compatibility and full functional acceptance.
Shared release status, the original 353-entry inventory and certification flags
are intentionally outside this change.

The pinned external configuration reference is
[`project.template.yml`, lines 44–47](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/project.template.yml#L44-L47),
contract `serena:configuration:projectKeys:2`, pointer
`/upstreams/serena/configuration/projectKeys/2`, EP027.
The entry SHA-256 is
`02071f6af6a897c92c98c81ebd39a67f9d67cbf0f51ea346a773f967de8cb08b`;
the immutable inventory SHA-256 is
`993eb15143bf61a1cf64c80ff53d491ee13f7faeecf2ff4b53a6975de9972c3e`.
Only the configuration contract and official encoding documentation informed this
independent MIT implementation. No GPL application implementation is incorporated.

## Selection and aliases

Selected host YAML accepts `encoding` in global, project and project-local
documents. Local overrides project, which overrides global; null clears that
layer and inherits. Resolution is independent of document input order. Context
and mode documents cannot set it. Project names must already be host-registered.
Configuration files remain UTF-8; this setting describes analyzed project text.

Alias lookup is finite, ASCII case-insensitive, and accepts hyphens in place of
underscores. These are the admitted spellings (plus those case/separator forms):

| Codec | Canonical and documented aliases |
| --- | --- |
| UTF-8 | `utf_8`, `utf8`, `u8`, `utf`, `cp65001` |
| Strict ASCII | `ascii`, `us_ascii`, `646` |
| UTF-8 with optional signature | `utf_8_sig` |
| BOM-selected UTF-16 | `utf_16`, `utf16`, `u16` |
| Explicit little endian | `utf_16_le`, `utf-16le` |
| Explicit big endian | `utf_16_be`, `utf-16be` |
| Windows-1252 | `cp1252`, `windows-1252` |
| True ISO-8859-1 | `latin_1`, `latin1`, `iso-8859-1`, `iso8859-1`, `8859`, `cp819`, `latin`, `L1` |

The [Python 3.11 standard encoding table](https://docs.python.org/3.11/library/codecs.html#standard-encodings)
is the alias reference, not a promise to implement its entire codec registry or
additional registry normalization. Whitespace aliases, arbitrary punctuation,
UTF-32 and every other codec are unsupported.
Unknown codecs and invalid setting types refuse explicitly. Latin1 maps each
byte directly to the same numbered Unicode scalar, including C1 controls;
byte `80` is U+0080, not the Windows-1252 euro sign. Explicit ASCII rejects
every byte at or above `80` and rejects edits containing non-ASCII characters;
it never replaces or transliterates them. ASCII is a selected policy, not
automatic detection.
For an existing UTF-8-SIG file without a signature, an edit that would place
U+FEFF at the start is refused: its bytes would otherwise be consumed as a
signature when the file is read again. Existing signatures are preserved.

The resolved host profile uses `sourceEncodings` with `global` and
`projects.<registered-name>.project/local`, with canonical enum values `utf8`,
`utf8_sig`, `utf16`, `utf16_le`, `utf16_be`, `latin1`, `ascii`, `windows1252`. Example:

```json
{"sourceEncodings":{"global":"latin1","projects":{"default":{"local":"utf16"},"secondary":{"project":"utf8"}}}}
```

Host capture configuration can independently select the versioned fallback:

```json
{"sourceEncoding":{"kind":"strict_v1","codec":"utf16"}}
```

The existing native `--capture-config` paths accept this strict schema, including
native indexing, `read`, `invoke`, `serve` and HTTP host capture options. Session
host selection overrides the capture fallback per project. Secondary engines
start from the original host capture options rather than the primary selection.
Explicit `sourceEncodings` can select UTF-8 over a non-UTF8 capture fallback.

Absent, null and standalone standard UTF-8 YAML settings retain the preceding
exported profile bytes. UTF-8 declarations are retained during multi-layer
resolution when a non-UTF8 selection exists, so a local UTF-8 override remains
effective even if the global document is processed later. An empty or UTF-8
capture policy canonicalizes to the legacy absence. Existing UTF-8 capture IDs,
cache keys, graph rows, plan IDs, result fields and historical source bytes keep
their original recipe. New explicit host selections bind Session fingerprints;
changing that selection refuses reuse of an old persisted Session.

## Bytes, BOM and coordinates

Original bytes, raw sizes and SHA-256 hashes remain in the immutable source CAS.
No existing capture is rewritten or reinterpreted using the current host codec.
Replay recovers the codec from the original, checked capture-options proof, also
inherited by native derived snapshots.

`source_encoding::View` separately decodes a bounded UTF-8 analysis view and a
checked monotonic scalar-boundary map. It never applies Unicode normalization,
replacement characters or newline conversion. For the new policies, decoding
is a strict two-pass operation: sizes are checked before allocating text/maps.
Odd UTF-16 input and unpaired high/low surrogates refuse. Windows-1252 follows the
[Python 3.11 strict cp1252 codec](https://github.com/python/cpython/blob/3.11/Lib/encodings/cp1252.py):
bytes `81`, `8D`, `8F`, `90` and `9D` are undefined and refuse; they are not mapped
to C1 controls. Unrepresentable characters refuse on write.

`utf8_sig` consumes an optional initial `EF BB BF` signature and keeps it as an immutable raw prefix; without one the decoded text starts at raw byte zero. New files use a signature; existing files preserve its presence. Invalid UTF-8 refuses. Legacy UTF-8 continues to expose U+FEFF as text.

`utf16` requires an initial `FF FE` or `FE FF` BOM. That BOM selects byte order
and is consumed from analysis text; it remains an immutable raw prefix. This
portable native policy deliberately refuses an unmarked stream rather than
choosing a machine-dependent fallback. It is narrower than every behavior a
Python decoder or Unicode interchange protocol might allow. Explicit LE/BE
codecs never infer byte order or strip a signature: an initial matching U+FEFF
is text, and opposite-order bytes are decoded according to the explicit order.
UTF-8 retains its preceding U+FEFF behavior. The distinction follows the
[Python Unicode codec discussion](https://docs.python.org/3.11/library/codecs.html#encodings-and-unicode)
and [Unicode UTF/BOM FAQ](https://www.unicode.org/faq/utf_bom.html).

Public `startByte`/`endByte`, entity/name spans, diagnostic starts, lexical
scopes/bindings/uses and graph evidence ranges are original byte coordinates.
Tree-sitter parses the analysis view, then all of those coordinate families and
their entity keys are translated together. Directed edges and parallel
observations remain separate. Selected codec cache keys cannot collide with
legacy UTF-8 cache entries. Signatures/names describe the decoded view; graph
coverage states this, and evidence uses
`exact_decoded_view_original_byte_boundaries_v1` rather than the legacy anchor.

Reads, symbol bodies, declaration headers, FTS/document snippets, literal/regex
search and bulk occurrence edits use checked mappings. Selected-codec source
responses include `sourceView`: raw and analysis hashes, codec policy, byte
spaces, sizes and consumed prefix length. Quoted text is UTF-8; its associated
range/hash identifies original bytes, not the UTF-8 representation of the quote.
A full-file read may include the raw BOM prefix in its requested range; edits
cannot consume that prefix or start inside a code unit, surrogate pair or UTF-8
scalar. CR, LF and CRLF remain exact unless a separate line-ending policy is set.

Protected paths and decoded NUL/obvious-secret classification run before source
publication. Decode failures have explicit omission reasons and retain partial
coverage; an omitted file is never an empty complete source result. Paths,
captured authority, ignored-source/read-only rules and current write scope keep
their independent admission checks.

## Reviewed changes and optional consumers

Replacement inputs are Unicode text with **original raw byte ranges**. Native
explicit and symbolic edits translate boundaries before composing replacements,
then strictly encode the result into the original codec before after-hashes,
review diffs, derived graphs, approval or verification. Existing BOM-selected
files retain exact byte order/prefix; new BOM-selected files use a deterministic
LE BOM. Explicit LE/BE and Latin1 creates emit no implicit BOM. Bytes outside
approved ranges round trip exactly; a separately selected line-ending convention
authorizes its existing whole-file formatting stage before encoding/review.
Unrepresentable Latin1 text refuses, including euro signs, non-BMP characters
and combining sequences not individually in Latin1.

Selected-codec review uses a deterministic linear full-file unified diff with
before-allocation output admission and cooperative deadline checks. The legacy
UTF-8 diff recipe is unchanged. Plan identity additionally binds the codec;
replay checks both capture proofs, actual raw after-bytes and decoded review.
Approval/current workspace application rejects a changed codec selection.
Workspace writers, exact verification trees and recovery journals keep original
encoded bytes; they never write the private UTF-8 compiler representation.

LSP read/navigation operations use create-only private UTF-8 source copies,
UTF-16 protocol positions derived from checked decoded boundaries, and raw
coordinates in returned anchors. Copy integrity compares decoded bytes while
source hashes remain raw. Provider profile fingerprints include the selected
codec/private-view recipe. Direct and staged entrypoints share this policy.
No provider is selected, trusted or started merely by selecting a codec.
Facade symbol bodies use the same checked view, retain raw body boundaries and
hashes, and carry selected-codec view evidence. Legacy UTF-8 output is unchanged.

The following consumers currently refuse selected non-UTF8 proof before their
optional execution, or at their non-executing proof entrypoint:

- LSP workspace edits/resource operations (`rename`, code-action planning,
  `willRenameFiles`) and native semantic refactoring/raw-piece compiler moves
  or inlining.
- The independent TypeScript worker protocol, Python runtime/debugger trace
  mapping, V8 coverage/test-map interpretation, dataflow analysis and persisted
  semantic-overlay proofs.
- Manifest-balanced retrieval, which currently owns a separate UTF-8 source
  capability. Ordinary core context/read/search paths are supported.
- UTF-8 editor overlay buffers and committed-source Git workflow acquisition.
- Optional model source selection for summaries, embeddings and reranking;
  source admission refuses before provider execution, including Latin1 bytes
  that are independently valid UTF-8 with a different interpretation.
- Knowledge proof import/local vectors, Meaning source selection/archive text,
  GitHub source declarations/bindings, Git impact/owner text and Trail definition
  bodies retain explicit UTF-8-only admission. Raw-byte readers remain available
  for byte-preserving capture, plans and journals.
- Other legacy consumers of `query::source[_in_scope]` receive an explicit
  `source_encoding_capability_unavailable` error rather than a decoded string
  whose offsets they could mistake for raw positions. Advanced archive, model,
  federation, exported-source/UI and provider combinations need separate
  qualification; this candidate does not claim they are supported.

These capability errors do not disable core capture/index/read/change support.
JetBrains facade method names remain adapters, not an independent backend.
Dashboard selected-document editing retains its separate UTF-8 document
contract; selecting a project source codec does not grant document authority.

## Bounds and qualification

Per-file ceilings are 4,000,000 raw bytes, 4,000,000 decoded UTF-8 bytes and
32,000,008 mapping bytes (two u32 coordinates per admitted scalar boundary).
Raw capture admission precedes its fixed bounded buffer; text and mapping
admission precede their allocations. New encoded extraction releases each map
before loading the next source. Document publication admits a conservative
512,000,000-byte decoded total before allocation. Source caches additionally
admit retained mappings; bulk source maps have a separate 128,000,000-byte
ceiling. Private LSP copy preparation also checks the existing 16,000,000-byte
semantic mapping-cache ceiling per source before any provider starts; larger
sources retain core capture/read/change support but refuse that optional route.
Existing source/output/file-count guards remain in force. Cancellation
and inherited deadlines cover decoding, mapping, encoding, hashing, diff output,
extraction and source/copy reads. Optional operations retain their original host
timeouts and memory/process cleanup requirements.

Authored acceptance sources:

- `native/tests/source_encodings.rs`: real capture/index/read/headers/search,
  parallel observations, immutable replay, native create/replace/bulk plans,
  strict type/unknown-field/path/secret/BOM/surrogate/CRLF/budget/stale refusal,
  layer ordering, per-project separation and legacy identities.
- `native/src/source_encoding_lsp_tests.rs`: private view integrity and LSP
  normalization without provider execution.
- `native/tests/source_encodings_transport.rs`: actual CLI configuration and
  YAML export/Session, native stdio, authenticated HTTP, a labeled real stdio
  LSP protocol fixture, and actual executed raw-byte verification followed by
  approval/workspace journal checks. These are deferred fixtures, not executed
  evidence and not certification of a real compiler/backend.

The author performed source/rustfmt checks only. After review corrections, the
parent's exact `ceb23aa9` candidate passes guarded Windows workspace/all-target
Clippy with warnings denied and unchanged selected inputs. Its 759 Cargo
artifact observations are metadata; no native image was admitted or executed.
Actual codec/transport/runtime fixtures remain unrun. The independent PR8
activation HostProfile/config/CLI fields must survive combined integration.

The 24 new test functions are authored and typechecked, not executed. Two parent review regressions
cover UTF-8-valid Latin1/UTF-16 symbol bodies, raw anchors, unchanged legacy
output, and model refusal before provider execution. The static check used
Rust 1.94.1's rustfmt 1.8.0, edition 2024, with `skip_children=true` over all 29
changed Rust files. The two previously CRLF source files (`changes.rs` and
`changes_plan.rs`) retain Windows newlines. The whitespace check uses
`git -c core.whitespace=cr-at-eol diff --check` for that existing convention.
A separate bounded .NET calculation over an owned literal confirms UTF-16 BOM
source size 50 versus UTF-8 view size 27, and name range 24..36 versus decoded
14..20. Its Latin1 byte-128 calculation yields U+0080 / UTF-8 `C2 80`. These
calculations do not execute the implementation or establish functional acceptance.

## Implementation file manifest

New files are this document, `native/src/source_encoding.rs`,
`native/src/source_encoding_lsp_tests.rs`, `native/tests/source_encodings.rs`
and `native/tests/source_encodings_transport.rs`. Existing integration files:

```text
native/src/capture.rs
native/src/agent_repl.rs
native/src/changes.rs
native/src/changes_plan.rs
native/src/compiler_inline.rs
native/src/compiler_move.rs
native/src/dataflow.rs
native/src/debug_adapter.rs
native/src/editor.rs
native/src/file_api.rs
native/src/github.rs
native/src/github_app_digest_source.rs
native/src/git_impact_details.rs
native/src/git_owners.rs
native/src/git_workflows.rs
native/src/graph_view.rs
native/src/graph_view_archive.rs
native/src/graph_view_comparison_input.rs
native/src/knowledge.rs
native/src/lib.rs
native/src/lsp.rs
native/src/main.rs
native/src/meaning_build.rs
native/src/project_session.rs
native/src/projects.rs
native/src/providers.rs
native/src/query.rs
native/src/query_features.rs
native/src/retrieval_scopes.rs
native/src/runtime.rs
native/src/semantic_enrichment.rs
native/src/semantic_refactor.rs
native/src/semantic_propagated_delete.rs
native/src/session_config.rs
native/src/session_config_tests.rs
native/src/store.rs
native/src/store_read_only.rs
native/src/test_map.rs
native/src/trail_adapter.rs
```

## Windows-1252 increment

The Windows-1252 implementation from source-only candidate `4995fe20` is
integrated after ASCII and UTF-8-SIG at `272ec171`, retaining both codec variants,
their aliases, byte mappings, capability entries and original fixtures.
The original undefined-codec assertions now exclude only the newly supported
Windows-1252 names. Native execution of this combined increment remains pending
guarded qualification; earlier codec results retain their own source identity.

The added core fixture pins the independent UTF-8 hash of all 251 defined byte
values and checks every scalar boundary in both directions. Expansion/cancellation
cases refuse before publication. The new stdio fixture checks decoded reads,
raw-byte create plans, request-codec override refusal and unrepresentable edits
without writing source. A separate actual verification/approval/writer fixture
requires a failed wrong-byte verification, refusal to apply it, a successful
exact-byte verification, a checked journal and retention of original CAS bytes.
These fixtures are authored for the unchanged guarded CI; they have not run
locally. No selected-codec compiler/runtime proof or semantic edit capability is
enabled by adding this codec.
