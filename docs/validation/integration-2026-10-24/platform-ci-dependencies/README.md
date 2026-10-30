# Independent platform acceptance scheduling

The original `native-rust` matrix depends on the complete cross-platform
`native-shards` matrix. Each of its three acceptance jobs therefore waits for
all 24 shard jobs, including shards from the other platforms. The replacement
uses one eight-shard matrix and one acceptance job per platform. Each acceptance
job depends only on its own platform's shard matrix. This changes dependency
readiness; it does not establish hosted queue speed or native correctness.

All 24 shard and three acceptance check names remain the same after matrix
expansion. Every parsed original step, command, step condition, timeout,
fail-fast policy, runner, event and permission is unchanged. The eight-receipt
coverage check remains before the actual rust-analyzer test, release build,
binary upload and Windows owned-history check. A failed or missing own shard
still cannot produce complete coverage. The `always() && !cancelled()` policy
still attempts independent evidence after failure and stops on cancellation.
All three platforms remain required for full acceptance.

The two original step lists are shared with plain YAML anchors and aliases,
which [GitHub officially supports](https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations#yaml-anchors-and-aliases).
No YAML merge key, reusable-workflow check-name prefix, native test filter,
profile change, assertion relaxation, CI skip or automatic cancellation is added.

## Actual local verification

`checks.json` records a parsed comparison against the exact workflow Git blob at
`d80c6c784f462d5ad60c0f553986b2b4895108d4`. PyYAML 6.0.3 expands the aliases;
all 27 original check names and step lists match. Eighteen deliberately damaged
variants are refused, including missing/duplicate shards, cross-platform or
missing dependencies, altered commands/budgets, skipped failure evidence,
missing release/coverage/live steps and wider permissions. The first private
checker attempt stopped with KeyError on its missing-dependency adversary;
explicit absence handling was corrected before the complete successful run.

A separate 6,144-case dependency-readiness model varies every own eight-shard
terminal mask, both other platforms' terminal states and cancellation. It
confirms the new graph waits for all eight own shards and does not wait for the
other platforms. This models prerequisites only; it does not simulate GitHub
execution, successful receipt validation or queue availability.

[Actionlint 1.7.12](https://github.com/rhysd/actionlint/releases/tag/v1.7.12)
exits 0 on the actual workflow with shellcheck and pyflakes disabled. The
official Windows archive length and published SHA-256 digest are independently
checked before extraction. No downloaded tool binary is committed. These are
local structural checks; GitHub must still execute the revised workflow.

The complete Python suite also passes: `python -Werror -m unittest discover -s
scripts/tests -v`, 113 tests, zero failures or skips. The [Windows typing
checkpoint](../native-windows-typing/README.md) retains separate Cargo/Clippy and
TypeScript results; those native input pins describe the original d80 workflow,
not this scheduling edit. No native test image is read or executed by these
workflow checks. Existing failing hosted observations remain failures.

The exact before/after YAML, private checker, linter output and Python log are
preserved in `raw-evidence.zip`. `raw-index.json` records every reopened member's
length and SHA-256. The checker is evidence of the one-off local procedure,
not a new installed project dependency or normal test entry point. Hosted
PowerShell phases, native runtime/platform/history/VSIX, full STATUS, main and
release remain open.
