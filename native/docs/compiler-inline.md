# Explicit immutable TypeScript compiler inlining

The optional host-selected TypeScript **5.8.3** compiler can prepare native
`inline` plans for captured TS, TSX, JS and JSX function-valued variable
declarations. It uses the compiler's actual `refactor.inline.variable` action,
including arrow and function expressions, sibling declarators, JSX expressions
and object shorthand consumers. It substitutes the function value at its bound
uses; it does not expand a function call into an arbitrary surrounding statement.
For example, `helper(2)` can become `((n) => n + 1)(2)`.

The same explicit host profile also offers independently implemented classic
function-body expansion. It places checked statements in the caller and removes
the selected call. This separate `native.inline.function.body` action uses actual
compiler binding/type observations, while Rust reconstructs the full replacement
from original source. It does not claim to be a TypeScript built-in refactor.

## Classic function-body statements

One owning strict ES module can select a nonasync function declaration without a
TypeScript type-parameter list and complete direct calls in that captured file.
A call must occupy one whole
single-declarator lexical initializer, return expression, expression statement,
throw expression, if condition or switch discriminant. Parentheses are retained.
Non-declaration statements receive a block so unbraced branches retain their
control scope. Direct if/switch/throw heads evaluate the selected call once before
the original statement. The worker and independent Rust reconstruction require
the exact original AST field; original branches, cases, labels and comments remain
source copies. Loop tests, compound/short-circuit/nested expression contexts and
overlapping statement frames remain unavailable. See the
[statement-head qualification](../../docs/validation/integration-2026-10-23/inline-statement-heads/README.md).

Exported definitions require `keepDefinition=true`; captured
consumers outside the file, recursive references and noncall/write consumers refuse.

Free references in the classic body and its TypeScript signature use the pinned
compiler's reference meaning when resolving each consumer: type, value and
namespace bindings remain separate. A value parameter called `Result` does not
capture a copied `Result` type alias; a type alias called `BASE` does not capture
a copied `BASE` value. Qualified type roots use the namespace meaning, while
`typeof` queries and object shorthand retain value meaning. Actual shadows in
the required namespace still refuse. Missing or unknown reference classification
fails closed. This uses TypeScript 5.8.3's internal `getMeaningFromLocation`
export; it adds no compatibility with another compiler version. Existing source
copies, before/after diagnostics, binding/type observations and independent Rust
reconstruction remain mandatory. The
[direct compiler qualification](../../docs/validation/integration-2026-10-24/typescript-namespace-bindings/README.md)
does not establish native integration acceptance.

Every argument, including extra arguments, is evaluated once before any parameter
initialization.
Simple literal and identifier arguments retain direct fresh bindings so original
generic literal inference is preserved. Other arguments use a fixed pure rest
collector receiving the original argument list once. Both finite recipes retain
original source/binding/type checks; direct anonymous expressions also receive a
source-derived name barrier if that recipe is supplied. Its
callable type comes from an inert source-parameter signature; no analyzed function
or default is invoked to collect values. Original signatures and generated tuple
return types preserve contextual callback/property/tuple types, JSDoc template
instantiation and anonymous argument names. Fresh raw argument arrays are read
only at their own dense indices. Destructuring these arrays would invoke a
potentially replaced Array iterator and is deliberately excluded. The existing
parameter initialization then consumes the fresh argument bindings, preserving
left-to-right defaults, rest/pattern effects and parameter/body environments.
Two original-source function carriers remain inside the fixed false void guard:
one supplies fallback parameter types, and the other supplies the callable
signature with a fixed throwing body. Its parameter defaults never execute.
Only the generated `(...fresh) => fresh` collector executes in an applied program;
preparation still executes no analyzed source. Source-derived documentation type
pieces and template tags retain the existing compiler free-binding checks.
Native checks attached-comment containment, UTF-8 bounds and exact source copies;
the host compiler supplies the bounded JSDoc type/tag enumeration. A separate lexical block initializes parameters left to right,
then the original body block follows. Defaults, rest arrays, object/array patterns
and optional TypeScript annotations are supported within the admitted scope.
Parameter and argument comments retain their exact original bytes. Parameter
separator commas are omitted when lowering declarations. Argument commas stay in
the original collector call list; direct fresh bindings omit only the AST separators. Anonymous default function/class names
use computed own-property initialization, including the `__proto__` name. Original
parameter types also provide contextual typing for copied default expressions;
no arbitrary compiler-supplied type text is inserted.

