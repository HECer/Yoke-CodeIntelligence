# App-bound history integration evidence

Checked source: `8fa410f380f384cb736babd7a3814758ec71be56`, based on integration `a6bbe2c`.
The locally preserved WIP ancestors are development history, not independent
acceptance. The checked source adds signed original-plan bindings, authenticated
worker/Git replay, exact queue/native fences, durable bounded execution from the
worker CLI, existing-only read-only management/status and typed retention.
Production deployment, receiver-side independent source verification/reasoning,
full upstream equivalence and whole-platform acceptance remain open.

| Final check | Actual result |
| --- | --- |
| 13 ordinary Cargo test targets | 287 passed, 0 failed, 6 default ignored |
| Unchanged digest-history-job target, explicit test profile | 13 passed, 0 failed |
| Combined scoped native targets | 300 passed, 0 failed, 6 default ignored, 14 groups |
| Format, build, all-target Clippy `-D warnings` | Passed on the same 758 native input hashes |
| Actual Windows CLI/Credential Manager gate | Passed; source and both binary hashes unchanged |
| Temporary owned signing credential | Removed, absence independently verified by outer controller |
| Retained checks, 759 unchanged runtime inputs | Python17, typecheck, TypeScript260 / 3 skipped pass |

The digest-history-job target used `cargo rustc --profile test` and only an
additional output suffix (`-C extra-filename=-r8-acquisition`), followed by its
normal native test harness. Windows retained the crashed MSVC linker PID22940
and denied termination; the original output path remained locked. No test source,
assertion, permission gate or recovery rule was changed for this execution.
The standard Cargo-path failure is retained, and this does not establish a
whole-native or standard whole-platform pass. The default ignored counts are
preserved even though one separately selected CLI gate was executed explicitly.

The tests exercise actual local Git objects and a host-owned loopback App
transport, not a configured production App. They cover closed input and permission
ordering; exact CRLF/Unicode source; source kind refusal; idempotent signed binding;
current versus historical selection; stale and future initial revisions; consistent
edits to both saved plans; key rotation and current grant denial; missing sidecars
and native jobs; output/SQL rollback; restart/offline read-only status; typed retention;
and worker replay without repeating HTTP or adopting another revision. The real
CLI gate includes lost stdout followed by explicit status/recovery. It owns no
real account secret: its temporary public fixture key is created/deleted outside
the potentially terminated test process.

Earlier failures are retained: the initial nonexistent Snapshot field (zero groups),
incorrect fixture HTTP200 instead of201, empty/probe and inherited nonblocking
socket handling, a double-panic process abort, a test-only private-field compile
error, a nonexistent test-target name, and linker/cache failures. A deliberately
failing revision test first proved that an invalid initial history revision could
publish both history rows; the final source checks that fence inside creation's
transaction. Prior durable worker completion remains a separate phase.

The final native source manifests are identical across quality, ordinary targets,
the separately named target and the CLI gate. All final changed native source
bytes are archived; additional historic source variants are provided when they
were captured. `additional-source-coverage.json` explicitly identifies older
hash-only variants instead of inventing missing bytes. Completed-result totals
do not include a target that aborted before its summary. Raw logs/commands and
before/after manifests retain those distinctions.

`tracker/proof.json` proves that only the R8 development fields changed; inventory,
other rows and certification flags remain exact. The protected PostgreSQL fixture
retains its original 2,779 bytes and SHA-256. Cache cleanup removed only old inactive
owned executable/PDB cache files after recording their hashes; no source/evidence
was removed. One crashed test image was preserved under a separate cache filename.
The attempted linker-image relocation was refused before mutation because its
bytes could not be read. Historic recipes reference completed phase directories
and must not be blindly rerun. They carry no permission to deploy or publish.

`index.json` hashes every artifact in this directory except itself. PR metadata
shown here is the prior description; post-push remote verification is recorded
separately in the ignored local validation receipt. No release receipt is claimed.
