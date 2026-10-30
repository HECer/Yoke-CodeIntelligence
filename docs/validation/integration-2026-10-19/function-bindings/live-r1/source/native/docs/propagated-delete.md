# Captured function deletion propagation

Symbol-selected native `safeDelete` and its facade aliases accept
`propagate=true` for a bounded TypeScript/JavaScript ES function module.
The module must contain unique named function declarations, comments, empty
statements or expression statements, with at least one exported declaration.
Imports, variable/class/type declarations, ambient declarations and other
language or file selections remain outside this policy.

Every declaration must bind exact captured entity, name and declaration byte
ranges. The host-selected provider must return its exact definition and
bounded references including its declaration. Missing, empty, foreign,
truncated or differently bound proofs refuse before a plan is published.
Provider/source configuration and omissions must stay consistent across all
requests. Explicit force does not bypass these checks or expand the policy.
Reported positions inside the selected module must bind exact identifier-use
nodes. Comments, strings and declaration/parameter binding positions refuse
instead of seeding deletion of a previously unrelated unused declaration.
Only explicitly supported expression-use roles are admitted; unknown roles,
including unclassified parameter forms, refuse rather than defaulting to use.

Only provider-bound target uses whose direct call is an entire expression
statement contribute usage removals. Return values, assignments, callbacks,
constructors and other expression consumers refuse if their source survives.
References inside an already removed declaration or call statement need no
second overlapping edit. Parallel observations remain in the original graph;
identical physical statement removals are emitted once.

Dependency propagation follows references from removed source regions to the
declarations they use. It does not sweep unrelated unused functions. Private
candidate cycles can be removed together. A surviving reported or possible
reference keeps a candidate and its dependencies. Exported dependencies remain
protected as possible external entry points. Captured graph evidence,
unreported identifier occurrences and conservative literal-name matches can
prevent pruning; an unsupported surviving target use refuses the operation.

Admission is limited to 2,000 captured physical files, 16,000,000 source bytes
counted per file, a 256,000-byte selected module, sixteen declarations and
thirty-two provider requests. Each response allows at most 2,000 references;
the combined reported/candidate evidence budget is 4,000 rows. At most 128
distinct call statements are removed. AST and graph traversals have explicit
200,000-node/row bounds and retain the original deadline and cancellation.
Source reads use one captured scope. Physical captured originals are checked
before and after provider work.

One ordinary guarded plan contains all exact original-coordinate removals.
No project module is executed, no provider command is run and no model call is
added. Application retains explicit host permission, exact approval, executed
verification, stale-source preconditions and conditional recovery. Preparation
does not acquire a lease over future consumers; later application retains the
ordinary plan's source preconditions rather than repeating semantic queries.

The result records `captured_es_function_propagation_v1`, deleted declarations,
removed usages, retained candidates, proof counts, captured manifest identity
and provider/source context. It keeps `complete=false`,
`exhaustiveCompilerProof=false` and `remainingUsagesMayBreak=true`. Dynamic,
external, omitted or differently configured consumers remain unknown. This
finite native policy does not establish general propagated deletion or IDE
equivalence. See [symbol deletion boundaries](symbol-deletion.md) for ordinary
declaration deletion and [file usage checks](file-usage.md) for file selection.

## Explicit wrapped-call policy

Direct native semantic `safeDelete` additionally accepts
`propagationPolicy: "captured_es_function_propagation_v2"` with `propagate:true`
and a nonempty symbol `namePath`. Omission selects v1; explicitly selecting
`captured_es_function_propagation_v1` retains its original effects. This native
selector does not change the published facade schemas or accept a new facade
argument. Unknown values, nulls, nontext policies and policy selection without
symbol propagation refuse before indexing or starting a provider.

V2 can remove a plain identifier call wrapped in at most 32 combined parentheses,
`await` expressions and unary `void` expressions, such as `((removeMe()));`,
`await (removeMe());` or `void (await removeMe());`. The complete expression
statement must belong directly to the program or a statement block. Each wrapper
must contain exactly its one supported expression. The original complete
statement bytes are removed once, including nested calls, while adjacent
comments, BOM, UTF-8 and line endings remain. Dependency propagation uses the
same removed original regions and retains surviving/exported dependencies.

Value consumers, other unary operators, optional calls, parenthesized callees,
type arguments/assertions, non-null assertions, wrapper comments and other
unclassified children remain outside v2. A lone `if`, loop, label or switch-case
statement slot is not removable; an explicit block can retain its empty body.
The entire proposed source is reparsed before publishing a plan, under the same
256,000-byte source bound and original deadline/cancellation. Grammar acceptance
does not prove type correctness or runtime equivalence. V2 retains all original
provider/source proofs, capacities, guarded application and unknown-consumer
limits, reporting its distinct policy and `complete:false`.

The result must also retain every surviving named and anonymous syntax node's
kind, nesting, field role, order and mapped original byte range. Only fully
removed original subtrees are omitted from this comparison, with a shared
200,000 visited-node budget. This refuses unintended merges across automatic
semicolon boundaries without inserting punctuation. Syntax correspondence still
does not establish semantic or runtime equivalence.
The undelimited program container starts at physical byte zero for this
comparison; all child-node coordinates remain exact. Leading whitespace left
by removing the first declaration is retained even when the parser's program
node starts at the next surviving token.

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
