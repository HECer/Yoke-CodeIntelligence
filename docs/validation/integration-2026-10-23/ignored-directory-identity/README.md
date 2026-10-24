# Windows physical ignored-directory identity — 2026-10-23

Source `55c1ab70dcfa6b73e68011a3be1c1ab1151dfb4f` adds the explicit host-only
`ignoredDirectoryIdentity: windows-physical-v1` policy. It resolves selected
existing regular Windows directory names, records that effective policy and
compares stored names literally. Current and captured write admission separately
resolve real candidate/selected paths, including aliases and case-only renames.
No runtime/model/source-write authority follows from the capture setting.
The [contract](../../../../native/docs/ignored-source-capture.md#explicit-windows-physical-directory-identity)
describes unsupported/missing-directory and checked-plan/live-root boundaries.

The [index](index.json) pins 1,078 frozen inputs, seven product Git blobs,
CLI images and all 296 [archived raw members](raw.zip). The archive was
independently reopened; each member's byte count and SHA256 were verified.
Archive SHA256: `083bd8bc9fff4369275da681f00ed7fe2f7c2969521656ec3a961681a023b5e1` (737,130 bytes).
Fixture homes, preserved executable images and unrelated/personal data are not
copied into this archive.

## Final qualification

| Check | Result |
| --- | --- |
| Full library | 574 passed, 7 existing ignored |
| New identity integration target | 15 passed, 0 ignored |
| Other 17 scoped groups | 114 passed, 2 existing ignored |
| Total 19 Rust groups | 703 passed, 0 failed, 9 existing ignored |
| All-target Clippy, warnings denied; native locked build | passed |
| Typecheck; npm | passed; npm 260 passed, 3 existing skips |
| Formatting | all 349 actual metadata roots pass; fmt-all OS206 retained |

`capture-unicode-qualified2` qualifies the final policy and exact source manifest.
The two analyzer8 fixture ignores are existing corpus/prerequisite gates; none
are newly introduced. The protected capture_options executable/PDB is neither
run nor read/hashed/deleted; that integration target is not selected. All-target
Clippy checks metadata without replacing that executable. No original symlink
fixture, process privilege or Developer Mode/system setting changes.
The current CLI SHA256 is `b386daa7cc05cabbddc36cf41ac020d29bf1cecd30db05c64f650e510a3dbf66`.

The related edited `capture.rs` and `store_read_only.rs` files were normalized
from CRLF to LF before these checks. Their exact qualified bytes are committed;
this accounts for the large raw Git line diff. Ignoring end-of-line whitespace
shows 862 added/2 removed code-and-test lines across the seven product files.
Analyzed source samples and their original CRLF/Unicode byte anchors are preserved.

## Genuine counterexamples and retained failures

The byte-identical published reference
`558409a2aa9151022d0a8fb7e1bee61a686a1460d42c7b39046952e757a5969c`
reproduces the original issue in `capture-unicode-preflight1`: actual NTFS `ß`
and `SS` directories have distinct physical identities. Default capture permits
a guarded `SS/plain.rs` plan. Selecting ignored directory `ß` wrongly marks that
ordinary sibling read-only and refuses the same effect. No source is applied;
refusal SQL and original bytes remain exact. Those legacy absent-field semantics
remain historical rather than being silently reinterpreted.

The first new test fails 0/1 because the old production has no explicit identity
selection. Initial prototype tests record 6 passes/1 incorrect overlay assertion:
all virtual editor overlays have an independent read-only boundary, which is
preserved. The corrected test checks that boundary and original stored policy.

An initial `literal-ascii-v1` WIP passes its selected tests and broad gate, then
actual prepublication alias review finds a real additional defect. Preserved
image `1040aeab7b952708769428f1c97b495c3d9daa8f6d322e0291a6b0371b80fe11`
refuses a selected physical `é` path but wrongly permits prepare when the same
physical directory is selected as `É`. `capture-unicode-physical-alias-probe1`
records the two actual outcomes and unchanged source. A same-physical-capture
identity regression fails 0/1 against that WIP. The final policy normalizes
physical names, uses an independent tagged identity and rejects that unpublished
intermediate tag. No earlier raw gate/failure is rewritten or presented as final
qualification. The original and WIP reference images remain in their owned local
fixtures, outside this archive.

The final tests cover distinct Unicode names, same-physical Unicode and DOS
aliases, case-only renames, protected physical aliases, stale source plans,
current host restrictions, missing/non-directory selectors, source/Store binding,
fresh request deadlines, immutable source/old policy/proof bytes, overlays,
retention, copied collection replay and forged policy mutation. All 15 pass.

## Actual final native CLI and recovery

`capture-unicode-native-live1` performs 23 owned process calls over a Unicode,
apostrophe and CRLF fixture. Published/current default and legacy capture IDs
are exact. Older snapshot reads still succeed. The older executable explicitly
refuses the new policy. Old snapshot, graph and control rows remain exact while
new policy rows are added. Legacy snapshots retain their old scope even when the
current host selects the new policy.

New physical `é`/`É` selections have the same captured identity. Distinct `ß`/`SS`
names remain separate. A current physical alias blocks an older writable source
snapshot before publication. An actual DOS8.3 candidate alias is refused with
exact SQL. An unrelated ordinary sibling can produce a reviewed plan.

The host explicitly approves that owned plan and selects trusted host Python
for a byte-check-only verification in its copied result. This is a deliberate
validation command, not automatic analyzed project-code execution or a default
runtime. A newly restricted current host selection refuses even the approved,
verified saved plan with exact SQL/source. The allowed setting applies only the
reviewed bytes. Journal recovery restores every original source byte; no approval,
verification, write or recovery gate is bypassed. All original source/control
rows remain exact.

An explicitly created owned junction targets another directory inside the owned
fixture but outside the selected source. Both capture and current-write selection
refuse it with exact SQL; target bytes are unchanged and no outside canary blob
is published. The temporary fixture is preserved. No privileged symbolic-link
creation or global filesystem setting is used. All 23 process Jobs end empty.
No real model calls occur.

This is scoped Windows development acceptance. Broader discovery/nested
repository lifecycle, all filesystems/aliases/platforms, broader offline checked
plan portability, all 77 runtimes, original 353/R1–R9, whole Windows, main merge,
packaging and release remain open. The [post-review report](post-review-docs.json)
identifies documentation-only changes after the final freeze; product Git blobs
remain exact.
