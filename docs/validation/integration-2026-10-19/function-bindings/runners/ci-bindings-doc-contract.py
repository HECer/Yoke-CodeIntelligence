from pathlib import Path
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=w/'native/docs/propagated-delete.md';s=p.read_text(encoding='utf-8');assert '## Explicit function binding policy' not in s
s+='''
## Explicit function binding policy

`propagationPolicy: "captured_es_function_propagation_v3"` additionally admits
one plain `const`, `let` or `var` binding per module-level declaration statement.
Its initializer must directly be an arrow function or an anonymous function
expression with a body. Ordinary named function declarations remain supported.
An owning `export` wrapper belongs to its one declaration. Omission still selects
v1; v1 and v2 keep their original named-function module boundary.

Each binding must match an existing captured function entity, including its exact
variable-declarator range, identifier name and name-byte range. No entity or
analyzer identity is invented or rewritten. The removable statement range owns
the declaration keyword, semicolon and optional export wrapper; the recorded
entity and evidence ranges keep their original coordinates.

Multiple declarators, binding patterns, parenthesized or nonfunction initializers,
named inner function expressions, using/ambient declarations and unclassified
direct declaration comments remain unsupported. Surviving assignment or value
consumers prevent removing the target even with force. Surviving or exported
dependencies remain protected; private cycles and argument dependencies use the
same source-bound closure without sweeping unrelated unused bindings.

V3 retains v2's complete-call wrappers, full-result grammar and surviving-node
comparison. Exact definitions/references and provider/source context remain
mandatory. All existing source, sixteen-declaration, thirty-two-request,
reference and128-usage limits, original deadline/cancellation and guarded plan
application/recovery remain. This adds declaration admission, not general alias,
rebinding, execution or exhaustive consumer analysis. `complete` and
`exhaustiveCompilerProof` remain false; unknown consumers remain visible.

The native session CLI and `ci_session` route accept this opt-in policy through
the existing native arguments. See [session planning](agent-sessions.md#native-semantic-planning).
'''
p.write_text(s,encoding='utf-8')
p=w/'native/README.md';s=p.read_text(encoding='utf-8');s+='''
Explicit [function binding propagation](docs/propagated-delete.md#explicit-function-binding-policy)
adds single plain const/let/var arrow or anonymous-function bindings under v3.
Original entity/evidence ranges, mandatory provider proofs and guarded application
remain; defaultv1 and explicitv2 keep their named-function module boundary.
''';p.write_text(s,encoding='utf-8')
print('Function binding policy contract documented')
