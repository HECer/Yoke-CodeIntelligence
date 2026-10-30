# Semantic provider enrichment

The Rust client implements capability-negotiated read/plan operations,
plus guarded semantic rename, against explicitly selected language servers.
The independent public-contract inventory pins Serena revision
`4f2520efb128dd02e66a2302095bdbbb26ddd5a0`. No GPL application implementation was
copied. The 77 catalog entries describe selectable backends and alternative
providers; they are not 77 certified language runtimes.

`HostRegistration::configured` accepts an absolute selected stdio executable or
an existing numeric loopback TCP endpoint. Registration requires explicit host
trust and does not install or start a provider. GDScript uses an existing TCP
endpoint. A TCP request owns its connection only and does not shut down the
externally managed language server. Repository data and MCP query options cannot
choose executables, argv, initialization settings, or trust.

The operations are `definition`, `declaration`, `references`, `implementation`,
`hover`, `symbols`, `typeDefinition`, `incomingCalls`, `outgoingCalls`,
`supertypes`, `subtypes`, `completion`, `signatureHelp`, `diagnostics`,
`symbolInfo`, `prepareRename`, `codeActions`, `willRenameFiles`, and `capabilities`. Unsupported advertised
capabilities fail explicitly. Both symbol operations normalize checked file
URIs, byte ranges, selection containment, snapshot IDs, content hashes, kinds,
name paths, and optional bounded bodies. Hierarchy traversal preserves opaque
provider data across rounds, directed edges, and repeated call occurrences;
visited items bound cycles. Diagnostics use full pull reports when supported or
actual version-checked push publications otherwise. Windows drive-letter URI
spelling is normalized through checked captured-file identity.

All requests share the caller's deadline. Frames are limited to 4 MB, diagnostic
retention to 8 MB and 200 notifications, configuration callbacks to 64 items and
4 MB before cloning settings, hierarchy traversal to depth 8 and bounded item
and serialized-result counts. Completion edits validate source ranges, nested
InsertReplaceEdit ranges, and string replacement text. Server commands are
refused. Explicit WorkspaceEdit text changes and file create/rename/delete
operations prepare ordinary immutable guarded change plans; preparation never
applies them to the original workspace. Literal code actions can negotiate
`codeAction/resolve` for their deferred `edit`, preserving all original opaque
attributes under a cumulative 4 MB result budget. Source-validated full pull
diagnostics or the measured TypeScript publication pathway supply action context.
Disabled actions, unrequested kinds and standalone commands are not resolved.
Combined edit/command actions expose their resolved edit availability but remain
unusable and prepare no partial plan. Directory resource changes, providers
without action resolution, general executeCommand, and proprietary IDE refactorings remain
explicit unsupported boundaries.

Every result includes a `serverProfileHash` derived from the declared command,
arguments, language, initialization settings, workspace folders, language
mappings, transport endpoint, and selected command bytes when resolvable.
`serverProfileBinding` describes what was bound. This identifies launch context;
it does not attest transitive scripts/plugins, existing TCP peer binaries,
runtime execution, or provider correctness. Exact executable hashing is included
in the request deadline; unoptimized debug SHA-256 introduces measurable overhead.

Call and type hierarchy results can be persisted as separate semantic overlays.
Nodes, evidence, and observations include provider profile identity and exact
source bindings. Duplicate call ranges retain distinct observation identities.
The published baseline graph is immutable. CAS bytes may remain unregistered
after a failed write; artifact and overlay records become discoverable together
in one SQLite transaction. Existing records must compare equal, and the deadline
is checked before commit. Load validates artifact content identity, baseline
graph identity, snapshot, and source manifest. Overlay results remain incomplete
provider claims rather than exhaustive compiler or runtime proof.

The first-wave focused Windows gates passed 33 tests: 10 legacy LSP, 19 enrichment, two
overlay, and two catalog tests. Targeted failing tests reproduced each repaired
review defect before implementation. Independent read-only review found no
remaining Important defect in this first-wave scope. Actual installed Windows
Jedi 0.47, rust-analyzer 1.94, and TypeScript language server 5.1.3 gates passed
three tests in 74.55 seconds after review repairs. These validate selected
operations, not every provider method or every catalog backend. A real Godot
runtime gate, remaining provider installations/certification, and Linux/macOS
runtime certification remain separate measurable work.

The later resolver gates passed five independent contract tests plus one actual
installed TypeScript 5.1.3 test on Windows. The real server supplied two published
diagnostics and a deferred fix-all edit. Its auxiliary command remained refused;
this validates actual resolution and the explicit command boundary, not complete
execution of that provider action. See the
[LSP 3.17 code-action contract](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#codeAction_resolve).
