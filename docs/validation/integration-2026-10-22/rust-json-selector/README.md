# Passive Rust JSON project selector isolation

Code Intelligence 0.5.0, Windows. Source 2721b39dc198eab679b28f6b2bd5533e157e01a4.

Passive Rust now omits captured rust-project.json selectors from its private
provider copy at every directory depth, with case-insensitive filename matching.
Original bytes, capture, graph and source hashes remain unchanged; omission
receipts identify exact original paths/hashes. Ordinary similarly named files
remain available, and explicit host source-execution authority retains selectors.
New JSON-aware source-policy strings bind new provider evidence. Original policy
strings retain their original exact omission meaning for archived overlays;
old evidence is not rewritten or upgraded. Captures without JSON selectors retain
preceding policy strings and default results.

At source 2721b39d, 694 Rust functions pass across 18 scoped groups,
including the full library (569 passes). 10 existing ignores and zero failed
functions occur in this scope. One expanded provider-copy test and one new
versioned-overlay function cover actual file delivery, host authorization,
hash/order/set refusals and immutable prior results. All ten original symlink
fixture Git and frozen working bytes remain exact; their Windows prerequisite
acceptance remains open. No fixture skips, privileges or system settings change.
All-target Clippy -Dwarnings, native build, typecheck, npm 260 passed/3 existing
skips and 345 actual format roots pass on 1061 frozen inputs. Cargo fmt --all
retains OS206. This is not whole current Windows certification.

The causal test fails against unchanged published provider code because the actual
fixture sees both JSON selectors. Current code passes. Actual old/new CLI default
configuration/MCP53 and literal LSP data JSON remain exact in the same owned
fixture; whole SQL before/after is unchanged. Only Native requestId/duration are
excluded after independently repeating the old binary. The installed rust-analyzer
1.94.1 still resolves an exact cross-folder reference when an owned JSON selector
points outside captured source: that selector is omitted and hash-reported with no
source-execution grant or external dependencies. This is one controlled
runtime case, not a general runtime-effect or semantic-quality certificate.
See [passive JSON-selector evidence](README.md)
and [provider authority](../../../../native/docs/provider-authority.md).

Broader all-77 runtime/vendor settings/install/version acceptance, remaining
project-setting extensions, original353/R1-R9, authenticated Google, current whole
Windows, main/release and other listed status work remain open.


This fixes observed provider-copy admission, not a demonstrated arbitrary code
execution exploit. The [upstream JSON format](https://rust-analyzer.github.io/book/non_cargo_based_projects.html)
can identify source/sysroot paths and build metadata and is auto-discovered.
It is not interpreted as host authority. No upstream application code is copied.

RED retains the original test failure and the controller failure: the child
fixture originally throws, while the host sees only a closed LSP protocol; the
controller had incorrectly expected child stderr text. RED2 makes actual selector
presence observable through a fixture capability, so its assertion identifies
both JSON paths against production lsp.rs byte-identical to published 70d01389.
The Rust function remains failed (0/1); the causal controller records that expected
failure. R1 qualifies the omission correction but was superseded before publishing
because archived policy semantics also needed versioning. R2/final qualify the
final exact frozen source. Earlier results are not relabeled as current passes.

The actual default comparison uses the old published3fb image in its original
owned fixture, preserving default config/MCP53 and literal LSP data RAW. Every
other Native envelope field is compared after only requestId/metrics.durationMs
are removed, as independently justified by an original-binary repeat. Whole
current fixture SQL is identical before/after; no database is reset or edited.

The installed rust-analyzer binary SHA256 is 8780a0a408169100c7b4df49ba2c9dcc4b40b48dd375e3985bd585f1a4fe0a57.
The live fixture supplies one crate with no external dependencies and a captured
JSON selector pointing at an owned uncaptured canary. The selector's original
hash appears in omissions while the genuine exact cross-folder reference still
resolves. No source-execution authority is supplied. This tests this controlled
path; it does not certify transitive provider effects, all77 runtimes, source
execution safety under arbitrary host tools, or general semantic completeness.

No original ten symlink fixtures, protected failed linker output or protected
unrelated processes are changed. All completed owned Jobs reach zero. Six later
explanatory edits are recorded in post-validation-docs.json. Raw archive member
counts are packaging, not parity counts. Full current Windows remains open.
