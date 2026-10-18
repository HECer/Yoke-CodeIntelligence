# Structural languages and captured host scope

The native grammar registry is `languages::grammar`; `analysis::grammar` remains a compatibility wrapper. Registry membership describes an available syntax parser. It does not certify a compiler, runtime, language server, or exhaustive reference graph.

The additional Windows grammar gates exercise PowerShell, Julia, Objective-C, Fortran, Erlang, Common Lisp, Pascal, VB.NET, Verilog/SystemVerilog, HCL/Terraform and Apex. Their dependency versions are pinned in Cargo. The maintained MIT `tree-sitter-sfapex` grammar supplies Apex; the older Apex crate depends on an incompatible tree-sitter API. All supplied fixtures are parsed without executing source.

Source-bound candidate extraction additionally recognizes literal R S4 registrations, R6/S7 constructor assignments, named R6 methods, S7 method assignments, and ordinary S3-style function names. Registration syntax remains a structural candidate, not evidence that a package has loaded or a class has been instantiated.

COBOL program/section declarations, explicitly named DreamMaker procedures, and Robot keyword/test/task headings have a separate bounded structural pattern path. Those declarations use producer `yoke-structural-patterns-1`; they are not represented as compiler bindings. The initial pattern gates do not establish comprehensive COBOL paragraphs, DreamMaker dispatch, or Robot library resolution.

Vue and Svelte script segments, Astro frontmatter/scripts, EJS code segments, Blade PHP blocks, and Razor code/functions blocks use native grammars over independent syntax units. Entities, parent keys, graph observations, scopes, bindings, and diagnostics are mapped back to the original immutable source bytes. Separate script blocks cannot repair each other's invalid syntax or create a symbol spanning intervening markup. HTML comments, EJS comment/escaped tags, and non-executable script data do not become code. Quoted tag attributes retain their delimiters; unsupported embedded languages produce an explicit warning rather than being parsed as another language. The current segment gates do not establish exhaustive template-expression, generated-renderer, cross-block EJS control flow, or framework routing semantics.

Unrecognized extensions can use an unambiguous language from the host language-server catalog. Existing explicit extension mappings take priority. Ambiguous catalog extensions remain text unless the host supplies a language override. An Objective-C `.m` file therefore needs an explicit override when its intended language is MATLAB. Catalog-only languages retain full-text/source access and require an explicitly trusted semantic provider for compiler-oriented operations; catalog membership does not add an AST grammar.

## Host capture options

`CaptureOptions` is typed host configuration, never repository or MCP authority:

```json
{
  "onlyDirs": ["src"],
  "includeDirs": ["build"],
  "languageOverrides": {"custom": "rust", "m": "matlab"},
  "readOnlyDependencyDirs": ["node_modules", "target"]
}
```

`onlyDirs` contains canonical relative directory prefixes and compares directory segments. `includeDirs` contains bare non-dot directory names and lifts only scanner skips for `build`, `dist`, and `vendor`; it does not widen `onlyDirs` or override Git ignore rules. `readOnlyDependencyDirs` accepts only `target` and `node_modules`. It permits capturing those components while preserving every other protected state, credential, link, content and size exclusion. Git-ignored dependencies remain absent; the walker does not enumerate ignored paths or invent individual omission records for them. An empty dependency option is omitted from serialization to preserve existing capture identities.

`source_scope_policy` verifies immutable native capture or derived-origin provenance and indexes exact manifest membership without caching source bytes. Its `path_policy` and `file` methods permit dependency reads only within that proven scope. Persisted ordinary foreign sources receive no dependency exception. All virtual or dependency sources carry an additional read-only restriction; this is never source-write authorization. `captured_read_path_policy` supplies prospective language/scope metadata for guarded immutable-source producers, without establishing file membership, filesystem access or blob authority. Isolated language-server materialization uses the same captured proof and rejects destination links/reparse points. Global source-write path protection remains intact.

`read_source` rejects linked, non-regular or incorrectly sized CAS leaves before allocating a source buffer. It checks the opened handle, bounds reads to the captured size, and verifies the exact source hash with shared deadline checks. A corrupt oversized CAS leaf cannot turn a small captured file into a large source allocation.

