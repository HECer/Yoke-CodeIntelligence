# Explicit source-category report policy

The [checkpoint](checkpoint.json) selects 55 passing Rust tests across seven
targets, final format/all-target Clippy, typecheck and TypeScript 260 passed /
3 existing skips. [The index](index.json) binds exact copied raw bytes; this
explanatory README is outside that index. Runtime inputs remain unchanged after
final validation; subsequent edits are documentation.

`guard-r1` preserves 54 passing tests followed by a Boolean-expression lint.
`guard-r2` passes 55 tests after equivalent early-return filtering and adds a
source-bound percentile boundary and obvious-secret-label refusal. Its remaining
lint concerns only initialization of an invalid test configuration. `guard-r3`
rechecks that changed five-test target, format, all-target Clippy, typecheck and
the full retained suite successfully. Exact byte reconstruction proves the
initializer is the only r2-to-r3 source difference. Counts are not added twice.

The actual CLI control analyzes the same owned saved snapshot before/after the
implementation. Legacy JSON stdout, report and Markdown artifact IDs are exact.
The explicitly selected new policy has its own producer/artifact while keeping
the original graph, source bindings, evidence anchors and resolution counts.
The test with pre-existing fixed canonical golden hashes also passes. No model or
analyzed program is executed. Initial capture occurs only in the owned control;
the after-check explicitly pins the saved snapshot.

Declared projection categories are not source truth or confidence probabilities.
Unknown categories remain visible. Filtering changes report display and numerical
projection, with original direction and parallel observations retained. Selected
View replay rejects coherently altered stored policy metadata without rewriting
the earlier artifact or original graph.

Native Leiden/splitting/hub-reattachment behavior, broader upstream equivalence,
Windows symlink prerequisites, external live qualification and other R1-R9 work
remain open. This is not full Windows/current-head hosted acceptance or a release.
