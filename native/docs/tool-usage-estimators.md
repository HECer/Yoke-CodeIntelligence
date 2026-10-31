# Explicit tool-usage token statistics

Global YAML can select `token_count_estimator: CHAR_COUNT`, `TIKTOKEN_GPT4O` or
`ANTHROPIC_CLAUDE_SONNET_4`. Equivalent host JSON is:

```json
{"tokenCountEstimator":"CHAR_COUNT"}
```

Omission retains the host selection; null clears it. Project, project-local,
context and mode declarations refuse this field, including null. Unknown names,
maps, numbers and booleans refuse before prompt rendering. Without selection,
preceding profile/State serialization, fingerprints, results and native byte
estimates remain unchanged. The public default is not silently enabled.

The compiled declaration contract pins Serena revision
`ede329aaf0c824bea75455a2926f01420d7aa620`, its
[analytics declarations](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/analytics.py)
and global/tool/agent source blob IDs. Native implementation is independent;
no upstream application code is imported, executed or vendored.

## Three counting methods

`CHAR_COUNT` counts Unicode scalar values and divides each input/output count
by four, rounding downward. Combining scalars and CR/LF count separately; astral
characters count once. A short text can genuinely count as zero. This is a
character heuristic, distinct from native UTF-8-byte estimates.

