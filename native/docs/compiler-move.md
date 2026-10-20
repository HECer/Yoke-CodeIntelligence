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

This is not an exhaustive compiler bug workaround. TypeScript can also omit a
required import when its only use is shorthand. A diagnostic-free substitution
of an existing target variable is explicitly rejected by native binding checks;
missing-import synthesis remains open. Wider export/import/context shapes,
runtime semantics and full acceptance remain unqualified. The own_v1 LSP
contract retains its separate finite scope.
