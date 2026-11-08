# Complete Windows native formatting gate

Code Intelligence native 0.5.0 / retained TypeScript 0.3.0, 2026-10-23.
Qualified code ced3f2d2867bb7c3eb973217f46855246a33adfe; base f7176df00c025bd119e9431108948e76e3b30504.

## Outcome

The Windows CI formatting gate now checks every Cargo target through short
check-only rustfmt invocations. The exact 361 canonical Cargo verbose roots,
independent offline locked metadata and helper JSON list agree. All 361 checks
pass with ordinary module recursion, target editions and configuration discovery.
The old aggregate command still fails OS206; its reconstructed command has
33,872 UTF-16 units. Unix CI retains cargo fmt --all --check.

Python 47 tests pass (12 new, all 35 original unchanged), typecheck passes and
npm reports 260 passed/zero failures/3 existing skips. All 1,106 frozen inputs
stay exact through qualification and all owned Jobs end empty. Native production,
original Rust tests/ignores/deadlines/Cargo profiles and the primary executable
are unchanged. Its SHA256 remains
aa391752ae06896894e0610e65f7a006ac8757dccf32eb19428882e5cf72f7c7.
No native rebuild or test rerun is claimed for this contributor-tool change.

## Behavioral evidence

Tests cover every target kind, per-target editions, recursive local dependencies,
cycles/deduplication, conflicting editions, missing/relative/malformed inputs,
empty results, metadata/manifest/root capacities and duplicate metadata keys.
Literal Unicode/metacharacter paths and host formatter selection remain one
argument each; nonzero formatter statuses survive and later roots still run.

An actual Cargo/rustfmt fixture includes different editions, inherited hard-tabs
configuration, a non-root module and a build-script canary. Both the helper and
canonical Cargo reject the malformed dependency; both accept the corrected
fixture. A separately malformed child module also fails. Source bytes remain
unchanged by checks, no target directory appears and the canary never executes.
All metadata is offline/locked; there is no network fallback or source build.

## Preserved attempts and scope

red1 records the initially absent helper and actual Cargo OS206 failure.
green1 retains two missing-file exception errors and one newly authored canary
formatting error; product errors and the fixture are corrected. green2 passes
Python and the complete format gate, then its controller fails a case-sensitive
SystemRoot dictionary lookup before starting npm. qualified1 runs all final
gates on the same final source map; audit1 independently checks exact canonical
target coverage, versions and source/executable preservation. Earlier failures
and original logs remain intact; they add no accepted counts.

Rustfmt 1.8.0-stable from toolchain 1.94.1 is the observed formatter. Runtime
binary hashes are measured during the final audit, not asserted as snapshots
from before the first format command. Limits and host trust are documented in
the [contract](../../../../native/docs/native-formatting.md). The tool is a
trusted contributor check, not a project-code execution grant or OS sandbox.

## Archive and documentation

[raw.zip](raw.zip) contains 61 reopened members, 350,526 bytes,
SHA256 ce27b7264ed5d465e0021f19daa0bf6f47ea1695a22ea0548db6f080947fa1b3. Each member length/SHA256 is checked against
[index.json](index.json). It includes raw controllers, logs, maps and results;
no executable, dependency cache or private host profile is included.

Only STATUS, REMAINING_WORK and one additive reconciliation key change after
qualification; the new contract and this evidence are documentation.
[post-review-docs.json](post-review-docs.json) records those exact hashes.
Removing the new reconciliation key leaves the entire prior object deep-equal,
including the original 353 inventory/73 unreconciled/77 unknown runtime values.
The Windows format gate passes; original OS1314 prerequisites, current whole
native/hosted acceptance, canonical bundled VSIX, release, broader R1-R9/client/
quality/approved Google/main requirements remain open. No system setting,
privilege, Developer Mode or artifact deletion is used.