Outer returns assign a fresh result and break a fresh label; nested function
returns stay intact. The original body block is the consequent of the fixed
runtime-true guard `void 0 === void 0`; an outer labelled block is emitted only
when outer returns need its label. This retains enabled unreachable-code and
unused-label diagnostics without adding unreachable scaffold statements. Normal
body completion, after that original block, clears the result to `void 0`, since
a finally break/continue can cancel a pending return. Original finally blocks
still run and may override a return. An explicit ordinary value return annotation is copied
from source; inferred primitive return hints have a fixed keyword whitelist.
[Predicate/assertion signatures](compiler-inline-predicates.md) use an inert
original signature and fixed result relay instead of treating a predicate as a
value type. Eligible captured lexical targets retain a separately checked pure
caller reference for narrowing; calls/getters are never repeated. Dense collector
indices retain original tuple element types under noUncheckedIndexedAccess,
including genuine undefined alternatives.
That original TypeScript annotation also types the result storage, with a fixed
undefined alternative for normal completion. Assigning returned expressions into
this storage retains contextual typing for function-valued properties, methods,
tuples and callbacks. Source type identifiers retain the original binding checks;
a consumer type shadow refuses. No worker-supplied type expression replaces the
annotation. Anonymous returned function/class expressions use a fixed
`(void 0, originalExpression)` barrier so
anonymous arrows, function expressions and classes keep the empty runtime name
of a returned expression, rather than acquiring the generated result variable's
name. Parentheses and erased TypeScript wrappers retain this classification.
Named expressions and other return values use the original expression wrapper.
In particular, never-typed calls keep their existing compiler flow; inserting a
comma wrapper would make a subsequent generated break appear unreachable.
The void operand adds no
source evaluation and preserves contextual typing. Finally overrides and
cancellation retain the completion behavior described above.
Generated result assertions use a fixed intermediate `unknown` assertion so a
throwing/never-returning source does not make the scaffold's undefined path an
invalid conversion. The original compiler context must still have no errors.
Other inferred result types use fixed `typeof` syntax over a fresh result carrier.
A separate function carrier has exact original parameters and body, including
attached leading JSDoc comments for JavaScript typing. Trailing comments on the
previous statement are excluded, including newlines inside block comments. A call
through the function carrier uses only the already evaluated fresh argument
temporaries, preserving JSDoc template instantiation, literal types and argument
order without reevaluating source expressions. Both carrier initializers and the
result read occur only under `void 0 !== void 0`; the clone and its call never
execute. Every copied binding and generated argument mapping has independent
native correspondence. The compiler additionally checks free type/value bindings
in JSDoc type expressions and template defaults against the original consumer
scope; local documentation declarations remain owned by the copied signature.
No ambient `ReturnType` helper or arbitrary compiler type expression is trusted.
Literal const/contextual results use a fixed readonly `value` wrapper and indexed
typeof query to prevent widening; other contexts retain ordinary inference. A
generated TypeScript const-assertion token is admitted only at its exact parsed
assertion position, without accepting arbitrary new identifiers.
Module typedef/callback/import declarations stay at their original site. When an
attached comment mixes such declarations with function typing, only original
function-tag byte ranges are copied into fresh comment delimiters. Native checks
attached-comment containment, ordering, a closed tag set, UTF-8 bounds and a
512-piece limit. This preserves alias identity while templates remain scoped to
the function copy; source comments and raw type observations are retained.
Removing a definition with attached leading JSDoc leaves a fixed empty statement
at its original declaration span. Original comment bytes and typedefs remain;
the comments cannot silently reannotate the following declaration.

Function-owned `var` names are hoisted once inside the body block. Initializers
stay at their original execution positions, including ordinary `for`, `for-in`
and `for-of` loops. The hoisted storage is shared by loop closures; it does not
create a fresh lexical variable for each iteration. Duplicate and escaped names
use their decoded function binding. Nested functions and classes retain their
own declarations unchanged. Parameter collisions copy initialized parameter
values before the body block, so closures created by parameter defaults retain
their separate parameter environment. A `var` initializer inside a simple
same-name `catch` still assigns the catch parameter while retaining its distinct
function-owned declaration.

