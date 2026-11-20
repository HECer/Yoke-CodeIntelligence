# Benchmarks and evidence

## Graph edge diagnostics

Run the native diagnostic on an explicitly selected Graphify graph or extraction.

~~~~sh
yoke graph diagnose --input /host/graph.json --examples 5
yoke graph diagnose --input /host/raw-extraction.json --producer-source /host/extract.py
~~~~

The report binds the exact input SHA-256. It counts bound and unresolved
endpoints, self-loops, duplicate edge objects, and variations in relation,
source location and context. Directed and undirected collapse counts show
how many present edges a one-edge-per-pair representation would discard.
Endpoints take string or integer IDs. Edge aliases may use source/target or
from/to. Paired _src/_tgt markers restore the original direction when both
identify the same endpoints. Partial or conflicting markings count as invalid.
An undeclared target whose ID starts with ref_ is counted separately
when its source is present. The optional producer scan reports candidate
suppression declarations by line and identifier without evaluating the file.

The command reads at most 32 MB of graph JSON and 8 MB of producer source. The
example limit applies to each list, up to 50, and zero keeps only the counts.
The input files stay unchanged and the diagnostic opens no project store. A
post-build file can describe risks in the edges it still holds. Measuring loss
during production requires the raw pre-build extraction. The benchmark harness
below supplies separate performance and cost measures.

## Benchmark harnesses

`node benchmarks/run.mjs` is a synthetic native retrieval microbenchmark, not a coding agent or upstream comparison. Cold capture/indexing and warm explicit-snapshot queries are separated. Its output cannot substantiate token-cost, task-correctness or production-latency claims.

`node benchmarks/paired.mjs --config /host/reviewed-campaign.json --trust-drivers --output /host/report.json` is a bounded acceptance harness for separately supplied real agent drivers. The config contains `schema:1`, one `model` identity, `iterations`, `maxCost`, `maxCostPerAttempt`, `tasks` (`id`, `root`, `prompt`, independent `verify` command/args entries and `protectedPaths`) and at least two `arms` (`id`, executable `command`, `args`, optional host `env`). Executable paths must be absolute when needed; commands run in fresh copied workspaces.

Drivers receive one JSON request on stdin, including the workspace, task, cold/warm preparation flag and cost reservation. They modify that copy and return one JSON response with `schema:1`, the requested `model`, `usage` and numeric `actualCost` or null. Setup/indexing, retries, warm-up and nested model calls belong in driver accounting. Drivers are responsible for equivalent model settings and genuinely distinct cold/warm behavior. A label alone is not proof of equivalent execution. Unknown cost keeps its reservation; all attempts are checkpointed before execution. Driver exit, protected test files and independent verification are checked separately. Results group success and cost per successful task by arm. Missing cost makes that metric unknown rather than zero.

This harness does not supply a commercial agent subscription, provider key, trusted oracle, OS sandbox, independent billing meter or statistically representative task set. The shipped harness tests use an explicitly artificial protocol driver to exercise accounting and refusal paths. **No actual paired multi-agent/model campaign has been run for v0.3.0.** Trusted external programs can execute code; review them, keep credentials out of repository config, and use an appropriate operating-system/container boundary for untrusted tasks.
