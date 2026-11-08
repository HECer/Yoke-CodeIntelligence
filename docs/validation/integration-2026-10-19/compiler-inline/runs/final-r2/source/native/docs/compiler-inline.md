# Explicit immutable TypeScript compiler inlining

The optional host-selected TypeScript **5.8.3** compiler can prepare native
`inline` plans for captured TS, TSX, JS and JSX function-valued variable
declarations. It uses the compiler's actual `refactor.inline.variable` action,
including arrow and function expressions, sibling declarators, JSX expressions
and object shorthand consumers. It substitutes the function value at its bound
uses; it does not expand a function call into an arbitrary surrounding statement.
For example, `helper(2)` can become `((n) => n + 1)(2)`.

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

Classic function declarations are not supported by this compiler action. Other
languages, multiple owning contexts, consumers outside the selected file,
alternative escaped identifier spellings, unowned removal trivia and other
replacement shapes remain explicit boundaries. Broader function-body inline,
symbol moves and universal IDE parity require further implementation and evidence.
