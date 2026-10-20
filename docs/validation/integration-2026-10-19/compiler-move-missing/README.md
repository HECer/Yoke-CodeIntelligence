# Captured omitted compiler-move dependencies

Source commit: `fabc0971f91c9bf5b29c371050fce7cb4e8f79ae`.
Base integration: `ec8b89f56ed503e292a05fa608b19de6637b5f38`.

TypeScript 5.8.3 can omit a dependency used only as object shorthand, including
diagnostic-free substitution of a target variable. The explicitly registered
move worker now reconstructs original named/default/namespace imports and
source-owned top-level free dependencies. Source exports are added only when
required. Original declarations and public aliases retain their names.

Native admission independently checks original/current import or definition
anchors, captured free references, existing named export syntax, declaration
boundaries, relative captured-module candidates and final import targets.
Fresh aliases must avoid both original observations and current syntax, including
earlier repair steps. The first reference's destination is checked before pairing
its tree and text. Original-byte piece composition and shared operation/output
budgets remain. Final syntax/binding/API/source checks precede the unchanged
permission, stale-source, verification, approval, application and recovery gates.

Shorthand `__proto__` becomes `["__proto__"]: alias`, retaining an own data
property. Native correspondence distinguishes the prototype setter form
`__proto__: alias`. Three explicitly executed owned JavaScript fixtures check
actual own-property, value and prototype behavior through missing-import,
existing-import-alias and source-dependency repairs. Their captured consumers
also access the property. Preparation itself executes no analyzed source.

## Final checks

| Check | Actual result |
| --- | --- |
| Library plus twelve scoped integration groups | 659 passed, 0 failed, 9 existing ignored |
| Additional Rust integration cases included above | Six passed |
| Updated existing integration / unit cases | Two / two passed |
| Named/default/namespace/combined import scenarios | Twelve passed |
| Private/public/default/aliased source-dependency scenarios | Twelve passed |
| Actual CLI plus independent tsc, approval, apply and conditional recovery | Three scenarios passed |
| Explicit owned property execution oracles | Three passed |
| Format, native build, all-target Clippy `-D warnings` | Passed |
| Python contracts | 17 passed |
| TypeScript typecheck | Passed |
| TypeScript tests | 260 passed, 0 failed, 3 existing skipped |

The final native checks retain 799 input hashes unchanged. Retained checks retain
793 input hashes unchanged. Ignored capacity, Windows-path, live and parent-worker
fixtures are not counted as passed. Development runs are not added to totals.
The final scope contains all 25 compiler-move integrations, the library and the
listed existing session, inline, intent, semantic and file/directory move groups.

Adversarial cases include coherent result rehashing, missing/foreign export
insertions, wrong definitions and reference paths, duplicate references, reused
fresh names, module traversal/code injection, unknown fields and shared budgets.
The earlier UTF-8 composition and source/approval/recovery gates remain exercised.

A separate rerun of the same owned Array/Promise no-repair CLI fixture retains
exact files, diff, result snapshot, intent, source bindings, compiler context,
standard-library hashes and effect digest without adding repair metadata.
Fresh timestamped plan IDs differ under the unchanged identity formula. This is
additional evidence and is not added to the Rust test count. Fixture sources and
final runtime inputs remain unchanged.

## Preserved attempts and limits

`feature-r1` retains the directive fixture mismatch: the original source lacked
the target's `"use strict"` directive, so the existing gate correctly refused.
The fixture was corrected without weakening that gate. `feature-r2` passed
502 tests before the later source-dependency and property additions.
`targeted-r3` passed five units and 25 integrations before the final export/path/
freshness additions and extended export scenarios.

`final-r1` retains three arXiv state-I/O failures and a Clippy reference finding.
The exact same test binary subsequently passed all four arXiv journal tests.
The underlying I/O failure
was not localized, and no arXiv implementation or test was changed. The final
run includes those four tests and passes. The Clippy expression was corrected.
`final-r2` was intentionally terminated before completion to finish early
reference-path admission; its owned process tree was verified terminal before
editing, and no passing result is claimed. `final-r3` is the separate final run.

Exploratory worker probes retain synthetic request IDs and complete scripts.
The private-dependency probe instruments an earlier worker to show its incorrect
binding. These probes are not native admission evidence; actual captured-source
native and CLI tests provide that evidence. The previous PR description is
archived as historical data and is superseded by the current description.

Classic function-body inline, unsupported attributes and sibling bundles,
broader import/export/context/destination/language and consumer coverage,
evaluation order, general runtime equivalence and full Windows/upstream/current-
head hosted acceptance remain open. Existing OS1314 and macOS PDF/runtime/timing
failures remain unresolved. No whole-native or other-platform rerun, main merge
or release is claimed.

The source/retained union has 910 materialized paths: 667 exact byte matches and
243 CRLF-only variants in the integration checkout. No runtime rerun there is
claimed. Qualification compares unchanged analyzer/model/LSP/change/session/
inline/arXiv/dependency sources and the protected 2,779-byte PostgreSQL fixture
against the base. Tracker proof preserves existing key order and all decoded
data except R1's missingBehavior, suggestedScope and new
compilerMoveMissingImportsDevelopment field.

[index.json](index.json) records the sizes and SHA-256 hashes of 123 raw
artifacts, including source variants, successful/failed logs, the aborted-run
receipt, runners, inventories, qualification, tool pins and materialization.
Index SHA-256:
`1c13cef4d0b3e2e87364eca649e39d59e51fc4cb0c41869bf704a8e392bd33b2`.
This explanatory README and the index itself are outside the raw artifact list.
