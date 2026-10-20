# Immutable compiler move repair checkpoint

Source commit: `275c321bd8baa25bdaac5828612ded5230c8fe76`.
Base integration: `99febf08db9016f4382c2425c80eedc6dcdd8630`.

The explicitly registered TypeScript 5.8.3 move worker now repairs direct named
selected-symbol reexports and generated import aliases that collide with target
names, including globals. Original declarations and shorthand property keys keep
their names. Rust independently reconstructs every repair from captured original
and current source anchors, then composes edits into original UTF-8 byte offsets.
Complete-result syntax, binding, import/export, source and result checks still
precede the ordinary permission, stale-source, verification, approval and recovery
gates. No project plugins, model calls or project code run during analysis.

Repair composition is `original_byte_pieces_v1`. Admission retains the shared
2,000 original-plus-repair operation limit, 256 repair stages, 4 MB per changed
file and 16 MB aggregate output limits. No-repair operations retain their original
list, order and report shape. Object shorthand property definitions and value
references have separate anchors; coherent forged property changes are rejected.

## Recorded checks

| Check | Actual result |
| --- | --- |
| Final library plus twelve scoped integration groups | 653 passed, 0 failed, 9 existing ignored |
| Additional Rust cases included above | Six integration and two unit cases passed |
| Updated existing integrations | Two passed, including original and repaired CLI plans |
| Import forms / reexport styles | Four / three scenarios passed |
| Independent UTF-8 edit composition | 128 sequences of eight edits passed |
| Independent tsc, approval, apply and conditional recovery | Passed for both CLI scenarios |
| Format, native build, all-target Clippy `-D warnings` | Passed |
| Python contracts | 17 passed |
| TypeScript typecheck | Passed |
| TypeScript tests | 260 passed, 0 failed, 3 existing skipped |

The final native run kept 798 input hashes unchanged. Retained checks kept 792
input hashes unchanged. Development runs are not added to the final totals.
Ignored capacity, Windows-path, live and parent-worker cases remain ignored.
Adversarial cases cover coherent result rehashing, original export ownership,
free-reference ownership, fresh names, duplicate references, unknown protocol
fields, property keys, UTF-8 boundaries, budgets and read-only consumers.

A separate rerun of the same owned Array/Promise no-repair fixture retains exact
files, diff, result snapshot, intent, source bindings, compiler context, standard
library hashes and effect digest, without adding repair metadata. Fresh plan
timestamps produce different plan IDs under the unchanged identity formula.
This probe is additional evidence and is not included in the Rust test count.
Its fixture, runtime inputs and original sources remained unchanged.

## Preserved attempts and remaining scope

The archive retains the initial private-import failure, its repaired successful
run, and a Clippy collapsible-if failure followed by the passing final run. An
exploratory worker-only probe can produce diagnostic-free output that binds a
shorthand value to the wrong target name; it is not native admission evidence.
The product's native binding gate rejects that case before plan publication.
The first legacy CLI probe also records a helper lookup of a nonexistent
`plan.edits` field after successful CLI preparation; the corrected probe compares
the actual files and diff. Failed attempts remain indexed with their logs.

Missing shorthand-only dependency import synthesis remains open. These repairs
do not certify every import/reexport variant, destination, context or language.
Classic function-body inline, runtime and upstream acceptance, other R1-R9 work,
and full Windows/current-head hosted acceptance remain incomplete. Previously
observed OS1314 symlink-privilege and macOS PDF/runtime/timing failures remain
unresolved. No whole-native or other-platform rerun, main merge or release is
claimed.

The source/retained union contains 909 materialized paths: 667 exact byte matches
and 242 CRLF-only variants in the integration checkout. No runtime rerun there
is claimed. Qualification separately compares unchanged analyzer/model/LSP,
own_v1/change/session/inline sources and the protected 2,779-byte PostgreSQL
fixture against the base. Tracker proof preserves all decoded values and key
order except R1's missingBehavior, suggestedScope and new
compilerMoveRepairsDevelopment field.

[index.json](index.json) retains sizes and SHA-256 hashes for 96 raw artifacts,
including source variants, runners, passing and failing logs, inventories,
qualification, tool pins and materialization proof. Index SHA-256:
`1f9621d050bfd4004643fb1002fe7cd058111c3a7ac9464ef41388721bd76cd0`.
This explanatory README and the index itself are outside the raw artifact list.
