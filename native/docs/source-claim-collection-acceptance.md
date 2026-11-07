# Source-claim collection archive acceptance

`native/tests/graphify_source_claim_archives.rs` adds public `Registry` acceptance
fixtures for an ordinary source-claim View. They use real indexed Unicode/CRLF
source with two distinct parallel calls, the explicit host importer and the
ordinary View builder. They do not manufacture accepted View fields.

The positive fixture registers the View, closes and removes both the entire
original project (including its Store) and the host input directory, then
requires an identical collection join and exact archived source readback.

The negative fixture first checks that its typed registration mirror reproduces
the original registration identity. It then reseals the outer registration while
changing the original claim payload bytes, removing its input, or duplicating
it. Each read must refuse without rewriting the corrupt registration. Restoring
the original registration must restore the exact successful join. A separate
case corrupts captured source bytes at their retained hash and requires refusal
without repair, followed by successful replay after explicit fixture restoration.

These are authored tests, not executed native evidence. Rustfmt parsing does not
establish Rust typing or runtime correctness. Fresh hosted native execution of
the complete target is required. The fixtures do not qualify every archive,
transport, cancellation or resource failure path, nor Graphify node-link file
interoperability, whole Windows/backend parity, main or release acceptance.
No production admission, source authority, resource or integrity guard changes.
