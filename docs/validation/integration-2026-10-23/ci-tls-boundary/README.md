# Cargo TLS test boundary and current explicit VS Code host

Code Intelligence native 0.5.0 / retained TypeScript 0.3.0, 2026-10-23.
Qualified code commit: c2f872d01440cb187a18f9699a0f367f7f173114. Its three CI/helper/test blobs equal the
qualified4 working bytes on checkout 527d692d, including the then uncommitted
case-policy fix. The cumulative change base is 2095792b.
Rust, TypeScript, extension product code, original native tests/ignores/deadlines
and build profiles are unchanged. The original two Python helper tests also
retain their exact AST.

## Result and scope

- Python: 24 passed, including a real dependency-free Cargo fixture; typecheck passed.
- npm: 260 passed, zero failures, 3 existing skips.
- Original Google HTTP CLI harness: injected TLS gives 1 pass/3 failures;
  the executable boundary gives 4 passed, zero failures.
- Original normal library harness: 578 passed, zero failures, 7 existing ignores;
  deliberate inner ambient-TLS refusal remains tested. No Cargo compile or native
  test changes were needed for these existing programs. Counts overlap historical
  qualifications and must not be added to the full baseline.
- All 1,100 source/control inputs stay exact through qualification; every completed
  test/gate parent has an empty-Job record. Protected failed output paths remain
  metadata-only and unchanged. Primary CLI SHA256:
  8d66415f2af983ff2d8639f6dc50324f3b5d6722631401454897e52c90f7925e.

The explicit option uses Cargo's [array runner contract](https://doc.rust-lang.org/cargo/reference/config.html#targettriplerunner).
Cleanup removes only SSL_CERT_FILE, SSL_CERT_DIR and SSLKEYLOGFILE from each
child, leaving parent environment and literal argument data intact. Actual
subprocess failure statuses propagate. Configured runner/target policy, indirect
includes and oversized configuration refuse before runner replacement.
Legacy mode keeps the original Cargo argument vector. CI selects the explicit
mode for native tests and the already explicit rust-analyzer smoke test.

The previous [Ubuntu failure](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/39941557570/job/120373960482)
has seven library and three CLI ambient-TLS refusals (398/7/6 and 1/3/0).
Its exact TLS initializer is unproven; a real owned Cargo force-environment
fixture proves the post-parent-cleanup failure class. Hosted acceptance of the
new commit remains pending. Separate Windows Node 24 Graphify startup timeout
evidence and its single failed-job rerun request remain explicit, with no result
inferred and no deadline changes.

## Actual editor host and packaging limit

The existing actual-host test passes with explicit native runtime on official
Microsoft-signed VS Code 1.143.1, source 2095792b, in a private owned profile.
The original extension/test bytes are unchanged. All four assertions pass:
unsaved source binding, ignored workspace executable, protected open-buffer
exclusion and saved source preservation. Code.exe SHA256:
325fb30ba28d17fa6ba4df58043727b2469aca9d641793a65e413721b2110ec6.
This is a runtime contract test, not full UI/accessibility certification.
The official archive import validates every extracted member and its bounded path.
Private profile logs, fixture homes and application/binary archives are excluded.

Canonical combined VSIX and release remain open. The existing debug binary exceeds
the unmodified per-entry packaging budget. A normal locked offline release build
starts but is deliberately stopped on critical disk headroom; owned descendants
end empty, source stays exact and no release binary is accepted. Storage phases
select only proven owned successful outputs for file-only lossless compression,
preserving logical sizes/hashes, deleting nothing. The initial largest-first
attempt stops before mutation on low space; the finite ascending continuation
records 32 successful exact-file operations. Independent free-space movement is
not attributed to those operations. No system setting/profile/privilege change
or protected failed-artifact access is made.

## Preserved failures and audit recovery

Initial RED reproduces the missing executable boundary with the existing helper.
The subsequent real Cargo astral-path case rejects JSON surrogate TOML escapes;
literal Unicode fixes the structured runner. A separate policy RED exposes
indirect environment runner/target policy; explicit inspection now refuses it.
Final qualified4 contains the accepted Python/typecheck/npm run after the final Windows environment-name case check. The native executable entrypoint and cleanup retain the exact AST from the completed original harness trials; only runner/target policy inspection and its cases change in the follow-up.

Native2 completes both CLI trials and all 578 library tests with empty Jobs,
but its generated result regex lost backslashes. That controller retains its
false completion value. The first read-only recovery then assumes six ignores;
actual stdout and the original full-baseline library group both show seven.
The second recovery parses completed stdout without regex and verifies the
original normal executable hash. Final-audit3 accepts the recovered measurement; final-audit4 verifies the case-policy follow-up and exact executable/cleanup AST.
Failed audits and original records stay intact; no passed library test is
repeated to repair metadata.

## Archive and documentation

[raw.zip](raw.zip) contains 215 independently reopened members.
Every member length/SHA256 matches [index.json](index.json).
ZIP SHA256 f7b40b212f1ff68d0cb7756dc1270801f4c6e66d2e3f2e640083c5fde55fd683; 1,127,221 bytes.
Controllers, stdout/stderr, source hashes, failed attempts and recovery records
are retained; compiled programs, user profiles, credentials and cache binaries
are excluded.

Only STATUS, REMAINING_WORK and one additive reconciliation checkpoint follow
qualification. [post-review-docs.json](post-review-docs.json) records those exact
three changed frozen inputs. No new product test is claimed after docs. Removing
the new JSON key leaves the entire original reconciliation deep-equal. Original
353 ledger/summary and historical Windows baseline remain unchanged. Full STATUS,
Windows symlink prerequisites, 77 runtime reviews, broader format/client/quality,
approved live Google export, canonical VSIX, hosted checks, main and release
acceptance remain open.