Original explicit type annotations provide contextual typing for initializers.
Inferred primitive storage types use the same fixed keyword whitelist, with exact
original type witnesses and explicit initial `undefined` storage. Other inferred
storage types now use fresh `typeof` carriers inferred from original declarations
or loop heads at their original lexical positions. Native reconstruction wraps
those copied expressions in the fixed guard `void 0 !== void 0`. Both operands
evaluate to `undefined`; the copied initializers, iterable expressions and reads
cannot execute. The type carriers' `var` bindings are scoped by the containing
function/module, while actual body storage remains lexical and fresh per expansion.
No arbitrary compiler type expression is inserted. Quoted shorthand keys preserve
decoded property names; labelled and unbraced statements retain their control scope.
Source/member bindings and destructured declaration types remain separately checked.
The fixed guard preserves compiler diagnostic options; it does not suppress errors.
Some valid runtime forms, including TypeScript's rejected
function/var declaration combinations, therefore still refuse at the compiler
gate. Destructuring property reads and assignment-property syntax have separate
checked correspondence roles; raw compiler observations remain intact.

Native admission checks exact function/call/statement anchors, canonical fresh
names (including escaped identifier spellings), source-copy correspondences,
complete reported bound-call coverage and original identifier coverage. Before
and after observations preserve original static targets and surviving caller
declaration types. Captured/module/library targets and every type row are checked,
including observations for removed call/declaration names. Result grammar and
exact bytes must match the independent reconstruction before ordinary planning.
Compiler observations remain those of the trusted host installation, not signed
semantic attestations. Preparation executes no analyzed source; explicit runtime
oracles execute only owned literal test fixtures.

This route admits at most 128 call frames, 64 arguments and 64 parameters per
frame, 4 MB aggregate replacement text, 20,000 source-copy pieces, 100,000 binding
rows and 10,000 type rows per observation set. Each type is at most 16 KB, with
2 MB aggregate type text per set. Shared compiler request/reply/deadline bounds
still apply. Limits refuse instead of clipping a complete result.
Source inference additionally admits at most 512 original declaration/loop sites,
2,000 carrier names and fewer than 2,000 labels in an owning chain. Carrier text,
copy pieces, compiler observations and aggregate edits share the limits above.
Argument collection uses seven fresh names for a nonempty call, at most 64
parameter documentation slots and 64 template pieces, each at most 16 KB.
Original signature binding reads admit at most 2,000 parameter bindings and share
the output/copy/AST/deadline bounds. Inferred result carriers use two fresh names per frame. JSDoc ownership admits
at most 64 attached documentation comments, with at most 200,000 contiguous
comments and 200,000 leading whitespace characters inspected. These checks also
guard definition removal; complete source copies share the byte/copy/AST limits.

Invocation-bound `this`/`super`/`arguments`/meta properties,
async/generator/generic frames, spread arguments, wider expression sequencing,
nominal private classes and broader declaration/context/runtime equivalence
remain open. Such inputs fail visibly. Body expansion remains `complete=false`
and preserves all independent verification, approval, application and recovery
gates below. The broader STATUS/R1 requirements are not completed by this route.

## Host registration and requests

A session host profile can explicitly opt in:

```json
{
  "inlineCompiler": {
    "node": "C:/Program Files/nodejs/node.exe",
    "typescript": "C:/tools/typescript/lib/typescript.js",
    "trusted": true
  }
}
```

Both paths must be absolute and outside every host-registered analyzed project.
They are selected by the host; source/configuration contents and tool arguments
cannot register a compiler or grant trust. Native indexing and default sessions
continue to require no Node installation. An absent registration preserves the
previous host serialization and session fingerprint. Registration changes, or
changes to either selected executable/module hash, invalidate session resumption
before its stored row is rebound. An open session checks the current binding
again before using the compiler.

Use the existing session CLI and `refactor.prepare` operation:

```text
yoke --root C:/project session --id work --host-profile C:/host/host.json --requests C:/host/requests.json
```

```json
[
  {
    "operation": "refactor.prepare",
    "args": {
      "operation": "inline",
      "arguments": {
        "relativePath": "src/main.ts",
        "namePath": "helper",
        "keepDefinition": true
      }
    }
  }
]
```

The same route is available through the existing `ci_session` MCP operation and
`jb.inline_symbol` facade, without new tool arguments. Editing mode, active-project
write eligibility and the exact current session record are checked first.
Non-TS/JS languages retain their existing selected LSP path. For TS/JS, explicit
compiler selection does not fall back to executing an LSP command. Unsupported
selections and unavailable compiler actions fail visibly.

