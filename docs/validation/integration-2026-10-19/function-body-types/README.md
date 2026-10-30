# Source-derived inferred var types

Source `3f8fe565fd4fc88d9ca8266bbf605e698688e4a8` extends the optional `inlineCompiler` classic-body action.
Native reconstruction derives fresh type carriers from original declarations or
loop heads at their lexical sites. Fixed `typeof` syntax preserves admitted object,
array, readonly tuple, union and function storage types without inserting arbitrary
compiler type expressions. The fixed guard `void 0 !== void 0` keeps copied
initializers, iterable expressions and reads inert. The carrier var binding remains
function/module scoped; actual body storage is lexical and fresh per expansion.

| Frozen scope | Actual result |
| --- | --- |
| guard-r1, 809 unchanged native inputs | Nine body units; 111 integration passes / zero failures / two existing ignored, across eight targets |
| Final format/build/all-target Clippy with warnings denied | Passed |
| retained-r1, 803 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final checks | Python 17; all 809 native and 802 other retained inputs exact; only three allowed R1 tracker fields changed |

These scopes overlap and are not added. No source or test code changed after the
final guard freeze. The two ignored Session tests were not selected in this run;
their earlier actual live result remains at the preceding var-hoisting checkpoint.

Two added integration functions cover 22 inferred-storage scenarios and two cases
with `allowUnreachableCode:false`, `noUnusedLocals`, `noUnusedParameters` and
`noFallthroughCasesInSwitch` enabled. Coverage includes labels/continue, unbraced
controls, nested/adjacent insertions, actual once-only initializer/iterable calls,
destructuring/default/shorthand/prototype keys, JS checkJs, const assertions and
escaped UTF-8/CRLF input. Two new native unit functions and extended admission tests
cover malformed groups/names/deadlines, omitted destructured types, forged raw type
rows and a coherent forged true guard. Five actual body CLI gates include one new
inferred loop case, independent tsc, approval/application and foreign-preserving
recovery. Runtime comparisons execute only owned literal fixtures. Preparation
executes no analyzed source.

The final native and worker plans independently reconstruct exact source pieces,
quoted decoded property keys and ordered insertion/return frames. Copied member,
library and local bindings remain checked; raw compiler observations and graph IDs
are retained. Source sites, carrier names, label depth, bytes, edits, copies and
compiler observations share bounded budgets and refuse rather than truncate.

Development phases are distinct, nonadditive evidence. The initial worker probes
show actual implicit-any and const-token refusals; the worker green and owned VM
comparisons are synthetic-snapshot evidence without native admission. Feature-r1
has nine passes and one obsolete test assertion failure: a blanket ban on var also
rejected the intended cold carriers after actual native/runtime comparison passed.
Feature-r2 passes ten functions before the final strict-diagnostic additions.
Guard-options-r1 compares a literal false guard against the pure binary guard:
the nonreturning strict-option fixture passes only the latter; the returning-body
fixture still refuses under both. Its precise diagnostic cause is not localized.
The final guard passes all 11 body integration functions. Earlier compile/unit/
feature counts are not added to final totals. Targeted deletion of two verified
unused old build cache files is recorded separately; the CLI hash was unchanged.

Primary materialization checks 925 paths: 668 raw
matches and 257 UTF-8 CRLF-only matches. The protected
PostgreSQL fixture is 2779 bytes and raw exact in both checkouts. No primary runtime
rerun, complete native/Windows/current-head hosted/upstream acceptance, main merge
or release is claimed. Inferred return/parameter/annotation contexts, other strict
diagnostic combinations and wider expression/invocation/declaration/language/
consumer forms remain open, alongside the remaining STATUS/R1-R9 work. Existing
OS1314/macOS/runtime and production App qualifications remain separate.

The [index](index.json) binds 178 raw artifacts, including all passing and
failed development phases. Original runners, requests, logs, hashes, metadata and
materialization records are retained under `runs/`. Earlier evidence is immutable.
