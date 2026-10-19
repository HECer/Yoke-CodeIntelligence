# Native advanced workflow entry points

The core remains local and deterministic. Optional execution, model inference, source grants and state deletion require separate explicit host authority. MCP arguments and captured configuration cannot grant it.

| Workflow | CLI | Detailed contract |
|---|---|---|
| Observed worktree index conflicts | `yoke git conflicts --allow-index-read --trust-git [--allow-git-sources --scope-snapshot <id>]` | [Git conflicts](git-conflicts.md) |
| Manifest-balanced source retrieval | Session MCP `ci_retrieve` with optional `scopeMode`; private auto in `trail_find_code` / `graft_find_code` | [Retrieval](manifest-balanced-retrieval.md) |
| Structural intraprocedural taint candidates | `yoke dataflow [--policy policy.json]`; MCP `ci_status` with `mode: dataflow` | [Dataflow](dataflow.md) |
| Configured TS/JS bindings and virtual solution builds | `yoke compiler --node <absolute> --typescript <absolute> --trust-compiler [--build]` | [Compiler](compiler.md) |
| Supplied test observations | `yoke test-map import --input map.json` | [Test mapping](test-map.md) |
| Actual copied-snapshot Node collection | `yoke test-map collect --selections selections.json --node <host-executable> --trust-runtime` | [Test mapping](test-map.md) |
| Per-test selection | `yoke --snapshot <target> test-map select --base <base> [--test <id> ...]` | [Test mapping](test-map.md) |
| Embeddings, semantic query, reranking, summaries, media inference | `yoke models --config host-provider.json --trust-provider <operation>` | [Providers](providers.md) |
| Durable task usage/cost totals | `yoke ledger <task-id>` | [Providers](providers.md) |
| Reviewable history retirement | `yoke retention plan [--minimum-age-ms <age>]`; `retention apply <plan> --confirmed-hash <hash> --allow-state-deletion` | [Retention](retention.md) |
| Quarantine purge | `yoke retention purge-plan`; `retention purge <plan> --confirmed-hash <hash> --allow-state-deletion` | [Retention](retention.md) |
| Docker execution | `yoke verify-container --plan <id> --profile host-container.json --trust-project` | [Container](container.md) |
| Stateful Python debugger / REPL | `yoke runtime <entry.py> --python <absolute> --requests requests.json --trust-runtime [--allow-evaluation]` | [Runtime](runtime.md) |
| Local installed model inference | Host provider command `yoke model-worker --endpoint <loopback> --model <name> --digest <hash>` | [Ollama](ollama.md) |
| OS-held TLS identity / bearer token | `yoke credentials tls <name> --hostname <name>`; `yoke credentials token <name>` | [Credentials](credentials.md) |
| Signed source service | `yoke federation-service --profile host-service.json --trust-service [--allow-remote-bind]` | [TLS transport](federation-http.md) |
| Verified TLS source client | `yoke federation-request --profile host-client.json --input request.json --trust-transport` | [TLS transport](federation-http.md) |

Provider configuration accepts exactly `command`, optional `args`, `modelId`, `modelVersion`, `taskId`, `currency`, `maxCostMicros` and `taskLimitMicros`. It cannot carry a trust flag, API key or password. Fetch provider credentials through an OS credential store. Each operation uses the typed host profile; there are no automatic calls. `embeddings`, `search <query> [--limit N]`, `rerank <query> --entity <id> ...`, `summarize --entity <id> ...` and `media <path>` have distinct source-binding schemas. Provider-reported usage and costs remain unverified; unresolved sent attempts keep their reservations.

Container configuration accepts exactly `dockerCommand`, `daemonEndpoint`, optional `dockerArgs`, digest-pinned `image`, argv `command`, explicit `isolation: linux`, numeric nonroot `user`, `memoryMb`, `cpus`, `pids`, `tmpfsMb` and `testCountPattern`. The endpoint is local (`unix://` or Windows named pipe), pinned on every invocation; image pulling and container network access are disabled. The test-count pattern is a host profile assertion, not a claim of exhaustive test adequacy. See the container contract for limits and exact cleanup behavior.

Retention plans are data for review. Development of this capability does not authorize deleting an existing user's history. State-deletion authorization applies only to the exact confirmed plan supplied by the host. Unknown age or dependencies defer retirement. Current snapshots, pins, editors, change/recovery plans, verification, knowledge, vector/media records and artifacts are protected. Purge applies only to previously committed CAS quarantine and rechecks current reachability.

Every command supports the common root, immutable snapshot, deadline and output budget flags where relevant. Artifact-capable workflows expose oversized registered immutable artifacts through `ci_read`. Manifest-balanced retrieval and compatibility `find_code` instead fail at their hard output ceiling. Actual collection or execution with any failed/skipped-only/zero-executed job returns a failing CLI exit code. Imported observations are labeled supplied data and do not authenticate execution.

Host JSON inputs reject duplicate keys at every nesting depth and enforce byte, depth and node limits. Typed schema failures use fixed errors without quoting input values. Runtime evaluation/REPL requires its own host flag; session commands cannot grant it. Executed programs retain host privileges and may have effects outside the copied source tree. Runtime reports therefore distinguish the host's source-copy behavior from unmonitored program effects.

Failed program completion or an unsuccessful evaluation/REPL command returns a failing CLI exit status while preserving session events. An explicitly requested `close` remains a successful debugger command without claiming program completion. MCP and LSP JSON-RPC frames also reject duplicate object fields and ambiguous request identities.