Editor overlays prove their exact persisted `overlay-snapshot` receipt and native disk origin. Their IDs reconstruct the original analyzer/session/revision/version/source tuple, including released analyzer-four, development analyzer-six, and analyzer-seven overlays. Every unversioned file must equal its base member, new members require an explicit buffer version, and protected dependency files cannot become edited buffers. New buffers retain host language overrides. Historical reads do not depend on a still-open mutable editor session.

Language overrides map validated extension names to known structural or catalog languages. They do not select executables, execute source, change filesystem permissions, or authorize edits. Unknown fields/languages and unsafe directory names fail before capture.

Non-default options participate in immutable snapshot identity. The create-only `native_capture_options_v1` sidecar contains `snapshotId`, `workspaceId`, `analyzer`, `optionsHash`, and the normalized `options`. Validating this sidecar reconstructs the entire disk snapshot identity before trusting an override. Default capture preserves the existing identity shape.

Guarded workflows can propagate options only after the origin and derived snapshots are persisted exactly. `inherit_options` writes a create-only `native_capture_scope_derived_v1` record with `snapshotId`, `workspaceId`, `originSnapshotId`, `originSnapshotHash`, `derivedSnapshotHash`, and `optionsHash`. Every derived file must satisfy the original scope and language mapping. Reads verify both manifest hashes, workspace identity, and an origin chain bounded to 16 snapshots. Chain traversal and manifest validation check the shared store deadline. This record carries source scope, not permission to write source.

`options_for_snapshot` recovers proven options, `language_for_snapshot` validates a new source path against that scope, and `capture_for_snapshot` performs freshness capture with the same options. A configured or derived snapshot with missing/tampered provenance cannot silently validate an overridden language.

Analyzer migration preserves authenticated released analyzer-four, development analyzer-six, and analyzer-seven language metadata when the active analyzer changes. A historical disk manifest must reconstruct its original capture ID and match the exact persisted manifest; configured scopes additionally prove the original options, and derived scopes retain the bounded origin chain. Protected paths, directory membership, and explicit host language overrides remain strict. Unknown or federated identities do not establish this historical proof. Current captures and new paths use the active classifier; archived source is never silently relabeled.

## Checkpoint evidence

The first language/scope checkpoint passed 47 focused tests: six capture-option tests, 21 scoped-resolution tests, eight existing operation regressions, and 12 language/composite tests. A 20,000-keyword Robot fixture failed its original shared deadline, then passed after section lookup and source-line lookup were indexed rather than repeatedly scanning prefixes. This is a bounded regression fixture, not an upstream performance comparison.

Further class/method/module, structural import/call, and template-expression gates are being expanded. Windows is the current runtime validation target; Linux and macOS runtime certification is deferred. Full upstream functional parity remains an ongoing task.

Procedure scopes cover PowerShell function/method nodes, Erlang clauses, Common Lisp defuns and Fortran procedures. Nested local declarations remain within those syntax scopes. VB invocation and `AddressOf` occurrences carry distinct call/reference relations; local `Dim` declarations prevent false binding to a same-named module method. Objective-C message candidates retain the exact message span and receiver, so unknown receiver types remain unresolved. Lisp function designators and Erlang `fun name/arity` produce explicit function-reference candidates; quoted Lisp data and literal vectors produce neither executable calls nor function references. These are structural candidates: macro expansion, dynamic binding, exhaustive identifier references, language-specific overload/arity dispatch and full receiver type inference remain open.

The current native analyzer epoch is `yoke-tree-sitter-8`. Exact authenticated analyzer-four, analyzer-six and analyzer-seven disk/configured/overlay identities remain readable without rewriting their captured languages or archived bytes. Unknown epochs receive no native source-scope grant.

Offline archive scope receipts retain a typed chain of original native capture, derived-origin and editor-overlay manifests and records. Validation reconstructs known analyzer identities and all option/manifest bindings without opening the original workspace, checks the original protected-path and dependency scope rules, and bounds the chain to 16 with a 64 MB total input/proof serialization budget and shared deadline. Legacy archives without a receipt qualify only through an exact known default disk capture identity; configured, dependency or virtual authority cannot be inferred from missing metadata. A changed protected path fails even when an archive is resealed. This proof describes immutable source scope and grants no filesystem, execution or write authority.
