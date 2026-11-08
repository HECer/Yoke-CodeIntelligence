# Checked semantic refactoring intents

The Rust implementation uses host-registered semantic providers and independently
checks their explicit `WorkspaceEdit` results. It does not execute provider
commands, infer authority from action titles, or implement a proprietary IDE.
Session edit permission is checked before a provider is queried.

The optional [immutable TypeScript compiler path](compiler-inline.md) additionally
supports actual TS/JS function-binding inline actions through host-selected session
registration. It checks compiler-bound consumers, copied initializer structure,
comma/declaration removal and `keepDefinition` before guarded preparation. The
existing standard-LSP and owned `own_v1` protocols retain their contracts.

Direct native `safeDelete` has an explicit
[`propagationPolicy` v2](propagated-delete.md#explicit-wrapped-call-policy) for
complete standalone calls wrapped in parentheses, `await` or `void`. Default v1
and facade arguments retain their existing contract. All provider/source checks
and independent verification, approval, application and recovery gates remain.

## Unpublished admission

An internal checked batch binds the workspace, immutable snapshot manifest and
each virtual document input hash. It retains the ordered text/resource operations
and the final file contents separately. Text operations include byte coordinates
relative to each exact virtual input, an original-source flag and document
versions; resource operations record whether they
actually took effect. An ignored resource operation cannot prove a requested move
or deletion.

Native projections correlate the original and projected normalized-operation
digests with the snapshot, workspace, manifest and selected server profile.
These are audit correlations, not independently replayable provider attestations
or a substitute for checking the original typed operations.

Code-action candidates are checked before any candidate is handed to guarded
change preparation. Native nondefault intents select one unambiguous candidate
and validate their source and option postconditions before preparing one plan.
The ordinary public LSP result shape and guarded apply/approval mechanism remain
the existing interfaces. Derived source uses the proven base analyzer epoch,
including historical analyzer 8 and explicit analyzer 9 snapshots.

This staging boundary does not make existing storage publication transactional
across multiple plans. `changes::prepare` publishes a derived snapshot/graph and
then its source policy and plan record. A database, I/O, analysis or deadline
failure during publication can leave previously published objects. Atomic batch
publication needs a separate storage contract; wrapping this API in an outer
transaction would conflict with its existing transactions.

## File move facade

The pinned `jb.move` contract uses `target_relative_path` as the new parent
directory when moving a file. For example, moving `src/work.rs` to `pkg.rs`
prepares `pkg.rs/work.rs`; the basename is preserved even when the parent has a
file-like extension. Omitted, null and exactly empty `name_path` select the
captured file. Nonempty symbol selectors use the separate host-selected contract
below. Directory tree moves remain unsupported.

This bounded facade requires an explicit canonical existing parent directory.
Missing/null targets, root shorthand, occupied final paths, aliases of the source,
protected paths, reparse points and excluded/read-only write scopes are refused
before querying the provider. The directory metadata is only a host write
restriction; no directory source anchor or uncaptured membership is invented.
The registered `willRenameFiles` request and its checked pre-rename edits still
produce one guarded plan. The direct native semantic `move` operation continues
to accept the final destination filename, preserving its existing contract.

## Host-selected symbol move contract

`refactorIntentProfile: "own_v1"` is an optional host registration setting. An
absent setting preserves the previous serialized configuration and session
fingerprint. Changing only this setting invalidates an existing session binding
before its record is changed or a provider starts. Repository contents and tool
arguments cannot enable the setting. The registered provider must also advertise
the versioned `experimental.yokeRefactorIntent` capability during initialization.
This is an independently owned protocol, not a proprietary IDE protocol.

For a nonempty `jb.move` symbol selector, `target_relative_path` names an existing
captured writable Rust file. An absent or null `target_parent_name_path` selects
its `fileTopLevel` scope; a nonempty selector must identify an actual captured
inline Rust module and selects `parentSymbol`. An empty parent selector is
invalid. File selection retains the separate parent-directory semantics above;
it cannot select a symbol-parent destination.

The checked intent binds the exact snapshot, complete graph, source and target
hashes, canonical ancestry and source-order overload identity. Supported source
declarations are free Rust functions, structs, enums, traits, type aliases and
constants directly within a file or inline module. Each graph entity must match
the exact Rust AST kind, declaration interval and name interval. Generic fields,
variants, trait members, alias targets and constant initializers are transferred
as unchanged source bytes; this does not prove that the new namespace compiles.
Attached outer attributes and documentation use the separately negotiated text
bundle contract below. Associated items, local declarations and module moves
remain unsupported source selections.
Destination insertion means the end of the admitted scope, after its complete
items, rather than byte zero. Valid BOMs, shebangs, inner documentation and
attributes, and outer documentation or attributes on existing complete items
remain unchanged. Malformed or dangling syntax, impl/trait destinations and
moving a declaration into its own descendant are refused.

The private `yoke/refactorIntent` request correlates its canonical intent and
immutable base with one strict versioned response containing one explicit
`WorkspaceEdit`, or an explicit unsupported result. Command-only responses,
unknown envelopes, mismatched request identities and unsupported capabilities
cannot authorize preparation. Successful preparation additionally requires an
isolated original-coordinate deletion of exactly the selected declaration and
insertion of its unchanged bytes at the admitted scope boundary. Copy-only,
body-changing, whole-file, resource and subsequent virtual-document operations
cannot establish that effect. The same declaration-kind check also rejects extra
AST copies hidden behind added comments while retaining unrelated original
identical declarations. Declaration text inside comments or string literals is
not a declaration copy. A checked effect owns its unpublished batch and
is consumed by one guarded preparation; negotiation alone does not prove a move.

Other provider reference edits still need captured scope, hash and operation
checks. They do not establish exhaustive compiler binding or reference repair.
Results retain `complete=false` and `commandsExecuted=false`; existing approval,
freshness, analyzer 8/9 replay and publication limitations continue to apply.

### Attached declaration text

A provider may additionally advertise
`experimental.yokeRefactorIntent.sourceAttachments: "exact_text_v1"`. Without
this explicit capability, an attached declaration is refused after negotiation
and before intent dispatch. Existing capability envelopes without this optional
field remain valid for unattached declarations. An unknown attachment version is
refused; repository text and facade arguments cannot supply the capability.

For an attached source, the intent additionally includes
`sourceTextRange: {startByte, endByte}`. The ordinary `source` fields retain the
original graph entity's exact declaration and name intervals. The separate text
range begins at its first attached outer attribute or documentation comment and
ends at the declaration's end. Intervening ordinary comments, whitespace and line
endings belong to this exact byte bundle. Leading ordinary comments, BOMs,
shebangs, inner documentation and inner attributes remain in their original
container. No broader range is inferred from a provider response.

Effect admission requires one original-coordinate deletion of this complete
bundle and one unchanged insertion at the selected destination. It independently
checks the inserted declaration's original AST kind and name interval, each
transferred attachment's exact AST interval and its ownership by that declaration.
Leaving source attachments behind, stripping them from the insertion, changing
their owner or adding another declaration copy refuses before plan publication.
Other original preambles and attachments retain the existing preservation checks.
Whole-bundle transfer also preserves nested original syntax inside a moved
declaration; it does not verify attribute expansion or namespace/compiler effects.

## Nullable file rename facade

The pinned `jb.rename` signature and docstring select a file or directory when
`name_path` is omitted or null. This bounded implementation supports captured
writable files. An exactly empty selector is refused; the move facade's explicit
empty-selector convention is not inferred for rename. Directory tree renames
remain unavailable.

`new_name` must be one portable canonical basename, including any desired
extension. The source's parent is retained: renaming `src/old.rs` to
`renamed.rs` prepares `src/renamed.rs`. A root-level source remains at the
workspace root. Names are not trimmed and extensions are not appended. Protected
names, path components, source aliases and occupied destinations are refused.
Captured membership and write scope, existing parent metadata and scoped host
destination metadata are checked before provider work, including destinations
omitted from capture. Only an observed missing destination is admitted.

The public adapter supplies false defaults for `rename_in_comments` and
`rename_in_text_occurrences`. Absent and explicitly false values are accepted as
requesting no extra native occurrence pass. Either true value is explicitly
unsupported for file rename and is refused before querying the provider; null
and nonboolean option values are invalid. The standard `willRenameFiles`
protocol has no occurrence flags. Its independently checked provider edits may
include language-specific references in strings or other syntax; this adapter
does not independently certify provider compliance with syntax-class exclusions
or exhaustive reference discovery.

File rename uses the existing registered `willRenameFiles` contract and one
guarded resource-change plan, preserving the original workspace. Its source
snapshot is pinned through the facade's preflight and native preparation. Derived
source retains its proven analyzer 8 or 9 epoch. Existing approval, verification,
stale-source and apply/recovery rules still govern workspace writes. The direct
native semantic operation and its final-destination contract are unchanged.

## Explicit forced file deletion

The pinned `jb.safe_delete` contract selects a file or directory when
`name_path` is omitted or null. The supported file slice requires an explicit
`delete_even_if_used=true` and an absent or false `propagate`. It prepares one
whole-file deletion using the exact captured path, content hash and snapshot.
Source membership, captured write scope and an existing regular nonlink host file
are required. Directory trees, protected, read-only, explicitly captured ignored
and uncaptured sources are refused. An empty selector does not select a file.

This explicit force neither checks nor rewrites usages. The result states
`usageCheck.performed=false`, `referencesRewritten=false` and
`remainingUsagesMayBreak=true`, with a separate warning. Other captured source,
including callers and imports, remains unchanged. These fields and warnings are
outside the immutable plan record; the existing plan identity and host approval
rules are preserved. No semantic provider or command is started, and no symbol
offset is invented for a file.

Default/false `delete_even_if_used` is explicitly unsupported for files until a
file usage-proof contract exists. It is not treated as permission to force a
deletion. `propagate=true` is separately unsupported: the pinned contract means
deleting usages and cleaning symbols made unused, which this slice does not
perform. Null/nonboolean flags are invalid. A null `willDeleteFiles` response
would not prove a file unused; no such inference is made.

Preparation preserves the original workspace and proven analyzer 8 or 9 epoch.
Materialization and workspace apply use the existing guarded change API. Actual
workspace writes still require host permission, exact approval, executed
verification and fresh source preconditions; explicit force grants none of
those authorities. These tests include isolated materialization and real apply
preflight/write behavior with explicitly supplied host prerequisite records;
those records do not assert that a compiler was run.

## Native source projections

Explicit comment/text rename flags use a source-bound provider rename and a
separate bounded Rust syntax pass. The native supplement covers literal Unicode
identifier occurrences in captured writable Rust comment nodes and unescaped
plain/raw string content. Literal delimiters and raw/byte/C prefixes remain
unchanged. Byte and raw-byte occurrences require an ASCII replacement name;
applicable Unicode replacements are refused before preparation. Disabled
occurrence classes also constrain the provider's original edits. The projection
reports the checked source hashes, exclusions and
occurrence count. Prose occurrences do not assert compiler symbol binding.
Read-only dependencies and explicitly captured ignored sources remain excluded.
Other languages, syntax errors, uncertain interpolation, resource operations and
edits using subsequent virtual document coordinates require a different explicit
contract. Comment markers within strings remain string contents. Identifier
suffixes and Unicode combining marks are part of the occurrence boundary.

`keep_definition=true` is an independently checked projection of one explicit
inline action. It may omit an isolated deletion of exactly the selected
declaration and preserve its exact bytes while keeping separately checked call
replacements. A broad whole-file replacement cannot establish that isolated
operation. Ambiguous candidates, command-bearing actions and unverifiable
replacement intent are refused before plan preparation.
The selected call candidates retain their exact captured evidence ranges and
hashes; structural target resolution is not an exhaustive compiler binding proof.

These bounded projections do not imply universal compiler semantics, exhaustive
reference discovery, or compatibility with every proprietary refactoring option.
Directory moves, deletion propagation and default-safe file delete require their
own explicit contracts and acceptance evidence. Symbol moves require the
host-selected bounded contract above; captured-file `jb.move` selectors follow
the separate facade contract. Independently
owned protocol fixtures establish the local contract; they do not establish live
IDE or compiler parity.

## Explicit native Python empty-body deletion

Native `ci_change_prepare` accepts an optional closed policy on `deleteSymbol`:

```json
{
  "snapshotId": "CAPTURED_SNAPSHOT_SHA256",
  "deleteSymbol": {
    "entityId": "CAPTURED_ENTITY_SHA256",
    "emptyPythonBody": "pass"
  }
}
```

Replace the placeholders with actual captured IDs. Omitting `emptyPythonBody`
or selecting `reject` retains the released refusal when deletion would leave
a required Python block without a statement. Explicit `pass` inserts exactly
that keyword at the checked declaration's original start only when its parent
block has no other non-comment statement. Existing docstrings, assignments,
siblings and module-level deletion need no filler. The selected function/class
owns its checked decorator wrapper; parent decorators and neighboring comments
remain outside that edit. Original entity/evidence coordinates, source bytes,
BOM, CRLF and indentation stay authoritative.

The option is Python-only and cannot be mixed with explicit edits, another
symbol operation, rename or bulk replacement. Unknown values, JSON types or
fields refuse. The original source must have error-free syntax and match the
exact bounded AST declaration/name/kind/ranges. The same bounded parser checks
the entire resulting Python text after the proposed replacement, under the
shared deadline. Source is limited to 4,000,000 bytes and the original AST walk
to 200,000 nodes. No analyzed module, decorator, model or provider is executed.

This is an additional explicit native byte-edit contract. The eight-tool MCP
surface and existing default deletion behavior remain. Semantic-provider/facade
deletion still uses its original policy. Saved plan shapes, exact delta/review
replay and legacy identities remain unchanged; new planned bytes use the normal
plan identity. This option creates no approval, verification, application or
recovery authority. Workspace application still needs separate host write
permission, exact approval, executed verification and fresh source conditions.
Host-selected Python stdlib AST-only verification can inspect a copied result
without importing or executing the analyzed module; that check establishes
syntax/selected structure, not runtime behavior or exhaustive consumer safety.

## Explicit native sibling-declarator deletion

Native `ci_change_prepare.deleteSymbol` can separately select
`"siblingDeclarators": "preserve"` to delete one captured TS/JS function-valued
variable binding while preserving its siblings. The default is `reject`, and
the ordinary semantic-provider/facade routes retain their current behavior.
This singleton option cannot be combined with `emptyPythonBody`, another symbol
operation, explicit edits, rename or bulk replacement. It grants no usage,
approval, source-write, verification or recovery authority.

The selected identifier/declarator must bind the exact original source AST.
Only a plain statement `const`, `let` or `var` in program/statement-block scope
and its checked export/ambient wrappers are admitted. One direct parent comma
is selected from the AST, never from textual delimiter search. Two separate
deletions preserve all unrelated bytes. Direct inter-declarator comments and
unsupported binding/container forms refuse before publication. The complete
result must parse without syntax errors under the original deadline and finite
source/AST bounds. See [symbol deletion](symbol-deletion.md) for exact boundaries.

## Concrete class-method decorator ownership

Native `deleteSymbol` and the final source-preparation step of provider-based
`safeDelete` now bind preceding decorators to a selected concrete TS/JS class
method with a plain identifier name. The original method/name/entity coordinates
remain unchanged. Only deletion derives a broader checked owning range: up to
64 contiguous direct sibling decorators, their intervening comments and
whitespace, and the exact method body. Earlier comments, class decorators and
other members' decorations remain outside it. Unsupported gap tokens, constructor,
signature/abstract members and computed/quoted/private decorated names refuse.
Whole-result syntax replay uses the same finite source/AST/deadline bounds.

No new input flag or host authority is introduced. Existing symbol insert/
replace ranges, usage checks, immutable plan identity, approval, verification,
write permission, stale conditions and recovery remain separate. Actual
compiler/AST and language-server evidence qualifies the selected installed
versions and legacy experimental-decorator fixture mode. It does not establish
unusedness, exhaustive reference/dispatch semantics, other decorator modes or
general IDE/compiler equivalence.
