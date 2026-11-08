# Prepared root-only normal R3 gates

Status: **PREPARED, UNEXECUTED**. These new ignored files do not run anything when parsed or read. No H18 or full acceptance credit is established by preparing them. Existing H18 plans, prior wrappers, promotion proposals, normal/image sources and `acceptance_wave4.py` remain unchanged.

Prerequisites, in order:

1. Root completes the actual Wave 5 metadata apply and 2378-file postflight.
2. Root applies the reviewed 32-file normal promotion V2 and verifies the exact 2406-file map, SHA `afdc36edc71c1396dece79d8cc5ae1d5fddf81100c687a5878e373ac43825bcf`. Both wrappers refuse to run before its completed journal exists and the current full Git-visible source map matches.
3. Before the **R5-to-normal Cargo source-context switch**, root invokes the separately reviewed own-package fingerprint quarantine being prepared by `r3_test_promotion`, and records its receipt. These wrappers do not invoke, perform, or certify that quarantine. Do not run any other source context concurrently or switch contexts/quarantine between normal H18 and acceptance phases.
4. Root independently reviews the prepared tooling and its hashes. The commands below are instructions for root; the preparation agent has not invoked them.

From the normal `Code-Intelligence` checkout, run H18 once:

```powershell
rtk proxy python -X utf8 target/parity-validation-20261015/next-historical-view-comparison/revisions/normal-root-runner-v1/run_h18_normal.py
```

It uses one synchronous command with six explicit `--test` targets, `cargo test --workspace --locked -j 2 --no-fail-fast ... -- --test-threads=4`. It uses a fresh `D:/Yoke-Test-Scratch/r3-normal-h18-20261016-v1`. No extra Cargo inventory command, modified Cargo arguments, target cleanup or quarantine is added. Evidence is create-only at `target/parity-validation-20261015/r3-normal-h18-20261016-v1`: full source-before/source-after maps, raw `h18.log`, and `receipt.json`. The exact 51 historical names across six groups are a selection oracle; every actual name/result must be freshly observed and all six groups must match. A zero exit code alone is insufficient.

After H18 is accepted, run the full wrapper once:

```powershell
rtk proxy python -X utf8 target/parity-validation-20261015/next-historical-view-comparison/revisions/normal-root-runner-v1/run_r3_windows_acceptance.py
```

It sequentially invokes the unchanged runner SHA `b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15` for `format`, `clippy`, `python`, `typescript`, `native`, then `finalize`. The runner's original commands, fresh per-executable inventory, two-target disk cleanup, ACL partition and ignored-test treatment remain unchanged. The wrapper itself adds no Cargo arguments. Each phase has its own fresh `D:/Yoke-Test-Scratch/r3-normal-frozen-20261016-v1-<phase>`; native has fresh `C:/Users/Public/Yoke-Test-Scratch/r3-normal-frozen-20261016-v1-acl`. The frozen runner owns exactly its original three ACL fixtures.

The runner's bundle is `target/parity-validation-20261015/r3-normal-frozen-20261016-v1`. The wrapper's separate create-only bundle is `target/parity-validation-20261015/r3-normal-pipeline-20261016-v1`, containing full source maps, raw `pipeline.log`, and `pipeline.json` with script, runner, log and individual receipt hashes. Full acceptance comes from the five fresh phase receipts and verified finalize summary on the identical promoted source. No estimated count, previous pass, separate capacity run or H18 result is added to full acceptance totals.

Any prior evidence or scratch root causes refusal; preserve failed evidence and prepare a reviewed new tag for a retry. Neither wrapper automatically retries, rolls back, deletes a tree, overwrites previous evidence or changes the frozen runner. Runtime source drift, missing receipts, failed stages and validation errors force a nonzero wrapper result.
