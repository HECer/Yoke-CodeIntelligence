# Actual platform regressions and construction corrections

Evidence source: PR7 `b55064b9f1df8c2fa751d5fbf733428d8bad7293`, checkout merge
`1794d70f495cd2510499794998c88b9ccfce2605`,
[Rust run 40499831524](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40499831524).
This is a bounded evidence snapshot, not full platform or release acceptance.

[`artifacts.json`](artifacts.json) pins the 20 original ZIPs under `raw/`.
[`receipt.json`](receipt.json) records verified ZIP digests and **1,852 raw
stdout/stderr byte/hash entries** across eight Ubuntu, seven Windows and five
macOS shards. All archives were opened in memory and retain text evidence only;
no native-image/PDB payload is included. Counts from different platforms are
not combined into a target or test total.

Independent source comparisons bind all 1,099 Ubuntu selected inputs to b550,
all 798 recorded macOS Rust files to b550, and the reviewed Windows core files
to b550 modulo checkout CRLF. Each raw result retains its full selected map and
declares that this scope is not all build inputs.

| Observed scope | Actual result |
| --- | --- |
| Ubuntu, all eight shards | 371 unique inventory targets; 19 failed cases across 14 groups. Library: 490 passed, zero failed, six original ignored. All four new deadline cases and the original source-binding query pass. |
| Windows, six failed shards | 16 failed cases: 13 editor/overlay, two fresh V9 publication and one reserved-basename fixture. Separate shard 7 passes; current runtime/process/agent-REPL groups pass 15/8/22 tests. Shard 3 is outside this snapshot. |
| macOS, five completed shards | 16 new failures: 11 editor/overlay, two V9 publication, LSP refusal ordering and two fixture defects. Another 37 observed failures remain resource-related: 25 prompt-family and 12 explicit PDF address-space refusals. No full macOS count is claimed. |
| Codec integration targets | Core: 14 passed, one fixture failure. Transport: four passed, one fixture failure. These are executed results, not full codec certification. |

The corrections preserve the original assertions, deadlines, resource limits,
thread settings, ignores, permissions and serialized protocols:

- Owned editor analysis receives only a private, checked legacy-overlay route.
  It requires the exact published disk base and legacy-codec proof. It does not
  read a receipt that the existing editor ordering publishes afterward.
- First disk publication validates the typed native receipt's producing reader
  with the existing full capture-scope checker. Already published manifests and
  public historical readers keep their original checks. The existing V9 fixture
  now explicitly asserts that historical reading still refuses before publication.
- LSP checks immutable source/graph binding before codec/profile lookup and
  retains the original `manifest mismatch` assertion.
- The decoded-NUL fixture uses `embedded-nul.rs`, preserving the NUL bytes and
  omission assertion. The transport fixture reads typed `changes::Approval` and
  its existing `plan_id` field. No production path or schema is changed.

The new correction passes scoped rustfmt parsing/format and diff checks.
[`construction-clippy.json`](construction-clippy.json) indexes the raw text-only
`construction-clippy-attempts.zip`: the first attempt reports E0061 at an internal
Git-construction caller and retains an owned MSVC helper until forced cleanup.
The existing four-argument boundary is restored at `8525787d`. The second attempt
is interrupted by the original disk reserve at 328,544,256 free bytes, before
compiler completion and without diagnostics observed. Its original cleanup proves
the owned tree empty; selected bytes rehash unchanged, but final controller/clean
HEAD checks are not reached. Neither attempt is accepted or executes Yoke tests.
Fresh complete affected groups, all-target Clippy and same-source CI remain
required. The unrelated DAP `no-configuration-phase` failure expires the original
three-second deadline without phase evidence; no production cause or fix is
claimed. The original strict macOS 256 MiB address-space contract remains unmet.

[`node-ci.json`](node-ci.json) retains successful job/step metadata for
[PR7 Node CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40499831285)
and [PR8 Node CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40499822446).
All six OS/Node combinations pass typecheck, tests, smoke and publication/package
dry-runs at those recorded heads. The two inspected Windows-22 logs each report
269 tests, 267 passes, two skips and zero failures. No Rust acceptance follows
from the Node jobs' `native` names or from a publication dry-run.

[`local-observations.json`](local-observations.json) indexes two additional
text-only bundles. `cache-eviction.zip` preserves the exact cleanup helper, plan,
events and result for 1,943 receipt-emitted `.rmeta`/`.rlib` files. Held identity,
single-link and nonreparse checks precede every individual unlink. Sources,
protected EXE/PDB roles, selected private images and retained receipts remain.
Prior reported allocation: 1,221,591,040 bytes; observed free space:
5,410,709,504 to 6,631,845,888 bytes. Concurrent OS activity remains outside that
measurement. This is not 12 GB reclaimed or cache-content attestation.

`interrupted-consumer.zip` preserves the later d6 whole-library retry's request,
input map and partial streams. After turn interruption its exec handle is missing;
the exact controller mutex opens with error 2, and final result/Job/source receipts
are absent. These observations prove neither natural root/tree exit nor successful
cleanup. No whole-suite acceptance or replacement lifecycle receipt is fabricated.
