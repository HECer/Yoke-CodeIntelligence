# Native host-selected HTTP model workers

The native worker supports OpenAI-compatible Chat Completions and embeddings, Anthropic Messages, Azure OpenAI v1, and pinned dated Azure deployment routes. It makes one HTTP attempt for one existing native provider request. Bedrock and other dialects are not implemented by this worker.

Nothing opens a connection by default. The Rust `HttpProviderSpec` is deliberately not deserializable: the host selects the endpoint, exact requested model/version namespace, exact expected response model label, credential reference, authentication dialect, token limit field, JSON output dialect, and embedding/vision capabilities. `validate_spec` performs pure preflight without DNS, credential access, or network activity. Remote endpoints require HTTPS. Numeric loopback HTTP additionally requires an explicit host grant. Credentials cannot appear in endpoint userinfo, query parameters, configuration payloads, or model requests.

## Endpoint and wire contracts

| Dialect | Configured endpoint | Constructed operation | Authentication |
| --- | --- | --- | --- |
| OpenAI-compatible | Exact API base, for example `https://api.openai.com/v1/` | `chat/completions` or `embeddings` | Explicit bearer or API-key selection |
| Anthropic Messages | Exact API base, for example `https://api.anthropic.com/v1/` | `messages` | Explicit bearer or `x-api-key`; pinned `anthropic-version: 2023-06-01` |
| Azure v1 | Resource origin or explicit proxy base | `openai/v1/chat/completions` or `openai/v1/embeddings` | Explicit bearer or `api-key` |
| Azure dated deployments | Resource origin or explicit proxy base | `openai/deployments/{deployment}/{operation}?api-version={pinnedVersion}` | Explicit bearer or `api-key` |

Azure v1 sends the configured model/deployment label in the body. The dated deployment route binds deployment in the URL and omits body `model`. The expected returned model is independently host pinned; an Azure deployment alias must not be guessed to equal the service's response model label. A matching label is a namespace consistency check, not cryptographic model attestation.

Chat requests are non-streaming, request one choice, and never enable tools. OpenAI-compatible requests explicitly use JSON Schema, JSON object mode, or text JSON, and explicitly select `max_completion_tokens` or `max_tokens`; no automatic model-dependent fallback is attempted. Anthropic uses a top-level system instruction and `output_config.format` for structured JSON. Its native embedding operation is unavailable. Reranking is source-bound structured chat inference, not a claim that a vendor-native rerank endpoint was used. Vision requires an explicit host capability and admitted captured PNG/JPEG/WebP bytes.

## Source and accounting boundaries

The original native schema, request/input hashes, source bindings, ledger reservation, and parent worker protocol remain unchanged. Worker source path labels are admitted data; they grant no filesystem, CAS, snapshot, repository, or execution access. Dependency labels such as `node_modules` remain subject to the parent's authenticated capture membership checks. Secret/internal/state path labels are refused.

Source bindings in the response are local echoes of admitted request anchors, not statements that a remote model verified source bytes. Structured results must use only those source IDs. Summaries retain bounded statement/text/citation budgets; reranking must include exactly the supplied candidate IDs, once each, with finite scores. Media asset identity and hash are attached locally after validation.

OpenAI prompt token counts already include cached tokens. Anthropic total input is uncached input plus cache-creation plus cache-read input; cached input is the reported cache-read subset. Invalid reported counts are rejected, rather than replaced with fabricated zeroes. Valid reported usage is retained when a completion is refused, truncated, cites foreign sources, or violates the local result schema. Missing usage stays unknown. `actualCostMicros` stays null: token counts and host price estimates are not provider billing receipts. A timeout or transport failure does not establish zero cost or cancellation at the remote service.

## Resource and transport policy

Native input is limited to 16,000,000 serialized bytes. Outbound wire JSON and response bodies are limited to 32,000,000 serialized bytes; outbound serialization counts before allocation. Responses require HTTP 200 and `application/json`, identity content encoding, bounded headers, duplicate-free bounded-depth JSON, exact model identity, and operation-specific local validation. Embeddings require exact indices/count, consistent nonzero dimensions up to 4096, at most two million coordinates, and finite nonzero norms. Truncated or tool/refusal completions are not successful native data.

