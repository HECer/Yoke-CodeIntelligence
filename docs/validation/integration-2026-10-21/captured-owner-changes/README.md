# Captured owner changes across unchanged source files

The actual pre-change CLI fixture changes only CODEOWNERS from one authored
label to another. The existing owners query returns CODEOWNERS alone and omits
the unchanged Rust path. Its original raw result repeats exactly. At that point,
the owner-changes command refuses without changing any owned files. This is an
actual CLI availability/behavior witness. The six new integration functions ran
against the new implementation.

The new separate query compares all paths in both captured manifests through the
same checked parent transaction and finite rule evaluator. It verifies every
captured body, retains full original parent and rule/token byte evidence, and
distinguishes candidate, winning-evidence, added/removed and indeterminate cases.
Missing/unsupported rules remain explicit; no identity, permission or actual
review requirement is established. GitHub review requests use base-branch rules.

Six new integration functions and 65 existing Git regressions pass: 71 distinct
Rust cases, formatting and all-target Clippy with warnings denied. Coverage
includes Unicode/CRLF, duplicate tokens, unchanged source bodies, relocated rule
anchors, explicit empty lists, unrelated container-hash changes, all zero quotas,
tampered bodies, caller transactions/deadlines and offline/private CLI admission.
The exact CLI output boundary includes its final LF; a one-byte-short budget
refuses without stdout. Both binaries preserve original owner output bytes,
repeat the new result exactly and leave every owned source/Store/Git file intact.

The isolated worktree starts from 990 byte-exact primary inputs. Its initial
CRLF copy exposed 851 stat/filter-cache differences; refreshing only the index
produces zero staged content changes without touching working bytes. After
integration all 993 validated inputs match byte-exactly. Primary Cargo build
then freshly compiles both library and CLI from the primary manifest/source
paths; the artifact records have fresh:false. Primary formatting, typecheck and
TypeScript260/3 existing skips pass. Rust tests are not repeated on identical
inputs; an independent actual primary CLI control pins its own executable hash.
The complete new raw output is also byte-exact across the two binaries.

[checkpoint.json](checkpoint.json) records commands/results, source and binary
proofs. [raw-index.json](raw-index.json) indexes each decompressed member of
[raw-logs.zip](raw-logs.zip), verified against its exact original bytes. No raw
line endings or trailing whitespace are rewritten. Documentation follows the
source checks; no runtime input is changed during qualification.

The complete default baseline at bcf442b predates both new community and owner
change modes: 2,951 passed, ten original OS1314 setup failures and 53 ignored.
A complete native run at this source commit remains open, alongside wider
R5/R1-R9, compiler override/dispatch,
upstream/parser/platform/hosted acceptance, actual review/remote truth, main and
release remain open. No model or analyzed-source execution occurs in these CLI
controls. No Developer Mode, privilege or system setting is changed.
