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
| R8: GitHub App | Signed queue/worker lifecycle, controlled publication/pages, explicit discussions, digest client/receiver, authorized metadata queries, persistent local history jobs and App-bound bounded worker/history execution with signed original plans and exact revision recovery. | Receiver-side independent source verification and reasoning; configured live App installation/deployment. No background service is installed. Worker per-file Git-blob checks retain their existing finite scope. |
| R9: outcomes | Source-bound knowledge outcomes and separate historical saved-plan outcomes with immutable authored events, exact side quotes and transparent ranking. | Additional typed targets and explicit semantic transcript candidates; independent corroboration and observed execution evidence. Supplied labels do not establish actual application or runtime truth. |

The [mixed graph query](../native/docs/mixed-meaning-queries.md) and
[composition](../native/docs/mixed-meaning-composition.md) contracts now cover
several items formerly described as missing under R2/R4. The separate native
same-snapshot join API retains its original schema and preconditions.
The [GitHub digest overview](native-digest-development.md) and
[worker](../native/docs/github-app-workers.md) and
[App history](../native/docs/app-history-integration.md) contracts distinguish
available bounded execution from configured deployment. Discussion capture and
service handoff do not need another implementation.

## Acceptance still required

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