The client disables redirects, proxies, and hidden retries, and uses HTTP/1.1. A non-200 response returns a bound native error with unknown billing; remote error text is not emitted. Errors do not include raw response bodies or credentials. Exact credential echoes are refused in raw model text and in decoded JSON string values and keys, including Unicode and JSON quote/backslash escapes. This exact-byte guard is not a general data-loss-prevention or malicious-model confidentiality guarantee. Credential strings are zeroizing host callback results, limited to 2560 printable ASCII bytes, and are marked sensitive in request headers. Host-provided DER trust roots are additive and bounded. TLS certificate validation remains enabled.

The remaining absolute deadline bounds connection and HTTP request timeouts, with cooperative checks during admission and body reads. A cancellation flag is checked between these stages; a blocking network operation can remain until its configured remaining timeout. This is not a hard CPU-preemption or remote-request cancellation guarantee.

## CLI host integration

`models-http --config <host-profile> --trust-network <model-operation>` uses the existing provider process and ledger path. Loopback plaintext fixtures additionally need `--allow-loopback-http`. The private worker entry receives only a profile path and its exact SHA-256 binding; edits between parent preflight and child startup are refused. Model token import uses a dedicated OS credential namespace, `yoke-code-intelligence/service/model/{name}`, and a credential reference rather than secret material in JSON or command arguments. No default endpoint, model, credentials, retries, or discovery request is supplied.

A strict host profile can use the following shape (choose a real host-approved endpoint and model explicitly):

```json
{
  "endpoint": "https://api.openai.com/v1/",
  "dialect": "openAiCompatible",
  "auth": "bearer",
  "jsonMode": "jsonSchema",
  "tokenLimit": "maxCompletionTokens",
  "modelId": "HOST_SELECTED_MODEL",
  "modelVersion": "HOST_SELECTED_VERSION",
  "expectedRemoteModel": "EXACT_EXPECTED_RESPONSE_MODEL",
  "credentialName": "my-provider",
  "maxOutputTokens": 1024,
  "taskId": "source-summary",
  "currency": "USD",
  "maxCostMicros": 100000,
  "taskLimitMicros": 1000000,
  "capabilities": {"embeddings": false, "vision": false},
  "deployment": null,
  "apiVersion": null,
  "rootCertificatesDerBase64": []
}
```

The namespace labels above are placeholders, not defaults or recommendations. For Anthropic set `dialect` to `anthropicMessages` and select `anthropicApiKey` or `bearer`; for Azure use `azureV1` or `azureDeployment`. Only `azureDeployment` accepts non-null `deployment` and `apiVersion`. Enum values, field names, and unknown fields are checked strictly. Network and loopback authority are CLI flags, never profile JSON fields.

Import a credential through a private pipe from a trusted local secret supplier, never as a literal argument or checked-in file:

```text
<trusted local supplier writing only the token bytes> | yoke credentials model-import --name my-provider
```

The import is create-only and expects exactly the token bytes without a newline, limited to 2560 printable ASCII bytes. It returns only the credential reference and storage category. Then an explicitly authorized invocation is:

```text
yoke --root <captured-project> models-http --config <host-profile.json> --trust-network summarize
```

Tests use owned TCP/TLS listeners, generated certificates, and conspicuously synthetic callback credentials. They establish protocol behavior and admission boundaries, not real service availability or model quality. No real provider calls are required by the normal suite.

## Official contracts consulted

- [OpenAI Chat Completions](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create), [structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs?api-mode=chat), [embeddings](https://developers.openai.com/api/reference/resources/embeddings/methods/create).
- [Anthropic Messages](https://platform.claude.com/docs/en/api/messages/create), [structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs), [prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).
- [Azure API lifecycle](https://learn.microsoft.com/en-us/azure/foundry/openai/api-version-lifecycle), [dated REST Chat reference](https://learn.microsoft.com/en-us/rest/api/microsoft-foundry/azureopenai/chat), [structured outputs](https://learn.microsoft.com/en-us/azure/foundry/openai/how-to/structured-outputs).

Contract review date: 2026-10-15. Service/model capabilities still require explicit host selection.
