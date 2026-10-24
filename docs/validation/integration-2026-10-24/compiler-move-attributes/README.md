# Compiler move import attributes — Code Intelligence native 0.5.0

Qualified product `2240fadd29dbfb361f706dfc5a97be510e6cf3bb` on owned Windows fixtures. This is a bounded R1
checkpoint; full STATUS, current whole Windows, Unix/hosted CI, canonical VSIX,
Google live access, main and release acceptance remain open.

The previous qualified CLI accepts a move that drops a `with` clause while its
static bindings pass (`red1`). The current CLI preserves the same original
attribute bytes in its published plan, and independent TypeScript 5.8.3 `tsc`
passes on the copied result (`live1`). Planning and verification leave the owned
source files unchanged. No analyzed module or model runs.

651 Rust functions pass in five unique whole groups, with zero failures, seven
existing ignores and no filtered tests. The seven added functions are included.
594 library and six CLI functions run in the final phase. The three integration
groups (5 + 25 + 21) retain identical product and integration-test inputs; only a
new `cfg(test)` adversarial fixture changes before the final library run.
All-target Clippy with `-D warnings`, 367-root formatting, locked offline build,
`npm run typecheck` and npm tests (260 pass / three existing skips) pass.

Actual tests cover named/default/namespace imports in TS/TSX/JS/JSX, omitted
shorthand dependencies, Unicode/comments/BOM/CRLF and nested relative relocation.
Conflicting original attribute requests, path/deadline/budget/protocol violations,
changed or omitted attributes with matching result hashes and coherent import
spans/identifier observations refuse. A real CLI test independently checks write
permission, approval, executed compiler verification, stale-source refusal,
application and conditional recovery preserving a foreign write.

Three additional old/new CLI calls retain every result field except the existing
clock-derived plan ID, createdAt and expiresAt. Repeating the old CLI shows the
same three differences. Effect digest, diff, file effects and snapshot IDs are
exact. Three further current CLI calls admit each saved old/new plan through the
real plan reader and execute independent copied-result tsc successfully. This
proves saved-plan compatibility; fresh raw IDs/results are not claimed equal.
Existing inline regressions separately execute only their explicitly authored literal
runtime fixtures; attribute planning and compiler verification execute no analyzed
module. All reviewed Windows Jobs end empty.

Earlier attempts remain archived. `green1` stops on the unchanged 500 MB disk
guard before tests, preserving the previous CLI. Its compiled fresh integration
harness independently passes four functions in `recover1` (not counted again).
`green2` passes all 51 integration functions, then reports 593 library passes and
one new-fixture assertion failure: erased attribute text still had stale compiler
identifier spans. The corrected fixture removes those observations and retains
the attribute-specific refusal assertion. `compatibility1` incorrectly expects
fresh plan IDs/expiry fields to stay fixed; `compatibility2` records the observed
old-repeat behavior and verifies all three saved identities through the real
current reader. No original test or ignore changes.

File-only lossless compression retains exact logical sizes and SHA-256 hashes.
The selected files are positively exercised historical CLI copies and explicit
Cargo artifacts associated with a passed whole harness; copied qualification
proofs are included. Cache compression is not independent dependency semantics
or release certification. No deletion, system/privilege/profile change, protected
output content access or optimization of an unqualified executable is performed. The
stale release binary remains unused.

See the [native contract](../../../../native/docs/compiler-move.md#literal-import-attributes),
[checkpoint](checkpoint.json), [raw member index](raw-index.json) and
[reopened raw archive](raw-logs.zip). Archive: 191 members,
656533 bytes; SHA-256 `d0df0a4de09ba7dbf4430b8e6972067669a95d72afc6f30080d878dda8fe7a49`.
