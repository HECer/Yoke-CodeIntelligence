# Explicit immutable TypeScript compiler moves

An optional host-selected TypeScript **5.8.3** compiler prepares moves of one
top-level TS/TSX/JS/JSX declaration into an existing captured ES module. Its
actual `refactor.move.file` action supplies changes. Native admission checks
original bytes, resulting syntax and static binding correspondence before
creating an ordinary guarded plan. Source files stay unchanged.

Register external tools in the session's explicitly supplied host profile:

```json
{
  "moveCompiler": {
    "node": "C:/Program Files/nodejs/node.exe",
    "typescript": "C:/tools/typescript/lib/typescript.js",
    "trusted": true
  }
}
```

Both paths must be absolute and outside every registered analyzed project.
Their hashes bind session identity and are checked again before use. Absent
registration preserves original host serialization, fingerprints and routes.
Repository configuration grants no tool/trust authority. Default indexing still
needs no Node installation. Use the existing session CLI or `ci_session`:

```json
{
  "operation": "refactor.prepare",
  "args": {
    "operation": "move",
    "arguments": {
      "relativePath": "src/source.ts",
      "namePath": "helper",
      "targetRelativePath": "src/target.ts"
    }
  }
}
```

`jb.move` takes the corresponding snake_case arguments. Editing mode, current
session revision and active-project write eligibility remain required. The
destination must already be a captured file; symbol-parent selectors refuse.
File/directory moves and other languages keep their existing routes.

## Admission

The compiler sees immutable captured source and its trusted standard libraries.
It loads no repository plugins, executes no project modules/commands and makes
no model call. Before/after diagnostics must have no errors. Source and target
must share one unambiguous captured compiler context; incomplete configuration
or references refuse visibly.

Native checks preserve the moved declaration and contiguous leading comments,
unrelated syntax, literals/operators/control flow, directive prologues and
original BOM/shebang preambles. The normalization
`ordered_insertions_and_original_bom_v1` preserves compiler insertion order,
excludes the original leading BOM from removals and places insertions after it.
Native code independently reproduces normalized operations and result hashes.

Original/result definition spans map through independently parsed syntax.
Unchanged files retain their byte anchors. Known bindings and merged declaration
order must agree; namespace repairs require the selected declaration and module
observations. Import/reexport statements and binding-name coverage are checked
against syntax. Unrelated exports retain their binding. New source exports may
only expose original free dependencies used by the moved declaration;
`bindingCheck` discloses added exports and unresolved/unmodeled atoms.

Real cases cover functions, classes, interfaces, aliases, enums, function-valued
bindings, scalar variables and namespaces in both analyzer epochs. Some variables
and namespaces lack an original graph entity: their explicit
`independent_captured_variable_or_namespace_v1` AST origin has null `entityId`.
No graph entity, producer or identifier is invented or rewritten. Unicode,
CRLF/BOM, JSX components, private/imported dependencies, named/namespace consumers,
unrelated reexports and inert repository plugins have owned regression evidence.

All edited paths must be captured writable TS/JS files; a read-only consumer
needing repair refuses the whole plan. No resources are created. Bounds include
20,000 files/10 MB input, 2,000 combined original/repair operations, 4 MB per changed file and
16 MB total changed output. Protocol/AST/library and inherited deadline bounds
remain. Independent verification, explicit approval, stale-source preconditions,
application and conditional recovery use the existing changes pipeline. Real
CLI evidence runs independent `tsc` on copied results and preserves foreign writes
when recovery refuses.

## Remaining scope

Results retain `complete:false`. Static observations and preserved syntax prove
no dynamic consumer completeness, allocation/runtime equivalence, module
evaluation-order guarantee or observed execution. Other contexts/languages,
merged declarations, sibling binding bundles, new resources, symbol-parent
destinations and full upstream acceptance remain open.

## Source-only compiler repairs

TypeScript 5.8.3 can leave direct named reexports of the selected declaration
unrepaired and import dependencies under names already used in the target.
The separate repair stage partitions selected reexport specifiers, retaining
external aliases, type-only flags, comments and source styles. Its module
specifier comes from the original language service's File Rename analysis;
no file is renamed. Namespace/default/type-only imports and imports that would
shadow an original global binding receive fresh local aliases. Original target
and dependency declarations retain their names. Object shorthand expansion
preserves its original property key and separately checked value binding.

Rust independently checks each repair's original/current source anchors,
selected export membership, captured free references and import-name ownership.
It reconstructs the edits and an original-byte piece table. The table composes
later edits inside compiler-inserted text without replacing untouched file
regions. `sourceRepairs` and `editComposition:original_byte_pieces_v1` disclose
these steps only when used. The original no-repair operation list remains intact.
Up to 256 repair steps share the original 2,000-operation budget. Final syntax,
static bindings, export APIs, source/result hashes, permissions and independent
verification/application/recovery gates still apply to the complete result.

Omitted free dependencies now have separate repairs. `missingImport` selects an
original named/default/namespace import binding and only its captured free
references. `sourceDependency` selects an original top-level source declaration
and adds a named source export only when needed. Original and current definition
anchors, declaration boundaries, imported targets and new module linkage are
checked independently. New specifiers must be relative candidates for the exact
captured original module; the compiler resolves that same candidate before use.
No bare identifier lookup guesses a dependency or grants new source access.
Imports are inserted after the moved statement, preserving original preambles,
directives and adjoining documentation. Existing source export APIs remain bound.

Shorthand `__proto__` expands to `["__proto__"]: alias`, retaining an own data
property. Native syntax correspondence distinguishes this from the prototype
setter form `__proto__: alias`; a static binding match cannot disguise that
semantic change. An explicit owned regression fixture checks actual property
and prototype behavior. Preparation itself still executes no analyzed source.

## Literal import attributes

Original `with { ... }` clauses now survive compiler omissions and recovered
shorthand-only dependencies. Named/default/namespace imports retain original
literal and comment bytes, including Unicode and CRLF, before independently
selected host after-formatting. The separate `importAttributes` repair names its
original and current import spans; Rust reconstructs the insertion from original
source. Missing-import repairs use the same original suffix. Neither route can
supply arbitrary new attribute text.

Native admission independently parses original/result linkage statements and
compares token fingerprints for each compiler-bound imported/reexported target.
A changed/dropped clause, invented request or ambiguous original policies for
one target refuses the whole plan. Whitespace may be formatted; literal tokens
and comments remain bound. Standalone side-effect imports retain their existing
exact comparison. Absent attributes keep preceding fields and identity rules;
fresh plan IDs still incorporate their ordinary creation timestamp.

Bounds are 65,536 bytes, 256 literal string pairs and 4,096 syntax visits per
clause; aggregate signature text is at most 4 MB and named binding checks at most
200,000. Comments inside the object are supported. Other parsed clause shapes,
ambiguous policies and unqualified consumer/attribute forms refuse visibly.
Existing source/AST/edit/output/deadline limits still apply. The new checks add
no runtime, filesystem, network, model, approval or write authority.

Attribute contents are host-specific; [TypeScript's documentation](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-3.html#import-attributes)
describes their loader boundary. Preserving source requests and static bindings
does not certify actual loader interpretation, module evaluation order or runtime
equivalence. Results retain `complete:false` and independent verification,
approval, source freshness, application and conditional recovery gates.

These remain finite compiler repairs, not an exhaustive bug workaround. Sibling
declaration bundles, other attribute forms, broader export/import/context
and destination/language shapes, module evaluation order, runtime equivalence
and full acceptance remain unqualified. The own_v1 LSP contract retains its
separate finite scope.
