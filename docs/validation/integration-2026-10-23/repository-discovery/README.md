# Bounded repository marker discovery - 2026-10-23

Source `7cad977573625138314394f63f29e4a5b1a5fa9e` adds a Store-free host CLI/library search over one
explicit absolute root. Candidates distinguish regular `.git` directories and
opaque files, retain their nearest marked ancestor and support literal exact
selection. Every result remains incomplete metadata with no registration or source
grant. The [contract](../../../../native/docs/repository-discovery.md) describes
budgets, omission reasons, marker limitations and independent host authority.

The [index](index.json) pins 1,082 frozen inputs, six product Git blobs, the actual
CLI and all 145 members of [raw.zip](raw.zip). Each ZIP member was
independently reopened and its byte count and SHA256 checked. Archive SHA256:
`887e4d4ffff6a235baa6e5beffe2f4d256513bf642ee37985c15c0f1a683cb15` (197,835 bytes). External fixture
homes and executable images are preserved locally and are not copied here.

## Final qualification

| Check | Result |
| --- | --- |
| Full library | 574 passed, 7 existing ignored |
| New discovery library integration / actual CLI targets | 8 / 2 passed |
| Existing project/startup/physical-directory/read-only targets | 6 / 8 / 15 / 7 passed |
| Total seven Rust groups | 620 passed, 0 failed, 7 existing ignored |
| All-target Clippy with warnings denied; locked native build | passed |
| Typecheck; npm | passed; npm 260 passed, 3 existing skips |
| Formatting | all 351 metadata roots pass; fmt-all OS206 retained |

`repository-discovery-qualified1` is the final frozen qualification. No new
skip, system/privilege change or original OS1314 fixture change is introduced.
The protected failed capture_options executable/PDB is not run, read, hashed,
deleted or replaced; its integration target is not selected. All-target Clippy
uses metadata checks. All owned Jobs are empty. CLI SHA256:
`286f2aadd70b74eaa8593ec14251f708ca98fcb870cbbaf62f815f33e5cac935`.

## Actual Windows trial and retained failed attempts

`repository-discovery-live1` fails during controller fixture setup before any
owned process starts: PowerShell's pipe encoding replaces the intended Unicode
path characters with `?`, causing WinError123. The controller and observed
preparation-failure record are retained. The corrected controller uses explicit
Unicode escapes; no product fix or repeat qualification is needed.

`repository-discovery-live2` completes eleven owned setup/compatibility calls:
real template-free Git init for a parent and nested repository, an inert reviewed
file commit with explicit empty hooks, a real linked worktree, two owned junctions,
and old/new explicit index calls. The discovery implementation never runs Git.
The warm index data and original snapshot match exactly. An overstrict full
envelope assertion then fails: duration6 versus12 changes serialized size1425
versus1426 and estimated tokens475 versus476. This is a test-comparison failure;
the initial result's finally-written success field is not evidence of a pass.
`failed-result-audit.json` explicitly records that limitation.

`repository-discovery-live3/prior-default-index-audit.json` independently reuses
those same stored raw replies, verifies each original compact UTF8 byte count and
ceil(bytes/3) estimate, and compares all remaining fields except request ID and
duration. It makes zero new index/model/client calls. The final sixteen owned
calls then verify published-command refusal, deterministic real nested/worktree
metadata, exact selection, zero-depth omissions and ten source/provider/path/
budget/deadline refusals. A linked search root refuses. Both ordinary-directory
and `.git`-marker junctions are omitted without traversing the outside canary.

After all discovery calls, the entire owned fixture's regular-file hashes and
directory/reparse membership match the pre-discovery baseline, including the
previously initialized compatibility Store. No source, configuration, registration
or original repository state changes. Twenty-seven total owned calls have empty
Jobs. Code review establishes that discovery reads metadata only; this is not
an OS access-monitoring or race-isolation certificate.

## Remaining acceptance

The original 353-entry inventory, assessments and summaries remain untouched.
Discovery does not establish repository validity, initialized submodule state,
persisted nested capture boundaries/lifecycle, automatic activation, all-runtime
behavior or complete upstream equivalence. Unix-only symlink tests were not run
on this Windows host. The ten original OS1314 fixtures and current-head whole
Windows/main/release acceptance remain open. Original source/capture semantics,
project registration authority and default model/execution behavior are unchanged.
