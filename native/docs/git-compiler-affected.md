# Explicit compiler-bound Git affected analysis

`git compiler-affected` reads the checked base and head captures of an existing
Git workflow report with an explicitly trusted host TypeScript 5.8.3 compiler.
It produces a separate ephemeral JSON result. The original report,
`captured_hierarchy_v1` query, source graphs and compiler analyze/build artifacts
retain their own identities and behavior. This query runs no Git command,
fetch, model, analyzed module or project build and publishes no artifact/record.

```sh
yoke --root /project --timeout 60000 --max-bytes 4000000 git compiler-affected REPORT_ID --node /host/node --typescript /host/typescript/lib/typescript.js --trust-compiler
```

On Windows use absolute paths to `node.exe` and `typescript.js`. Both paths are
explicit host selections; Node must be outside the analyzed workspace. Trust
permits executing the selected compiler under the host account and is not an OS
sandbox. Only captured files and the selected compiler's standard libraries are
visible to its virtual source host. Repository plugins and project modules are
not imported. Ordinary compiler diagnostics remain separately inspectable;
this query refuses contexts with errors or without usable captured programs.

The root must already have complete initialized source state. Snapshot, capture
configuration, ignored-directory, analyzer, language-server/trust and write
overrides are refused before opening source state. There is no options-file,
format or provider flag. `--max-depth` defaults to 8 and accepts 0 through 32.
The global timeout accepts 10 through 600000 milliseconds. The global output
envelope accepts 1024 through 4000000 bytes and defaults to 32000. Increase it
for larger reports. It includes the final LF and participates in result identity.
The complete compact JSON plus one LF is admitted before output; insufficient
space refuses with no partial stdout or spill artifact.

## Evidence and identity

Native admission replays the original report, snapshots, graphs, captured scope,
source hashes and UTF-8 spans. The Store read transaction ends before compiler
execution. A new checked read view verifies the original pins and changed rows
again afterward. The selected Node executable and compiler module have checked
hashes before and after both observations; the complete joined worker has its
own hash. Standard-library hashes, compiler options, project files and context
identities are retained separately per side and configuration. Capture omissions,
capture consistency and compiler limitations stay visible.

Actual checker declarations supply `extends`, `implements`, `declaresMember`,
`overrides` and `implementsMember`. Aliased heritage and computed names resolve
through the checker. Private lexical slots do not become overrides. Static
overrides stay distinct from instance overrides. Source overload signatures
retain separate `signatureImplementation` links to their actual function/method
bodies. This is trusted compiler evidence, not independent compiler attestation.

`calls` retains the checker-resolved captured signature declaration. Its explicit
`invocationKind` distinguishes `call`, `new`, `super` and `taggedTemplate`.
Overloaded functions/methods and constructors retain source-signature-to-body
links; constructor bodies are bound through the actual class constructor symbol
and their original class ownership is checked. All proof quotes use the original
invocation bytes, including tagged literals, Unicode and conditional calls.

`constructResult` separately binds a new-expression's static result type to its
captured class declarations. A union can retain more than one result class.
This is not the identity of the actual runtime constructor value. In particular,
`new Derived()` can resolve a constructor declared in Base while its result type
is Derived. An implicit constructor has no declaration span: the result class
and explicit missing-signature coverage remain, without inventing a `calls` edge.

For a dynamically selected constructor value, `possibleConstruct` enumerates
captured class constructor types assignable to the static receiver type. It binds
their actual captured constructor implementation body when available, including
inherited bodies; otherwise its target is the candidate class value with explicit
missing-body/declaration coverage. Candidate overloads sharing one implementation
do not create invented alternative overload selections. The original resolved
signature and candidate owner remain separate. Private/abstract constructors and
incompatible constructor types must satisfy the checker's actual assignability.

`possibleConstructValue` independently targets each compatible candidate class
value, even when its constructor body is inherited from another class. Incoming
class seeds follow this role as well as the distinct body role. Consequently a
class-only field change remains connected to a dynamic constructor caller without
pretending that the inherited body belongs to the derived class. Repeated sites
and different candidate owners sharing one body keep separate proofs. This is
candidate impact, not proof of initializer execution or a general field-effects
analysis; unsupported field/accessor/implicit execution observations remain open.

