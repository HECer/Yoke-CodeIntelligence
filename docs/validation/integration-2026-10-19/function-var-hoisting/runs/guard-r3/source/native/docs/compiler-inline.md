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

One owning strict ES module can select a nonasync, nongeneric function declaration
with complete direct calls in that captured file. A call must occupy one whole
single-declarator lexical initializer, return expression or expression statement.
Return/expression statements receive a block so unbraced branches retain their
control scope. Exported definitions require `keepDefinition=true`; captured
consumers outside the file, recursive references and noncall/write consumers refuse.

Every argument, including extra arguments, is evaluated once before any parameter
initialization. A separate lexical block initializes parameters left to right,
then the original body block follows. Defaults, rest arrays, object/array patterns
and optional TypeScript annotations are supported within the admitted scope.
Parameter and argument comments retain their exact original bytes while their
actual AST separator commas are omitted. Anonymous default function/class names
use computed own-property initialization, including the `__proto__` name. Original
parameter types also provide contextual typing for copied default expressions;
no arbitrary compiler-supplied type text is inserted.

Outer returns assign a fresh result and break a fresh label; nested function
returns stay intact. Normal body completion clears the result to `void 0`, since
a finally break/continue can cancel a pending return. Original finally blocks
still run and may override a return. An explicit return annotation is copied
from source; inferred primitive return hints have a fixed keyword whitelist.

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
types must pass the full projected compiler context without generated arbitrary
type expressions. Some valid runtime forms, including TypeScript's rejected
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
