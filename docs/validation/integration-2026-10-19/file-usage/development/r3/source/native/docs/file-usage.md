# Captured file usage checks

`jb.safe_delete` with an omitted or null `name_path` selects a captured file.
Omitting `delete_even_if_used`, or passing false, selects the bounded
`captured_es_module_functions_v1` policy. Direct native `safeDelete` accepts the
same file selection through an omitted/null `namePath`. An exactly empty symbol
selector still refuses. Explicit forced-file deletion retains its source-policy
path and its visible skipped-usage-check result. File propagation remains
unsupported.

The checked policy currently accepts `.ts` TypeScript or `.js` JavaScript ES
modules containing comments and named function declarations, with at least one
exported declaration and a function body for every selected declaration.
Variables, anonymous exports, imports/re-exports, ambient declarations,
top-level execution and other declaration or language shapes refuse. Every
syntax range and name must bind one exact captured structural entity.

Before provider startup, native code checks writable captured membership,
original file integrity and complete selected-input quotas. Existing graph
references, candidates and unresolved path/name matches prevent deletion.
Every other captured text file is also checked for case-insensitive substrings
matching the file path, basename, stem or selected declaration names. This is a
conservative refusal policy: documentation or an unrelated name can prevent a
plan. Source hashes for the entire checked manifest remain visible.

Each selected declaration then requires a definition response binding its exact
original file/hash/name range. A reference request includes the declaration:
the response must contain that exact declaration, have no truncated results and
contain only source-bound references inside the selected file. Missing
capabilities, empty responses, foreign usages or changed provider/source context
refuse before plan publication. The registered host profile, source exclusions
and existing passive/source-execution permissions apply to every request.

Admission is finite: at most 2,000 captured files, 16,000,000 source bytes counted
per file, 256,000 bytes in the selected module and eight selected declarations.
The largest request set is sixteen provider queries. Existing LSP frame/result,
source-copy and absolute deadline limits also apply. Capacity refuses the whole
operation.

After all checks, one ordinary guarded file-deletion plan is prepared. Source
files stay unchanged. Exact host approval, passing executed verification,
current source preconditions and conditional recovery retain their existing
requirements. Usage-check metadata accompanies the result; it does not change
the released file-plan identity or create a new durable authority record.

The result keeps `complete=false`, `exhaustiveCompilerProof=false` and
`remainingUsagesMayBreak=true`: dynamic, external, omitted and differently
configured consumers remain unknown. Captured and provider omissions stay
explicit. Provider replies are host-selected semantic observations; they do not
attest exhaustive runtime behavior or proprietary IDE equivalence.

Owned protocol tests and the explicit `file_usage_live` gate have separate
validation. The live gate selects actual installed TypeScript language-server
and compiler CLIs through `YOKE_TS_LSP` and `YOKE_TSC`; it has no fixture fallback.