`possibleDispatch` separately compares captured declared class instance types,
captured class constructor types and source-observed generic instance types with
the static receiver type. Its basis distinguishes `compiler_type_assignability`,
`compiler_constructor_type_assignability` and
`compiler_captured_instance_assignability`. A structural interface can be matched
by an instance method or a method on a constructor value. Generic constructor
values keep static-member checks; generic instance checks use the actual checker
types observed at captured type syntax, typed declaration names, call/new/tag
results, heritage and method receivers. They do not instantiate an unseen family
with guessed `any` or inferred type arguments chosen by the query.

Each context has a separate `instances` table. A witness retains its captured
class owner, exact original source anchor and syntax kind, observed/effective/
class type displays and actual argument displays/TypeScript flags. Alias uses and
separate source locations remain separate; displays are not semantic identities.
Union branches remain separate candidates, while an intersection keeps its whole
effective type for compatibility. Phantom type parameters and source-authored
`any`, `unknown` or symbolic parameters follow actual checker behavior and keep
their original flags. JSDoc-template classes use checker-declared parameters.
Unobserved instantiations remain explicit coverage, with `complete: false`.

Generic method proofs reference their exact `candidateInstance` witness and class
owner. Native checks the closed schema, source/UTF-8 bounds, owner/reference and
context bindings before mapping witness IDs. The invocation pairing checks still
apply. Distinct witness and direct-callee roles survive even when they share one
body. The optional field is omitted for ordinary declared/constructor candidates,
preserving those relation facts. Candidate methods and original graphs remain
separate from actual object creation, runtime reachability or execution.

Actual method bodies, original resolved signature, candidate class, conditional
flag and original call/tag quote stay in every proof. Each possible relation
must pair with the exact original resolved invocation's source, target, range,
kind and conditional flag. Constructor facts without a source signature require
the explicit matching missing-signature coverage entry.

Direct checker-bound class references (including imported aliases, qualified
names and erased parentheses/type assertions) retain direct invocation bindings;
the query does not turn `Base.staticMethod()` into a call to Derived. Variables,
parameters and expressions have finite type-based candidate enumeration. `super`
and private method calls retain their direct bindings. These are static compiler
observations and conservative type-compatible candidates, not data-flow/runtime
reachability or a closed-world execution guarantee. Class expressions, unsupported
receiver types, property/accessor effects, unobserved generic instantiations,
implicit execution phases and external bodies retain visible limitations.

Original changed declarations seed the incoming walk by exact native declaration
binding or explicitly labeled original-source containment. Containment is byte
ownership, not semantic equivalence. Incoming traversal follows calls, possible
method/constructor targets, construct-result classes and overload-body links. It preserves every parallel proof,
including repeated call sites, while avoiding repeated states for one seed.
Sides, project contexts and changed rows remain separate. Every source range is
checked against original captured UTF-8 bytes; live disk is not a replacement.
Unmapped originals and depth-boundary incoming counts are explicit.

## Bounds and limits

Each side admits at most 5000 original graph entities, 50000 observations and
100000 evidence rows. Compiler input is at most 10 MB. Per compiler context the
defaults/maxima are 5000 nodes, 20000 relations/coverage entries, 200000 pair
visits and 500000 AST nodes. Each context independently admits at most
`maxNodes` instance witnesses and at most `maxNodes` arguments per witness.
The native argument-visit bound is `maxPairVisits`; cumulative raw witness JSON
is at most 4000000 bytes and every type/argument display at most 4096 UTF-8 bytes.
Displays are never silently truncated, and virtual host paths are sanitized. Incoming traversal separately admits at most 20000
states, 20000 proof rows and 200000 seed/edge visits across all sides/contexts.
The library `compiler_affected::Options` can lower these bounds and select depth
through 32; capacity exhaustion refuses instead of silently truncating evidence.
Depth zero retains seeds with explicit boundaries and does not walk incoming
edges. Existing compiler process/diagnostic/project/output limits also apply.

The result schema is `git_compiler_affected_v1`; result and observation node/edge
identities are domain-separated and source/context bound. The fixed public CLI
errors are `git_compiler_affected_input_invalid`,
`git_compiler_affected_capacity_exceeded` and
`git_compiler_affected_deadline_exceeded`. They do not expose internal source,
SQL or host-path diagnostics. Library callers receive contextual errors.

`compilerBindingEstablished: true` records real trusted compiler observations.
`runtimeDispatchEstablished` and `complete` remain false. Config-only effects,
implicit execution phases, arbitrary dynamic receivers, other languages/configurations,
broader ownership/report semantics, remote freshness and upstream/full R5
equivalence remain open. Original candidate reports retain their own uncertainty.
No identity, review, merge, execution or write authority is granted.
