# Wave4 acceptance runner: two-target integration batches

This is an ignored staging change to the acceptance harness. No product source,
old runner, failed attempt or preservation tool is changed. It does not establish
an MSVC root cause, universal parity, a capacity guarantee or release readiness.

The failed full 2107 native phase remains unaccepted: its first integration
inventory could not link `agent_dependency_options` (link.exe 1140, PDB limit).
Its passing library partition and four passing phases cannot contribute results
to the next acceptance. A same-flags single-target inventory later succeeded
after root's reviewed cleanup of ended artifacts. C: remained full. This is
consistent with batch resource pressure or partial artifacts, not proof of a
specific linker-internal cause or a safe free-space threshold.

## Exact runner change

`../acceptance_wave4.py` is an exclusive fresh copy of `../acceptance_wave3.py`.
The old runner SHA256 is
`0d7ed0fa533d55c6518da3dfdc322d6e9e227361f1e88070edbaf2b7067ba45e`.
The new runner SHA256 is
`b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15`.
Both have 27,430 bytes. Exactly three bytes change from `6` to `2`: integration
range step, slice width and label divisor. ROOT/EVIDENCE derive from the same
directory depth. Commands, `-j 2`, runtime threads, deadlines, checks, explicit
target inventory, D/C ACL partition, cleanup rules and freeze semantics remain
byte-identical. A two-target batch bounds selected integration artifacts; it
does not imply only one linker or bound an individual target's PDB size.

Root must freshly execute all five phases under one new tag, source manifest
and new runner hash. No result from 2107 or older attempts may be promoted.
Only root runs Cargo/Node and the reviewed ended-artifact cleanup. Native
acceptance still requires every discovered metadata target exactly once.

## Preservation reader

`../next-wave4-preservation/receipt_core.py` remains SHA256
`35e097ea51dee414c88c68d645ad43ca6bc2d71c6335152f05107e99a22f99f3`.
It already derives arbitrary nonempty sorted unique `--test` batches from raw
commands, requires consecutive labels, replays each all/ignored inventory and
runtime group, and requires concatenation equal to the full sorted Cargo
metadata inventory without duplication. It has no fixed-six policy. Therefore
no verifier version or parser change is needed. The same reviewed preserver
requires all five successful fresh receipts with the exact new runner hash.

## Synthetic proof and limitations

`test_runner_fallback.py` never imports or executes the candidate runner.
It checks the old/new hashes and complete three-byte inverse, reads the literal
step through AST, and traverses the actual frozen `replay_native` reader using
coherent in-memory raw fixtures: 209 synthetic targets, 105 consecutive labels
(including labels 100–104), 104 pairs and a singleton tail, ordinary/ignored
library tests and the three disjoint exact ACL fixtures. It independently
rejects coherent omission/duplication, missing tail inventory/runtime, command
versus raw selector mismatch, missing middle label, within-batch duplication,
duplicate receipt labels and a failed inventory despite claimed acceptance.
Fixture I/O is mocked; this is parser/coverage proof, not real execution or a
filesystem/hashing-boundary proof. Earlier synthetic fixture label/assertion
construction errors were corrected without changing runner or reader behavior.

`run_synthetic.py` runs only these Python tests and writes a fresh ignored raw
log with exclusive creation. The already reviewed preservation boundary tests
remain unchanged. Independent review must clear actual staged bytes and these
synthetic cases before root starts native acceptance.

After all five phases succeed, preservation still requires a separately
reviewed concrete plan with the successful tag, source SHA, new runner SHA and
fresh destination. Old 160+210+416 historical artifact pins, indices and failed
attempts stay intact. No actual preservation is authorized by this proposal.
