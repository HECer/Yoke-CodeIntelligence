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

`calls` binds a call expression to its resolved captured signature declaration.
For eligible instance method receivers, `possibleDispatch` separately enumerates
captured non-generic class instance types assignable to that static receiver
type. The candidate class, original resolved target, type-assignability basis,
conditional flag and original call quote remain in each proof. Structural
compatibility can admit a class without an explicit implements clause; identical
spelling with an incompatible type does not suffice. This finite type-based
candidate relation does not establish runtime reachability or execution.
`super` and private calls retain their direct binding. Static possible-target
enumeration, generic class instantiations, unsupported receiver types, class
expressions, non-method members, constructors and tagged-template invocation
remain explicit coverage limitations. A literal `Base.staticMethod()` is not
invented as a runtime call to `Derived.staticMethod()`.

Original changed declarations seed the incoming walk by exact native declaration
binding or explicitly labeled original-source containment. Containment is byte
ownership, not semantic equivalence. Incoming traversal follows calls, possible
dispatch and overload-implementation links. It preserves every parallel proof,
including repeated call sites, while avoiding repeated states for one seed.
Sides, project contexts and changed rows remain separate. Every source range is
checked against original captured UTF-8 bytes; live disk is not a replacement.
Unmapped originals and depth-boundary incoming counts are explicit.

## Bounds and limits

Each side admits at most 5000 original graph entities, 50000 observations and
100000 evidence rows. Compiler input is at most 10 MB. Per compiler context the
defaults/maxima are 5000 nodes, 20000 relations/coverage entries, 200000 pair
visits and 500000 AST nodes. Incoming traversal separately admits at most 20000
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
implicit execution, arbitrary dynamic receivers, other languages/configurations,
broader ownership/report semantics, remote freshness and upstream/full R5
equivalence remain open. Original candidate reports retain their own uncertainty.
No identity, review, merge, execution or write authority is granted.
