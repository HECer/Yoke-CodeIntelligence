# Native VSIX archive admission

Code Intelligence native 0.5.0 / retained TypeScript 0.3.0, 2026-10-23.
Qualified code 3c08fa3ffe669c4f642bf82972bea50d65b0706f; base df52798bbf799245397ffacb163dbca0490705d6. The committed packager
and tests equal the working bytes measured by qualified1 on 1,100 frozen inputs.
No Rust, TypeScript, extension production source, native fixture/ignore/deadline,
Cargo profile, system setting, privilege or Developer Mode changes.

## Qualification

- Python 35 passed, typecheck passed, npm 260 passed/zero failures/3 existing skips.
- The original three packaging test functions retain their exact AST.
- Final packager tests cover all four existing target identities and combined
  native/TypeScript layout, literal Unicode notice bytes, Unix executable mode,
  original output preservation and both stale-size/stale-checksum refusal.
- Adversarial tests cover portable paths/ADS/devices, exact and ASCII-case
  duplicates, file-parent conflicts, linked entries, per-entry/total/manifest
  limits before payload reads, central-directory limits before ZIP metadata
  allocation, duplicate/non-object/non-JSON manifests and binary tampering.
- All completed test/gate/CLI Jobs end empty. All source/control inputs remain
  exact through qualification; protected failed default outputs stay
  metadata-only and unchanged. The primary native CLI remains
  8d66415f2af983ff2d8639f6dc50324f3b5d6722631401454897e52c90f7925e.

The same final fourteen packager tests run against the exact published df52798b
packager in a private fixture. All three original tests still pass while the new
adversarial cases fail (21 failed subtests and 2 errors); the regression is not
inferred from implementation shape. Those failed original-process logs and
old-source/final-test hashes remain in published-red1.

## Actual CLI scope

The actual unmocked packager CLI uses current real extension/dist/worker/
TypeScript/license files with one explicitly synthetic native payload. Every
one of its 299 resulting archive members is reopened; non-generated contents
match exact source bytes. Platform identity is linux-x64, native package 0.5.0,
retained runtime 0.3.0. A separate unsafe archive fails with no output, and an
existing output remains byte-exact. Expected child statuses are 0, 1 and 1;
the controller's last observedExitCode is therefore 1 while the dedicated
actual-cli-result records the successful three-case contract.

The temporary archive is labelled synthetic-only/NOT-DELIVERABLE, its native
payload is never executed and the archive is not included here. This establishes
packaging behavior, not actual Linux hosting, a Windows native release, complete
license qualification or a canonical bundled VSIX. The previous actual explicit
VS Code 1.143.1 runtime-host result retains source2095792b and unchanged Rust/
extension bytes. Canonical executable VSIX and release remain open; a current
release build has no accepted artifact. No stale historical local release is
used as a substitute.

## Limits and preserved attempts

The original entry cap stays 128,000,000 bytes. Physical archive and aggregate
declared payload caps are each 256,000,000 bytes; manifest cap 1,000,000 bytes;
entry count 8,192; central-directory bytes 8,000,000. The current release checksum
and create-only output preconditions remain. See the
[contract](../../../../native/docs/vsix-packaging.md) for accepted format scope.
These are bounded package-format checks, not hard OS allocation isolation.

Initial RED attempts expose reads before unsafe metadata and ambiguous archive
acceptance. The raw backslash fixture is corrected to encode the literal ZIP
name on Windows instead of letting ZipInfo normalize it while writing. Final
published-red1 independently checks the final test bytes against old source.
Green2 records one newly added comment fixture rejected by Python's ZIP reader
because it contains an end-record marker; the ordinary portable comment fixture
is used for positive coverage. No broader ZIP-comment support is claimed.
Green3 and the final full qualification retain their actual source hashes.
Failed attempts and output records remain intact.

## Evidence and documentation

[raw.zip](raw.zip) contains 70 independently reopened members.
Every member length/SHA256 matches [index.json](index.json).
Archive SHA256 5f686ffa250dfd244007d837d10dbd4b516a960ae4d0a60eb87693dbc7ffcf3b; 496,270 bytes.
Only controllers, logs, hash maps and results are included; no native binaries,
synthetic VSIX, user profile or credential material is shipped.

STATUS, REMAINING_WORK, one additive reconciliation key and the new contract
follow qualification. [post-review-docs.json](post-review-docs.json) identifies
the three modified frozen inputs and new doc; no later product test is claimed.
Removing the new key leaves the entire prior reconciliation deep-equal. Original
353 ledger/summary values and the historical Windows baseline stay unchanged.
Full STATUS/R1-R9/77-runtime, Windows prerequisites, approved Google live export,
broader client/format/quality, current hosted checks, canonical VSIX, main and
release acceptance remain open.