## Source and effect checks

The compiler reads only immutable captured files and the selected installation's
direct standard-library declarations. It has no source filesystem fallback,
does not load repository compiler plugins and never imports analyzed modules.
Exactly one captured configured context must own the selected file; a configless
context is explicitly inferred and nonexhaustive. Ambiguous contexts, project
configuration errors and compiler errors before or after the projected change
refuse preparation. Original and result diagnostic spans are checked against
their respective bytes. Compiler context identity includes actual library hashes.

The selected entity must match its original analyzer 8/9 producer and exact
variable-declarator, initializer and name AST intervals. The statement must be
writable and non-exported. The compiler checks actual identifier bindings; other
captured files consuming a script-global binding refuse this finite operation.
Recursive variable initializers, writes, exported references and unavailable
actions refuse. Free identifiers must resolve to the same declarations at every
consumer; intrinsic JSX tags and attribute keys are classified separately from
component values. Shadowing a component/value at a consumer refuses. `this`,
`super`, `arguments` and meta-property forms are conservatively unsupported.

Rust independently admits original-coordinate edits before publishing a plan.
Every nondeletion edit must replace one exact reported consumer name span.
Object shorthand expansion must preserve the original property key. Parsing and
structural comparison require each replacement to contain the original function
initializer, allowing compiler formatting, parameter parentheses and optional
statement semicolons while retaining operators, literal bytes, statement shape
and expression precedence. Other statements, properties or declarations refuse.

One or two contiguous deletion pieces must together remove exactly the selected
complete statement, or the selected sibling declarator and one direct AST comma.
Only surrounding whitespace may join these spans; unrelated comments, siblings
and preambles are protected. The compiler's separate comma edit for a last
declarator is checked as part of the same bundle. With `keepDefinition=true`, all
admitted deletion pieces are omitted while separately checked consumer edits
remain. The retained declaration's original bytes are preserved even when a
consumer precedes it. Whole-file edits and resource operations cannot establish
this effect. Final grammar and the exact result hash are checked independently.

The shared request deadline, process cleanup and compiler limits remain:
20,000 captured files, 10 MB captured text, 16 MB request, 32 MB reply and a
512 MB Node heap. This adapter additionally admits at most 4 MB per changed file,
2,000 original-coordinate edits and 1,999 consumers. Native copied-syntax walks
have finite node/frontier/depth limits; compiler AST traversal is bounded too.
These are finite processing limits, not an OS sandbox or all-allocation proof.

## Guarded application and remaining scope

The result contains one ordinary plan under `actions[0].plan`, selected compiler
and host hashes, library/context/source bindings, diagnostics and an effect digest.
These are audit correlations, not independently signed compiler attestations.
Serialized result metadata grants no authority. Preparation writes native state
but leaves analyzed sources unchanged. Existing independent executed verification,
approval, workspace-write permission, freshness and conditional recovery remain
mandatory. The compiler's planning diagnostics alone cannot authorize application.
The existing nontransactional plan/derived-source publication boundary remains.

`commandsExecuted=false` means no LSP/provider command was executed: the explicitly
trusted Node compiler worker did run. No model or analyzed source was executed.
The trusted compiler retains host process privileges. Results remain
`complete=false`: omitted/dynamic consumers, allocation identity, closure lifetime
and observed runtime behavior are not proved by a compiler substitution.

Classic function declarations use the separate body action above. Other
languages, multiple owning contexts, consumers outside the selected file,
alternative escaped identifier spellings, unowned removal trivia and other
replacement shapes remain explicit boundaries. Broader function-body inline,
symbol moves and universal IDE parity require further implementation and evidence.

Attached caller JSDoc stays with the original lexical declaration after the
inserted statements. The source parser independently derives the widened edit
extent; the compiler provides no documentation-range override. Original comment
bytes, trivia and typedef declarations move once in the same lexical scope,
including exported declarations. Existing source correspondences retain type
declaration identity. Trailing comments that were not attached remain in place.
When original callee documentation also declares module typedefs/callbacks/imports,
its copied function tags form one contract comment. Keeping template, parameter
and return tags together preserves their binder on inert function expressions.
Unknown fields, changed comment extents/types and coherent forged projections
remain subject to independent native reconstruction and application gates.

Removing any body definition leaves a fixed empty statement. This preserves the
original statement boundary even when a trailing JSDoc comment was not attached
to either declaration; deletion cannot turn it into a generated variable type.
