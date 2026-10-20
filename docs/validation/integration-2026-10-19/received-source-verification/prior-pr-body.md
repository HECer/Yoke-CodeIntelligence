Combines native Code Intelligence 0.5 checkpoints `c5178b2` and `7b8eabb`
with guarded GitHub App workflows and subsequent source-bound native extensions.
The retained TypeScript runtime remains 0.3. This is an active draft integration;
full STATUS/R1–R9 acceptance, `main` integration and release remain open.

### Latest App-bound history execution

An explicitly permitted `github-app worker run` can now complete/replay its worker,
bind that durable completion to a checked Git head and drive bounded persistent
history batches. A domain-separated MAC protects the original cumulative plan,
including against consistent edits to both stored plans. Current App/repository
grants, selected queue revision and the host-supplied native revision are required.
Recovery never adopts a revision returned by creation/status. An invalid initial
revision now refuses inside the binding/job creation transaction, before either
history row is published; the prior worker completion remains durable.

Worker completion, binding/job creation and successive batches remain separate
durable phases. Later refusal does not erase earlier success. Historical replay
does not repeat HTTP. Missing sidecars cannot adopt generic jobs; missing native
jobs are not silently recreated. Key rotation and current grant denial are checked.
The separate `app-history-job` CLI supports creation, exact-revision steps/run,
pause/resume/cancel and existing-only read-only status without an Engine or Git.
Typed retention validates real worker/job/Git relationships without importing
authority from arbitrary strings. Defaults start no background job, model,
analyzed project code or publication. Receiver verification/reasoning and configured
production App installation remain separate work.

Source `8fa410f380f384cb736babd7a3814758ec71be56`; runtime evidence `02eeaa00c5266bc072812d060a8ee307a5d071f7`.
[Contract](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/native/docs/app-history-integration.md) ·
[Indexed source/runtime evidence](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/validation/integration-2026-10-19/app-history-integration/index.json)
— SHA-256 `93f85a880a8f85f71f58890725ec4a28589a8f9a5789074efd56d2db16280ced`.

### Other integrated behavior

- **Git/GitHub:** captured impact/areas/ownership candidates, verified local history,
  closed-PR discussions, unmerged-index conflict pages, approved digest submission,
  authenticated intake and receipt-qualified literal queries. Unknown-send recovery
  retains separate endpoint/source/principal permissions.
- **Guarded changes:** exact saved-plan replay, captured moves, function/file usage
  checks, bounded deletion propagation and concrete Python/JS/TS declaration cases.
  Verification, approval, application and conditional recovery remain distinct.
- **Sources/graphs:** selected PostgreSQL catalogs, received HTTP/arXiv bodies,
  reviewed Neo4j/FalkorDB publication and source-bound authored outcome events.
  Directed parallel observations, immutable revisions and unknown-write recovery stay.
- **Mixed graphs:** checked typed native/Office/PDF preparation, provider jobs,
  source/response replay, frozen graph queries and independently checked composition
  across workspace/history parents. Parent-qualified identities, indexed multiedges,
  ordered repeated hyperedge roles and complete original source/receipt/accounting
  remain. Authored links are unverified, traversed only by opt-in; labels create no
  semantic identity and hyperedge roles imply no flow. No default MCP schema change.
- **Host/runtime:** receipt-owned Codex MCP configuration, portable projections,
  checked registry coordination, query/root repairs and process/deadline cleanup.
  Optional provider qualifications retain their own indexed scope and runtimes.

Repository content remains data. Source bytes and immutable snapshots are
authoritative. Default retrieval starts no model or analyzed project code;
missing/unsupported capabilities do not become empty complete results.

### Validation and platform correction

| Scope | Actual result |
| --- | --- |
| Current 13 ordinary Cargo targets | 287 passed / 0 failed / 6 default ignored |
| Same-source history-job target, explicit test profile and separate output suffix | 13 passed / 0 failed |
| Combined current native scope | 300 passed / 0 failed / 6 default ignored, 14 groups; 758 inputs fixed |
| Current format/build/all-target Clippy `-D warnings` | Passed on those same native inputs |
| Actual Windows CLI/Credential Manager gate | Passed; lost stdout/status/pause/resume/run, source and binary hashes unchanged; owned key removed and absence verified |
| Mandatory retained checks | Typecheck; TypeScript260 / 3 skipped; Python17 pass; 759 runtime inputs fixed |
| Prior checked mixed-composition checkpoint | 630 passed / 0 failed / 7 ignored, 20 scoped groups; exact older source in its index |
| Prior installer platform correction | 159 Windows Rust passed / 1 existing worker ignored; all 41 original test bodies preserved |

Windows retained crashed MSVC linker PID22940 and its original history-job output
lock; termination was denied. That one unchanged target was built with
`cargo rustc --profile test` plus `-C extra-filename=-r8-acquisition`, then executed
through its normal native harness. This is an explicit output-location exception,
not a standard whole-native Cargo pass. The initial linker/launcher failures,
fixture repairs and the deliberately failing initial-revision regression are
retained. All final changed native bytes are archived; older hash-only variants
are explicitly identified. Scoped and historical counts overlap and are not added.

At d819d71, all 36 Ubuntu failures reached the intentionally Windows-only guarded
installer. Success/recovery fixtures are now scoped to Windows; Unix API/CLI tests
require explicit refusal without mutation/private output, and two permission
checks remain cross-platform. Runtime installer functions/guards are unchanged;
Unix installation is still unavailable. No current-head hosted acceptance is
claimed here; T3 owns monitoring rather than a local polling loop.

- [Current STATUS and remaining-work map](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/REMAINING_WORK.md)
- [App-history raw checks and limitations](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/validation/integration-2026-10-19/app-history-integration/README.md)
- [Installer platform evidence](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/validation/integration-2026-10-19/host-install-platform/index.json)
- [Prior mixed-composition evidence](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/validation/integration-2026-10-19/mixed-meaning-composition/index.json)
- [All integration evidence and historical failures](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/validation/integration-2026-10-19/README.md)
- [Archived preceding PR description](https://github.com/HECer/Yoke-CodeIntelligence/blob/5d806c2b1b176206c7e2535379f5d264fdfc4176/docs/validation/integration-2026-10-19/app-history-integration/receipts/prior-pr.json)

### Remaining boundaries

Only R8 development metadata changed in this slice. All 353 inventory identities,
73 unreconciled rows, 77 backend runtime unknowns and false full-contract
certifications remain. Wider provider intents, client/language/file shapes,
semantic identity/reasoning, authenticated/media routes, dynamic/external
consumers and upstream/IDE equivalence remain open. Historical OS1314 fixtures
and hosted failures stay indexed. Production App deployment, full platform
acceptance, `main` integration and the separate release publisher are not
certified by these scoped checks.
