# Direct once-only function-body statement heads

Explicit host-selected TypeScript 5.8.3 body inlining now admits a complete call
as an `if` condition, `switch` discriminant or `throw` expression. Parentheses
are retained. The worker and independent Rust checker derive the exact original
AST field; the existing frame executes arguments/body once before the original
statement. Original branches, cases, labels, completion behavior and source
pieces remain bound to the reconstructed replacement. Non-declaration statements
receive a block. This adds no general expression hoisting or automatic execution.
Loop conditions, short-circuit/compound/nested expressions and overlapping frames refuse.
Original permissions, immutable capture, diagnostics, binding/type correspondence,
verification, approval and conditional recovery remain mandatory; complete=false.

At source e01920adcf05b0a4fb3f6ff519179748400800f2, five whole Rust groups pass **630 functions**, zero failures,
seven existing ignored functions and no filtered functions: library 581,
body 21, predicates 10, variable-inline 16 and main 2. Five added functions are
already included. All-target Clippy with warnings denied, complete 362-root format,
build, Python 47, typecheck and npm 260 with three existing skips pass. All 1112
frozen source/control inputs remain exact through qualification and actual CLI
validation. All original body integration bytes remain an exact 69244-byte prefix
(SHA d7c59cc61d9f0e9373a23776f169839d9f9d51eeae6f7eeaf8d83dbf78a7f5ce);
all original body unit bytes remain an exact suffix. No original fixture, ignore,
internal deadline or profile changes. Only exact passed artifacts receive
byte/hash-preserving per-file LZX compression; no directory cleanup or release
retry is performed.

Three actual primary-CLI scenarios exercise if-predicate, switch and throw under
strict compiler diagnostics. All 27 owned parent processes end in empty Windows
Jobs. Unverified/unapproved applies refuse without source changes. Independent
tsc, approval and workspace apply succeed. Before/after owned literal runtime
observations agree. Recovery refuses and preserves foreign bytes, then restores
exact originals after the owned postimage is restored. Unicode appears before
the selected declaration and in quoted/Unicode root paths; LF/CRLF source
preservation, finally, switch case evaluation and labelled control are checked.
Preparation executes no analyzed source; runtime oracles execute only our literals.

Failing phases remain intact: RED1 includes an owned JavaScript fixture whose
JSDoc attachment needed a newline; RED2 then records three intended sequencing
refusals and one adversarial pass. Unicode substituted by the PowerShell pipe in
a new comment is corrected after RED2, before GREEN; final-test byte identity
with RED2 is not claimed. Intermediate GREEN 4/17-filtered is not added to the
final 630. See [checkpoint](checkpoint.json) for source/binary identities,
[group and CLI audit](checkpoint.json) and [indexed exact raw evidence](raw-index.json).
Every member of [raw-logs.zip](raw-logs.zip) is reopened and length/hash verified.
Only five frozen contract/status documents change after qualification, recorded
in [post-review hashes](post-review-docs.json).

Wider body/expression/invocation and predicate forms, original 353 inventory,
R1-R9, 77-runtime, current whole Windows/client/quality/hosted/canonical VSIX,
Google live export, main and release acceptance remain open. No Developer Mode,
privilege, execution-policy, PS1, protected/unvalidated executable or unrelated
process changes. No current VS Code or release acceptance is inferred.
