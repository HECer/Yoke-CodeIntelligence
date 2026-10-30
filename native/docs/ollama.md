# Explicit local Ollama adapter

`ollama::provider_reply(&OllamaSpec, input_bytes, &Deadline)` implements the native source-bound executable-provider protocol against an already running local Ollama API. It uses Rust and existing dependencies. It never invokes the Ollama CLI, starts a server, pulls a model, changes configuration, reads credentials or writes source. An explicitly requested inference may load the selected installed model into RAM through the backend's normal inference behavior.

The trusted host selects `endpoint`, `model` and `model_digest`. Endpoints must contain an explicit port and numeric loopback address, such as `http://127.0.0.1:11434` or `http://[::1]:11434`. Hostnames, remote addresses, HTTPS, URL credentials, paths, queries, fragments, redirects and compressed bodies are refused. No endpoint/model/version is inferred from source or configuration. The selected digest must be 64 lowercase hexadecimal characters, matching both the installed model manifest and the request's `modelVersion`.

The hidden executable worker can be selected as a host-authorized `ProviderSpec` command:

```text
yoke model-worker --endpoint http://127.0.0.1:11434 --model installed-model:tag --digest <64-character-installed-digest>
```

The parent provider operation sends its bounded schema-1 request on stdin and receives one JSON reply on stdout. Users receive a native adapter without needing a bespoke script. Default indexing/query still makes no model call. See [the provider protocol](providers.md) for exact source bindings, request identity, integer currency budgets and unknown billing.

