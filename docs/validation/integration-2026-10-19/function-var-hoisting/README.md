# Function-owned var hoisting evidence

Source `3d6983eb7c7926c46865b5b5a5170d2138b6f424` extends the optional `inlineCompiler` body action.
Native source reconstruction preserves original-position initializers, function
storage shared by loops, parameter-default environments, duplicate/escaped names,
destructuring, nested scopes and distinct catch assignment targets.

| Frozen scope | Actual result |
| --- | --- |
| guard-r3 on 807 unchanged native inputs | Seven body units; 109 integration passes / zero failures / two existing ignored, across eight targets |
| review-r1 on 807 unchanged inputs | Seven body units; two separately selected actual live session tests, zero failures or ignored |
| Final format/build/all-target Clippy with warnings denied | Passed |
| retained-r3 on 801 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final review | Python 17; all 807 native and 800 other retained inputs exact; only three allowed R1 tracker fields changed |

The scopes overlap and are not added. Between guard and review, two unintentionally
recoded test comment literals were restored to their original UTF-8 spelling,
with original array formatting. The production prefix and other 806 guard inputs
are exact. `fixture-repair/proof.json` checks the complete expected text; the
initial whitespace-only proof failure from rustfmt removing an optional trailing
comma is preserved. The CLI builds have different recorded hashes, and are not
claimed identical. No source or test code changed after the final review freeze.

Two added integration functions cover 21 var scenarios. Three added unit functions
exercise original pattern-role boundaries, decoded names, budgets and coherent
forged type/assignment/parameter/property witnesses. An existing native frame unit
adds 11 owned JavaScript scenarios. Four body CLI gates include two new var cases
and independent tsc, approval/application and foreign-preserving recovery.
The separate live Session cases execute 8 TS/JS fixture scenarios using explicitly
selected TypeScript 5.8.3/language-server 5.1.3, with compiler-only verification and
the existing independent gates. Preparation executes no analyzed source; runtime
comparisons execute only owned literal fixtures.

Original compiler observations remain raw. Only comparison IDs for independently
derived same-function var groups are canonicalized. Initializer resolution is a
separate original checker witness; a simple catch parameter can own the assignment
without owning the function hoist. Assignment-pattern property syntax may target
only its own exact checked source key. Arbitrary compiler type expressions are
not inserted; original annotations and fixed primitive hints retain admitted
types. Other inferred forms must pass projected diagnostics or visibly refuse.

Failed development phases remain indexed: actual loop-closure typing failures,
native destructuring-property correspondence, an invalid original TypeScript
function/var fixture, the test Debug compilation error and the runner preflight
that started no Cargo. Worker prototypes use synthetic snapshots and explicitly
claim no native admission or analyzed execution. Existing eval/finally protection
and variable-inline/move/session regression cases remain in the broad pass.

The protected PostgreSQL fixture is 2779 bytes and raw exact in both checkouts.
Primary materialization checks 923 paths: 668 raw
matches and 255 UTF-8 CRLF-only matches. No primary
checkout runtime rerun or complete native/Windows/current-head hosted/upstream
acceptance follows. Wider inferred types, expression/invocation/declaration/
language/consumer forms and full STATUS/R1-R9 work remain active. Existing OS1314,
macOS/runtime and production App qualifications remain separate. Main is unmerged
and no release was performed.

The [index](index.json) binds 225 raw artifacts. The complete raw runner,
sources, requests, logs, hashes, metadata and materialization records are under
`runs/`; immutable earlier checkpoints remain in their original directories.
