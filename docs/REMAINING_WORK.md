# Remaining implementation and acceptance

This map reconciles the R1–R9 tracker with the current integration.
It distinguishes concrete missing behavior from acceptance that remains
unverified. The pinned inventory, its 353 identities, 73 unreconciled rows,
77 backend runtime unknowns and false full-contract certifications remain.
The detailed [tracker](parity/native-reconciliation-2026-10-15.json) retains the
original inventory references and historical checkpoint evidence.

| Area | Implemented contracts | Remaining work |
| --- | --- | --- |
| R1: semantic edits | Guarded native/provider planning, exact declaration ownership, directory/file usage checks, explicit TS/JS propagation and session routing. | Broader declaration, file and language shapes and provider intents, especially inline/move and compiler-bound consumer behavior; qualify each extension with its actual selected provider and independent apply/recovery. |
| R2: meaning and identity | Checked native graphs/joins, portable data projections, and the separate mixed-source preparation/build/query/composition contracts. | Explicit semantic identity/deduplication policy and broader interpretation/outcome behavior. Existing mixed capabilities preserve separate namespaces; aliases and host link assertions establish no identity. |
| R3: historical analytics | Explicit checked pair-of-Views comparison with complete parent pins, immutable source/receipt replay and directed observations. | Remaining upstream analytics equivalence and acceptance. A bare previousSnapshotId remains insufficient to select two Views; that refusal is an intentional precondition. |
| R4: mixed documents | Native/Office XML/decoded/PDF spaces, fixed input preparation, checked claims, explicit provider jobs, checked graph queries and independently checked cross-workspace/history composition. | Broader mixed analytics, external semantic-provider qualification and exhaustive interpretation remain open. CLI composition currently selects one explicit Store; library callers may supply separately checked parents from different workspaces. |
| R5: Git interpretation | Captured impact, areas, conflict stages, hierarchy candidates and finite CODEOWNERS interpretation with original byte evidence and visible unknowns. | Wider report/ownership semantics, compiler-bound override/dispatch identity, remote freshness and upstream schema equivalence. Captured labels grant no identity, review or merge authority. |
| R6: host lifecycle | Generic managed installation, guarded v1/v2 recovery, the separate Codex MCP profile and native session semantic planning. | Other branded client schemas and hook/statusline adapters, their actual client gates and remaining platform qualification. |
| R7: external sources | Selected PostgreSQL catalogs; received HTTP text/raw HTML/PDF; coordinated arXiv; reviewed native Neo4j/FalkorDB publication with pinned Windows live evidence. | Google shortcuts, authenticated/social/video routes, article/semantic interpretation and wider service/version/platform qualification. |
| R8: GitHub App | Signed queue/worker lifecycle, controlled publication/pages, explicit discussions, digest client/receiver, authorized metadata queries, persistent/App-bound history execution, independent received Rust declaration correspondence and source-bound interpretation of explicitly built local native Meaning graphs. | Broader receiver reasoning and model-quality qualification; independent verification of other packet claims (sender source-digest/CAS identity, discussions and history); configured live App installation/deployment and whole-source/remote equivalence. No background service is installed. Worker per-file Git-blob checks retain their existing finite scope. |
| R9: outcomes | Separate source-bound knowledge, historical saved-plan and packet-qualified native Meaning concept/relation/hyperedge outcomes with immutable authored events, exact side quotes, original source/receipt replay and transparent ranking. | Further object surfaces and explicit semantic transcript candidates; independent corroboration, model-quality qualification and observed execution evidence. Supplied labels do not establish semantic truth, current applicability or actual application/runtime truth. |

The [mixed graph query](../native/docs/mixed-meaning-queries.md) and
[composition](../native/docs/mixed-meaning-composition.md) contracts now cover
several items formerly described as missing under R2/R4. The separate native
same-snapshot join API retains its original schema and preconditions.
The [GitHub digest overview](native-digest-development.md) and
[worker](../native/docs/github-app-workers.md) and
[App history](../native/docs/app-history-integration.md) contracts distinguish
available bounded execution from configured deployment. Discussion capture and
service handoff do not need another implementation.
The separate [received-declaration verification](../native/docs/received-source-verification.md)
contract checks the advertised Rust projection against explicit independently
replayed local sources; it preserves the distinction between finite correspondence
and sender identity, whole-source equivalence or semantic reasoning.
The separate [receiver interpretation](../native/docs/received-source-interpretation.md)
contract now binds independently replayed local native Meaning inferences and
their complete submitted units to these declaration witnesses. Source quotes,
original receipts and semantic claims remain distinct from interpretation truth.

## Acceptance still required

Meaning claim outcomes pass all 13 new cases. Their broader five-target native
run records 74 passed / one failed / zero ignored. The unchanged Windows alias
fixture fails before application reads at symlink setup (OS1314); the host lacks
the required privilege/Developer Mode. The security fixture and host-file reader
remain exact. Format/build/all-target Clippy, Python17, typecheck and
TypeScript260/3skipped pass on fixed 770 native / 768 retained inputs. This failed
broader run remains an open Windows acceptance gate. See
[Meaning outcome evidence](validation/integration-2026-10-19/meaning-feedback/README.md).

The source-bound receiver interpretation checkpoint passes 114 Rust tests / one
ignored case across five groups, plus format/build/all-target Clippy, Python17,
typecheck and TypeScript260/3skipped. Its 764 native and 763 retained inputs remain
fixed. Owned protocol fixtures qualify source/receipt processing and explicit
cache rebound, not model quality, remote billing or production deployment. See
[interpretation evidence](validation/integration-2026-10-19/received-source-interpretation/README.md).

The independent received-declaration checkpoint passes 142 Rust tests / two
ignored cases across five groups, plus format/build/all-target Clippy, Python17,
typecheck and TypeScript260/3skipped. Its actual CLI and source-corruption tests
establish the finite local correspondence contract, with no configured production
or semantic-reasoning acceptance. See
[verification evidence](validation/integration-2026-10-19/received-source-verification/README.md).

The checked mixed-composition checkpoint passes 630 tests with seven ignored
cases across 20 groups, plus format/build/all-target Clippy. The App-history
checkpoint passes 300 Rust tests / six default ignored cases across 14 groups
(one target used an explicit test profile and separate output filename because
Windows retained a crashed linker lock), plus its actual Windows CLI/key gate.
See [App-history evidence](validation/integration-2026-10-19/app-history-integration/README.md).
Fresh Python17, TypeScript typecheck and TypeScript260/3skipped checks pass.
These scopes overlap earlier runs and do not establish a whole-native or hosted
platform pass. [Mixed-composition evidence](validation/integration-2026-10-19/mixed-meaning-composition/index.json)
preserves source variants and the initial test-fixture compilation failure.

A fresh complete native run and current-head hosted acceptance remain open.
Historical Windows OS1314 symlink setup and hosted failures retain their original
logs; no fixture or permission gate is weakened to turn them into a pass.
External service qualification retains its exact selected runtime, version,
credentials and permission boundaries. Configured production deployment and
full upstream contract certification require their own actual evidence.

PR #7 remains a draft against main. Main integration, release packaging and the
separate `HECer/Code-Intelligence` publisher require real remote verification
before release status changes. The integration branch is not a release receipt.