Before inference, the adapter checks the selected name, positive local model size, GGUF format and exact digest through [the installed-model inventory](https://docs.ollama.com/api/tags), then checks [reported capabilities](https://docs.ollama.com/api-reference/show-model-details). It repeats the digest check after inference. Missing models/capabilities, changing tags, ambiguous inventory and cloud/remote metadata fail explicitly. These two checks are stable metadata checks, not an atomic lock against a hostile tag writer or cryptographic attestation of execution. The local daemon/model remain trusted software; loopback HTTP and cloud rejection do not provide OS network isolation.

| Native operation | Ollama API and boundary |
|---|---|
| Corpus/query embeddings | [`/api/embed`](https://docs.ollama.com/api/embed), exact text arrays and `truncate:false`; finite nonzero vectors bind to every exact supplied ID with one common dimension |
| Source summaries | [`/api/chat`](https://docs.ollama.com/api/chat), non-streaming [structured JSON](https://docs.ollama.com/capabilities/structured-outputs); bounded statements cite unique selected source IDs |
| Reranking | Structured completion scores every exact candidate once from 0 to 1; scores are inferred relevance, not calibrated confidence or lexical fallback |
| Media description | Models reporting completion and vision receive captured hash-checked PNG/JPEG/WebP bytes as base64 images; audio, video and PDF interpretation are explicitly unsupported |

Source/media are untrusted data. Generated permissions and tool calls are never executed. Structured content is revalidated for duplicate/unknown fields, exact citation/candidate IDs, finite scores and bounded text. Incomplete/truncated model output or tool calls fail. The native binding fields echo the validated request, without attesting model understanding. Summaries/descriptions remain `model_inference_not_verified_fact`; retrieval/reranking remain ranking rather than evidence.

One deadline covers connection, writes, headers, reads, JSON validation and digest checks. Request bodies are capped at 16,000,000 bytes, model replies at 32,000,000 bytes, metadata at 2,000,000 bytes, headers at 16,000 bytes/64 fields. Content-Length and chunked framing are supported. Conflicting/duplicate lengths, truncation, chunk extensions/trailers and extra bytes fail. Chunk counts, JSON nodes/depth and decoded vectors are bounded. The deadline bounds the client, not an OS quota or proof that the separate daemon stopped all computation.

Structured chat requires context metadata of at least 8,192 tokens and uses at most 16,384 context tokens with 1,024 output tokens. Serialized user source/query JSON is conservatively limited to context size minus 4,096 control/schema bytes and 1,024 output units. Oversized scope fails without silently omitting sources. Tokenizer/template behavior remains backend-specific and is not attested by those limits. The documented embedding API option disables input truncation.

Ollama source summaries request a concise subset: at most `min(sourceCount, 4)` statements, each at most 512 UTF-8 bytes, and at most `min(uniqueSourceCount, 4)` unique citations per statement. The generated JSON schema caps text at 512 characters; Rust postvalidation applies the stricter byte limit, including non-ASCII text. All selected source bindings remain exact and available for citation; a concise summary does not claim exhaustive coverage. The generic executable-provider summary contract remains unchanged. Truncated, invalid or oversized generated output still fails explicitly.

`actualCostMicros:null` preserves unknown billing instead of inventing a zero-cost external bill. Chat usage is mapped only when all input, output and cached-input counts are reported; otherwise usage is unknown. Embeddings without complete usage fields remain unknown. Replies contain no credentials or backend configuration secrets.

Standard tests use real local TCP/HTTP fixtures labeled as protocol fixtures, not neural models. They exercise digest/source binding, cloud/capability refusal, malformed framing, structured citations/ranking/vision, limits and timeouts. The separate actual neural gate is ignored by default:

```powershell
$env:YOKE_OLLAMA_ENDPOINT = 'http://127.0.0.1:11434'
$env:YOKE_OLLAMA_MODEL = 'explicitly-installed-embedding-model:tag'
$env:YOKE_OLLAMA_DIGEST = '<actual-64-character-installed-digest>'
cargo test --locked -j 2 --test ollama actual_local_installed_embedding_model_gate -- --ignored --nocapture
```

It requires an already available daemon/model. Missing host configuration, unavailable runtime or missing weights fails without selecting a fake, starting an app or downloading a model. HTTP fixtures do not establish neural quality, multilingual accuracy, pricing or actual model availability.

An explicitly authorized isolated integration run on 2026-10-14 passed the actual ignored Rust embedding gate and the native `yoke model-worker` stdio query path. The installed Ollama 0.40.3 runtime used the official [all-minilm:22m](https://ollama.com/library/all-minilm:22m) embedding model, independently verified Apache-2.0 license and manifest digest `1b226e2802dbb772b5fc32a58f103ca1804ef7501331012de126ab22f67475ef` (45,960,996 total downloaded bytes). The worker returned 384 finite, nonzero coordinates with exact model/request/input bindings and unknown billing. A dedicated model directory outside the repository, sanitized server environment and unused numeric loopback port isolated the test. The owned hidden server and its descendants were stopped through a Windows job; the endpoint closure and unchanged existing user identity metadata were checked. The existing default server and broken user model junction were left untouched. Weights are excluded from release artifacts. This embedding run evaluated neither structured completion nor vision. Explicit test setup does not change the adapter's no-start/no-pull behavior.

A separate actual structured-completion run used official [smollm2:135m](https://ollama.com/library/smollm2:135m), Apache-2.0, 270,898,672 bytes, digest `9077fe9d2ae1a4a41a868836b56b8163731a8fe16621397028c2c76f838c6907` and the reported 8,192-token context capability. Native source indexing, ProviderSpec and Store/Ledger operations accepted an actual bounded relevance score for one exact captured candidate. The initial summary attempt failed without accepting generated text; a diagnostic under a 30-second budget confirmed a Windows socket timeout. After applying the concise-summary limits above, one final end-to-end retry under a 60-second native budget passed: one 83-byte inferred statement cited the exact captured source ID, hash and UTF-8 byte span. Its unknown billing reservation remained encumbered, and source bytes were unchanged. All owned server jobs were stopped and endpoint closures checked. [Embedding evidence](../../docs/validation/native-workflows-ollama-live.json), [initial structured-completion evidence](../../docs/validation/native-workflows-ollama-structured-live.json) and [successful concise-summary evidence](../../docs/validation/native-workflows-ollama-summary-live.json) record each validation scope. Neural quality and actual vision remain unevaluated.