`TIKTOKEN_GPT4O` uses `o200k_base` with `encode_ordinary`, including literal
`<|endoftext|>` and `<|endofprompt|>` as ordinary text. MIT-licensed tiktoken-rs
0.12.0 and its vocabulary are pinned through Cargo. The vocabulary is compiled
into the dependency; selected Sessions share one lazily initialized tokenizer.
No runtime vocabulary/cache download occurs. This does not establish billing
or every model/server's tokenizer identity. Authored numeric fixtures use the
[OpenAI cookbook](https://github.com/openai/openai-cookbook/blob/main/examples/How_to_count_tokens_with_tiktoken.ipynb),
blob `19ce7689214e0868bf6c8ed8e15c051e3ab18eac` (6, 7 and 8-token examples).

`ANTHROPIC_CLAUDE_SONNET_4` retains its public historical name but selects the
pinned `claude-sonnet-4-6` model. It sends the input and output texts separately
as single user messages to `POST /v1/messages/count_tokens`, reading each
`input_tokens` report. Message framing can contribute to the count; an output
text counted as a user message is not an assistant-completion billing receipt.
This uses the [count-only API](https://platform.claude.com/docs/en/api/http/messages/count_tokens),
not a generation request. There is no fallback to a fabricated or local count.

## Independent remote authority

The remote name alone never grants network or text-disclosure authority.
The shipped CLI additionally requires both global flags:

```text
--allow-tool-token-disclosure --token-count-credential HOST_STORED_NAME
```

These flags apply only to Session/serve-session/serve-http/agent-repl routes
whose host profile explicitly selects the remote estimator. The name refers to
an existing native OS model-token credential. Raw keys and environment credential
lookup are not supported. Profile files and YAML cannot deserialize the runtime
grant. The CLI fixes the HTTPS endpoint to api.anthropic.com; no tool argument
selects a destination. The grant covers sending entire admitted tool arguments
and results, which can contain source, paths and other sensitive information.
Local estimators require neither flag and read no counting credential.

Trusted library callers can separately construct a nonserializable, revocable
`tool_usage::http::Host` with a typed endpoint/credential reader. Plain HTTP is
limited to explicit literal loopback IPs under its separate host opt-in; custom
TLS roots are bounded. Userinfo, query, fragment and another route refuse.
There is no proxy, redirect, cookie persistence or automatic retry. HTTP errors
use static diagnostics and never return remote bodies or credential values.

The estimator, compiled contract/library identity and any host runtime binding
participate in Session fingerprinting. Changing selection or endpoint/credential
reference/certificates refuses ordinary resume. Current Session state and grant
are rechecked after the operation, before credential access/send, between counts,
after responses and before aggregate publication. These are cooperative admission
checks, not an OS-atomic transaction against concurrent external actors.

## What is counted and retained

Only completed public Serena operations passing through the native compatibility
adapters are counted, including admitted partial results. Ordinary and persistent
REPL dispatch count each public operation once. Direct facade callbacks and other
protocol profiles do not acquire implicit counters. Failed underlying operations
do not increment tool totals.

The input is canonical native JSON of the supplied arguments; the output is
canonical native JSON of the native tool result. This differs explicitly from
Python's dictionary/string representation and MCP wire envelopes. It is not a
claim of byte-identical upstream statistics. Final transport formatting/delivery
can still fail after a completed operation was counted.

Both complete texts are admitted before the first remote request. Each has a
4,000,000-byte ceiling. At most 96 tool entries with bounded ASCII names are
retained; call/input/output totals use checked u64 arithmetic. Both estimates and
every next counter value succeed before one aggregate entry changes. Deadline,
revocation, malformed/duplicate/extra response fields, overflow or a failed second
count never publish a partial entry. A numeric zero remains distinct from failure.

Counts are ephemeral to one Session instance and reset when it is recreated.
Only per-tool `numTimesCalled`, `inputTokens` and `outputTokens` are retained.
No argument/result text, source hash, snapshot pin, history row or retention root
is added. `hostGrantRemoteAttempts` is a separate cumulative diagnostic for the
shared grant, including failed/unknown attempts; clearing tool totals does not
erase that diagnostic or alter another Session's counters.

`configuration`/`get_current_config` reports `toolUsageStatistics`. Its returned
snapshot precedes charging that configuration call itself. Native Session
operations `toolUsage` and `clearToolUsage` read/reset instance counters with
empty closed arguments. Both are passive and cannot enter activation commands.
Selected-hook inspection avoids new Session publication; without selected hooks,
the existing initial Session-publication behavior remains. Reading/resetting
counters adds no persisted counter record or State update. Dashboard JSON is
refreshed after successful counting and failed attempts; no HTML chart/control
or visual acceptance is added by this change.

For returned native ok/partial envelopes, selected remote counting sets
`metrics.localOnly=false` and `externalUsage=unknown`. A separate
`toolTokenCounting` record identifies two count requests, zero generation calls
and unestablished billing. Original native size/token values and methods are
preserved and explicitly scoped to the underlying result before this accounting
metadata. Measurement includes the decorated returned result. Existing answer
character limits are rechecked after decoration, before disclosure. Local/absent
selections do not decorate results. Reported provider usage and billing ledgers
remain independent.

An accounting failure after a successful operation returns an explicit
`tool_usage_statistics_failed_after_operation_no_rollback` error. The underlying
operation may already have read, captured, changed source or produced other
authorized effects; counting is not a transaction rollback or delivery receipt.
Original operation/cleanup failures are returned before accounting starts.

## Limits and qualification

Count responses are limited to 16,384 bytes, 32 headers/8,192 header bytes and a
single integer input_tokens field from 0 through 100,000,000. Body/schema/status,
media/encoding and credential errors refuse without retry. Original request,
frame, source permission, answer, cancellation and cleanup limits remain active.
The same outer deadline covers the operation and counting. Synchronous native
tokenization/client initialization can pass expiry before their next cooperative
check; this is not a new hard OS CPU, memory or interruptible-I/O quota.

Two new inert declaration tests were observed failing before data was added and
then passing; twelve related inert tests pass. Typecheck and npm (266 pass,
0 failures, 3 skips) passed separately. Native compilation and all authored Rust
fixtures remain unrun locally, including numeric BPE vectors, actual owned count
HTTP, stale/revoked refusal, count-once REPL, no-hook CLI, immediate dashboard,
stdio/MCP HTTP and disclosed native-envelope metadata. No live Anthropic request
or personal credential was used. Dependency byte/size/license packaging, real
vendor, Windows kernel/resource, combined platform/browser/VSIX/main/release
acceptance still require exact hosted evidence. Full upstream parity remains open.
