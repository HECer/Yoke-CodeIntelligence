# Body completion and call-specific inferred result types

Source `b700e6eb357f51c6a17584442e54449cacdf8912` extends the optional `inlineCompiler` classic-body action.
The original body block runs under fixed `void 0 === void 0`; normal completion
clears a cancelled return outside it, and only actual outer returns need a label.
Enabled unreachable-code/unused-label diagnostics remain active. Original/fixed
result assertions use a fixed intermediate unknown assertion for generated
undefined paths, including throwing and never-typed calls that actually return.

Other inferred results use fixed typeof syntax over a fresh value carrier. Its
initializer calls an original-parameter/body function carrier with already
evaluated fresh argument temporaries. Both initializers and the read remain under
`void 0 !== void 0`; neither function clone nor call executes. The var bindings
remain function/module scoped and initially undefined. JSDoc template/literal
instantiation, async/generator caller arguments and stateful argument order retain
checked types without reevaluating source expressions. Const/contextual literal results use a fixed readonly wrapper and indexed typeof;
ordinary contexts retain widening inference. Module declarations stay at their
original site; bounded original function-tag pieces preserve aliases when copied.
Generated const type tokens require exact parsed assertion positions. No arbitrary
compiler type text, global ReturnType helper or conditional type reconstruction
is inserted.

| Frozen scope | Actual result |
| --- | --- |
| guard-r4, 809 unchanged native inputs | Eleven body units; 113 integration passes / zero failures / two existing ignored, across eight targets |
| Final format/build/all-target Clippy with warnings denied | Passed |
| retained-r5, 803 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final checks | Python 17; all 809 native and 802 other retained inputs exact; only three allowed R1 tracker fields changed |

Scopes overlap and are not added. Two added integration functions cover 26 strict
completion/type cases, thirteen generic/documentation positives and one explicit local
type-shadow refusal. Two new unit functions cover leading/trailing comments, BOM,
Unicode/newlines, exact source copies, deadlines and bounds. Existing actual-host
admission tests now reject forged typeof queries, carrier argument maps, removed
documentation neutralization, guards, names and source/result hashes. Nine body
CLI cases include four new strict TS/JS cases, independent tsc, approval/application
and foreign-preserving recovery. Async runtime comparison awaits only owned actual
Promise observations with a one-second bound; runtime comparisons execute only
owned literal fixtures. Preparation executes no analyzed source.

Removing a documented definition emits a fixed empty statement at its original
declaration span, keeping comment/typedef bytes while preventing silent annotation
of neighboring functions. Native source-derived ownership excludes trailing docs,
including line breaks inside comments, and handles BOM explicitly. The compiler
checks type/value free bindings in JSDoc type expressions/template defaults at the
consumer scope; owned documentation declarations retain their copied scope and
fixed import-type qualifiers retain module meaning. Native original-byte frames,
fresh names, every reported copied binding and surviving caller types remain
checked. Raw observations and original graph/entity identities are not rewritten.

Development evidence is distinct and nonadditive. Initial probes pinpoint TS7027
at the synthetic reset and TS7028 at unused generated labels. An initial guard
prototype still produced TS2352 on never/undefined paths. Feature-r1 has 11 passes
and one invalid-original JSDoc fixture, corroborated as TS7006. Fixtures-r2 finds
one additional invalid JS-catch original (TS18046), corrected with instanceof;
fixtures-r3 selects that correction. Guard-r1 has nine unit passes and one actual
BOM ownership failure, subsequently fixed. Guard-r2 passes ten units and 112
integrations on its prior source freeze, before the final called-result design.
It is not a current-source result. Guard-r3 has 112 passes and one literal
widening failure (3 to number), with ten passing units. Initial always-boxed and
contextual-boxed probes still lose a typedef alias; the final original-tag piece
plan fixes the alias while retaining exact raw before/after type text.

The published-vs-current worker comparison then exposed loss of JSDoc generic
instantiation in the intermediate conditional-result design. The final call-specific
carrier supersedes it. Prototype async failure also localized orphaned documentation
reattachment; keep-definition probes isolate it and fixed empty statements preserve
neighbor/typedef types. Eighteen parse-only ownership cases and an actual owned CLI
red are retained. An initial inline local-typedef counterexample did not actually
shadow; the corrected newline-separated probe records original/consumer declaration
positions and the intended refusal. A runner preflight stops before any worker.
Synthetic probes explicitly claim no native admission or analyzed execution.
Earlier pass/fail counts are not added to final totals. The two ignored Session
tests were not selected in this run; earlier live results remain in their archive.

Primary materialization checks 925 paths: 668 raw
matches and 257 UTF-8 CRLF-only matches. The protected
PostgreSQL fixture is 2779 bytes and raw exact in both checkouts. No primary runtime
rerun, complete native/Windows/current-head hosted/upstream acceptance, main merge
or release is claimed. Further return/predicate/parameter/annotation contexts,
diagnostic combinations and wider expression/invocation/declaration/language/
consumer forms remain open, alongside STATUS/R1-R9. Existing OS1314/macOS/runtime
and production App qualifications remain separate.

The [index](index.json) binds 564 raw artifacts, preserving failed and
passing phases, workers, requests, outputs, available runners, logs, metadata and
materialization records. Earlier evidence is immutable.
