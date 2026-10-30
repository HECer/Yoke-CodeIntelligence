# Immutable compiler Move to file checkpoint

Source commit: `b1da9f70a57bbca1f1573b9b54d58beca517c63c`.
Base integration: `d36a6d0459e0b38352a04be30ce1f284e63ac768`.

The explicit `moveCompiler` profile selects trusted Node and TypeScript5.8.3
tools for actual Move to file actions over immutable TS/TSX/JS/JSX modules.
Native byte/syntax/binding/import-export/result admission precedes one ordinary
guarded plan. Original BOMs and compiler insertion order are preserved. Variables
and namespaces missing from the original graph carry explicit AST provenance and
null entityId; no original graph entity, producer or identity changes.

## Current checks

| Check | Actual result |
| --- | --- |
| Final library plus 12 scoped integration groups | 645 passed, 0 failed, 9 existing ignored |
| New Rust cases, included above | 13 integration + 3 unit passed |
| Declaration/analyzer matrix | 14 scenarios passed |
| Encoding/language matrix | 4 scenarios passed |
| JSX component matrix | 2 scenarios passed |
| Additional Array/Promise CLI probe | Passed; separate from Rust test counts |
| Independent copied-result tsc, approval/apply/conditional recovery | Passed in actual CLI integration |
| Format, native build, all-target Clippy `-D warnings` | Passed |
| Python contracts | 17 passed |
| TypeScript typecheck | Passed |
| TypeScript tests | 260 passed, 0 failed, 3 existing skipped |

The final native invocation kept **795 input hashes unchanged**. The retained
checks kept **789 hashes unchanged**. The additional CLI probe kept all final
runtime inputs and its original fixture sources unchanged, checked 13 static
bindings and ran independent tsc without applying the plan. Native binary and
selected tool hashes are retained. Repeated development phases are not added to
the final totals. Ignored entries include separately qualified large-capacity,
Windows path and live/parent-worker fixtures; none count as passed.

Owned real cases cover private free dependencies, ordered same-offset compiler
insertions, comments, namespace consumers, interfaces/types/enums/namespaces,
function/scalar bindings, JSX props/intrinsics, Unicode/CRLF/BOM, imported aliases,
unrelated reexports, inert plugins, session/facade/CLI routing and conditional
recovery preserving foreign writes. Adversarial native checks reject coherent
rehashed body changes, new unrelated exports, omitted import names, bad operation
order, UTF-8 boundaries, overlaps, paths, budgets and unknown fields. Read-only
consumers, ambiguous contexts and changed compiler bindings refuse publication.

## Preserved attempts and limits

All failed phases remain indexed with their original frozen sources and logs:
an integration-test API typo; graph-absent namespace/BOM admission gaps corrected
in source; an incorrectly named ambiguity fixture; unsafe compiler output; and
the first final run's four Clippy findings. The final run includes the subsequent
clarity fixes and one-time compilation of the fixed library filename regex.

Actual TypeScript5.8.3 leaves selected-symbol reexports unrepaired and may add an
import alias colliding with a target declaration. Real tests require both outputs
to fail after-diagnostics before plan publication. Independent repairs remain R1
work. The separate exploratory alias probe uses its explicitly recorded compiler
installation/hash; it is not substituted for the production worker's tests.

Results retain complete=false. Runtime/evaluation-order equivalence, dynamic or
omitted consumers, new resources, symbol-parent destinations, other shapes and
languages, classic function-body inline and full upstream acceptance remain open.
The existing Windows symlink-privilege failure and separately observed macOS
PDF/runtime/timing failures were not repaired by this checkpoint. No whole-native,
other-platform, main-merge or release certification is claimed.

The source/retained union has 906 materialized paths: 668 exact byte matches and
238 CRLF-only variants in the integration checkout. No runtime rerun there is
claimed. Original analyzer/model/LSP/own_v1/change/facade sources and the protected
2779-byte PostgreSQL fixture are independently compared against the base. Tracker
proof preserves existing key order and all decoded data except R1's
missingBehavior, suggestedScope and added compilerMoveDevelopment fields.

[index.json](index.json) records the sizes and SHA-256 hashes of **178 raw
artifacts**, including source variants, runners, successful/failed logs, native
and retained inventories, the additional CLI probe, tool pins and materialization
proof. This explanatory README and the index itself are outside that raw index.
