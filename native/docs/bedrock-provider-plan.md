# Bedrock native provider plan

Status: **native worker, Windows CLI/session storage, and bounded parent-managed embedding batches implemented**. `model_bedrock` adds an explicit Rust library worker alongside OpenAI-compatible, Anthropic Messages, and Azure. Twelve owned worker tests and seven independent review tests pass. Separately invoked Windows CLI gates cover a summary and a three-attempt Titan embedding batch using fake OS credentials and owned loopback HTTP servers. Explicit host attempt/entity/source limits and vector-store admission still apply. This document does not certify real AWS service availability, IAM permissions, model access, quality, billing, or region/model support. Research date: 2026-10-15. No AWS/model calls, paid services, or real credentials were used.

## Authority and proposed shape

Use a separate non-deserializable Rust `BedrockSpec`: explicit trusted-network authority, exact endpoint/region, remote model ID or ARN, native model/version namespace, credential reference, typed wire dialect, model-specific capabilities, JSON mode, and resource budgets. Keep the existing parent authenticated source capture and local source-binding protocol. Worker path labels remain data and confer no filesystem authority.

A host callback returns zeroizing access-key ID, secret key, optional session token, and expiration; secret-bearing structures must not derive Debug or Deserialize. No implicit AWS environment variables, shared profiles, IMDS, SSO, STS requests, refresh, discovery, default region/model, or network access. Expired/currently revoked host credentials fail before signing/sending. TLS validation, explicit owned loopback fixture grants, no redirects/proxies, and no hidden retries remain required.

Converse does not return a standard model identity field. The remote model ID therefore binds the outgoing request, rather than becoming a fabricated server attestation. A short native namespace remains distinct from a potentially long remote ARN. Model/profile support is host selected and must not be guessed from provider labels.

## SigV4

Sign the exact admitted outgoing URL and serialized body with service `bedrock`, region, UTC date, and `AWS4-HMAC-SHA256`. Bind host including any actual nondefault port, content type, payload SHA-256, `x-amz-date`, and the temporary security-token header. Canonical URI encoding, sorted encoded query pairs, normalized sorted headers, credential scope, and the date/region/service HMAC chain require official test vectors and an independent owned TCP/TLS verifier. ARN path separators, colons, percent encoding, and URL normalization must be tested explicitly.

The worker uses exact `aws-sigv4 = 1.6.0` and `aws-credential-types = 1.3.0` with Rust 1.94.1. Signing enables only `sign-http` and its required `http1` support; no full runtime SDK or credential-discovery chain is installed. The `sign-http` feature alone was experimentally rejected by this upstream version's unresolved HTTP imports. Signatures use the exact admitted request bytes, double-encoded canonical URI, signed payload checksum, and included temporary session token. AWS documents SDK retries for some Bedrock errors; the worker disables request retries and makes one attempt.

Sources: [AWS SigV4 signing](https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html), [Bedrock service signing example](https://docs.aws.amazon.com/bedrock/latest/userguide/inference-chat-completions.html).

## Converse

Use `POST /model/{encodedRemoteModelId}/converse`, system text blocks, a user message with admitted source data, and explicit `inferenceConfig.maxTokens`. Structured output currently uses `outputConfig.textFormat` with `type: json_schema` and `structure.jsonSchema`, whose `schema` is a JSON **string**. Supported schema subsets and model-specific additional strict fields require typed host capability selection; unsupported models can use explicitly selected text JSON, never an implicit tool-call workaround.

Accept only a complete assistant text result with `stopReason: end_turn` and locally validated source IDs/schema. Tool use, filtering, refusals, malformed or truncated output remain bound errors, preserving valid reported usage. Optional images require separate model capability and admitted byte/format/size checks; no S3 fetch authority is implied.

Native total input is the checked sum of uncached `inputTokens`, `cacheReadInputTokens`, and `cacheWriteInputTokens`; native cached input is cache-read tokens. Preserve the raw distinction internally for future reconciliation. These counters are not a monetary receipt; actual cost remains unknown. Cache-write pricing cannot be represented by inventing a uniform cached-token price.

Sources: [Converse API](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_runtime_Converse.html), [structured outputs](https://docs.aws.amazon.com/bedrock/latest/userguide/structured-output.html), [token fields](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_runtime_TokenUsage.html), [cache accounting](https://docs.aws.amazon.com/bedrock/latest/userguide/prompt-caching.html).

## Embedding dialects and attempt accounting

InvokeModel uses `POST /model/{id}/invoke`, JSON input, and a documented 25,000,000-byte request/response ceiling. Enforce the lower of that boundary, local budgets, and the specific model's limit before allocation/signing.

| Dialect | Input | Output/admission |
| --- | --- | --- |
| Titan G1 | One `inputText` | `embedding`, reported `inputTextTokenCount` |
| Titan V2 | One `inputText`, explicit normalization/dimensions/float type | Dimensions 256/512/1024; float embedding and reported input count |
| Cohere V3 | Up to 96 texts, `search_document` or `search_query`, no truncation | Ordered float vectors, dimension 1024 |
| Cohere V4 | Up to 96 texts, explicit input type/dimension, no truncation | Float response variants, dimensions 256/512/1024/1536 |

Validate exact vector count/order, finite coordinates, nonzero norms, expected dimensions, and aggregate coordinate/byte budgets. A model response lacking documented token usage keeps usage unknown. Do not estimate tokens from character counts or fabricate service metadata.

The native embedding request can contain up to 2000 sources, while Titan permits one text and Cohere permits at most 96 per call. Workers refuse oversized single-wire requests. The optional parent-managed [embedding batch path](embedding-batches.md) partitions only after complete source, sink, dimension, byte and worst-case cost admission. It atomically reserves each separate attempt, preserves individual reported/unknown accounting, and publishes a complete normalized corpus and completed batch record together. No worker silently fans out behind one ledger attempt.

The implemented worker refuses Titan batches larger than one and Cohere batches larger than 96 before reading credentials. Cohere V3 also uses a conservative local admission of 512 Unicode scalar values per text; this is not a token-count claim. `truncate: NONE` allows the service to reject input that exceeds its actual tokenizer limit. It never silently drops text. Titan G1 requires 1536 output coordinates; other dimensions follow the exact selected dialect. All results require finite nonzero vectors and exact count/order. Cohere V3 accepts the documented array form and the explicit `embedding_types: [float]` object form; V4 requires its float object.

Sources: [InvokeModel](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_runtime_InvokeModel.html), [Titan formats](https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters-titan-embed-text.html), [Cohere V3](https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters-embed-v3.html), [Cohere V4](https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters-embed-v4.html).

## Windows credential sessions

Windows Credential Manager permits at most 2560 bytes per credential blob. AWS STS explicitly warns that its session-token maximum is not fixed, so the existing single model-token entry cannot universally store an AWS session. Never truncate a token or copy it into repository/profile/state files.

Proposed root-owned storage: a dedicated versioned AWS-session namespace with immutable bounded chunks and a final create-only OS credential manifest. The manifest binds generation, exact chunk count/lengths/hash, credential type, and expiration. Publish it only after every chunk is written; readers reject partial, mixed, unknown, expired, or revoked generations and reread the current host reference per invocation. Cleanup failures must remain visible. All reconstructed material is zeroizing RAM. An implementation ceiling such as 64 KiB is a **local resource admission limit, not an AWS maximum**; larger sessions must be refused honestly. Windows logon-session persistence and AWS credential expiration are different concepts.

The Windows implementation uses the dedicated `yoke-code-intelligence/service/aws-session/{name}` manifest slot and immutable `/chunk/{generation}/{index}` entries. `YAWS1` manifests bind a random 16-byte generation, exact payload length, chunk count, SHA-256, and expiration. Every `YAWC1` chunk includes generation/index/count and at most 2375 payload bytes, for a total OS blob at most 2400 bytes. A zeroizing binary payload retains exact material rather than a generic JSON value. Readers validate admission before allocating, verify every chunk and whole-payload hash, check exact expiry binding, and reread the current manifest after reconstruction. Interrupted or concurrent publication may leave unpublished chunks; explicit errors report that possibility. This is not an atomic transaction over Windows Credential Manager entries. A late OS-write/deadline failure can leave a published manifest, so a failed import does not authorize overwriting it. Rotation uses a new explicit host credential name; automatic replacement or orphan removal is not implemented. Non-Windows session storage explicitly refuses access.

The worker's `AwsSession::new` accepts zeroizing strings, requires temporary token and expiration together, rejects expired sessions and header control characters, and bounds aggregate material to 64 KiB locally. Credentials are read through `Fn(&str, &Deadline) -> Result<AwsSession>` only after pure host, source, operation, batch, and request-byte admission. Expiration is checked during construction, signing, and immediately before sending. The official signer also owns zeroizing credential copies. HTTP libraries necessarily hold request-header copies; the API does not claim to erase every transport allocation or provide a hard in-process cancellation boundary.

## Rust entry point

Construct `model_bedrock::BedrockSpec` in trusted host code, call the pure `validate_spec`, then call `provider_reply` with exactly one native model protocol request, a host credential callback, and an absolute `Deadline`. The public dialects are `Converse { output: Output::JsonSchema | Output::TextJson, max_output_tokens, vision }`, `TitanV1`, `TitanV2 { dimensions, normalize }`, `CohereV3`, and `CohereV4 { dimensions }`. No authority-bearing type is deserializable. An endpoint may include a trusted base path but cannot include credentials, query, fragment, escaped path, or dot segments. The remote model selector is encoded as one URI segment, including ARN colons and slashes. Outgoing query parameters are deliberately unavailable.

Native source bindings are local protocol bindings, not remote model attestation. Valid reported token usage survives refused/incomplete output; malformed usage remains unknown. Real monetary cost is always unknown. Errors never return service error bodies, credentials, signature details, or unvalidated generated content. Both outer response strings/keys and decoded assistant JSON strings/keys are checked for access-key, secret-key, or temporary-token echoes.

`host_credentials::import_aws_session_json(name, bytes, deadline)` accepts only `accessKeyId`, `secretAccessKey`, optional `sessionToken`, and optional `expiresAtUnixMs`. The two optional fields must either both be absent/null or both contain valid values. The parser rejects duplicate/unknown fields, uses custom zeroizing string deserialization, and limits the private input to 70,000 bytes. `import_aws_session` takes an already constructed host session. `read_aws_session` reconstructs the current session with deadline checks on each chunk. Results expose neither secrets nor secret hashes.

The CLI imports this JSON from a private stdin pipe through `credentials aws-import NAME`; never put material in arguments or a profile file. `models-bedrock --config HOST_PROFILE --trust-network ...` pins the exact host profile bytes into its managed worker. `--allow-loopback-http` separately admits owned numeric loopback fixtures. The worker rereads OS material for the admitted invocation; no AWS profile or environment credential fallback exists. See the strict host-profile field definitions in `native/src/bedrock_cli.rs`.

The manual gate `actual_cli_bedrock_summary_uses_chunked_session_and_one_accounted_attempt` uses a synthetic 4200-byte token, checks create-only publication and reconstruction, sends one signed summary request to an owned TCP server through the actual executable, verifies captured source identity/text and reported cache counters, and checks one unknown-billing ledger reservation. It then removes only that unique test generation's two chunks and manifest. This checks local Windows integration, not AWS acceptance or price accuracy.

Sources: [Microsoft CREDENTIALW limits](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw), [AWS AssumeRole token-size warning](https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html).

## Acceptance gates before support claims

- Official signature vectors and independent owned verifier; exact body/URI/region/service/date/token bindings, including ARN encoding.
- Pure preflight with no credentials/DNS/network; fake credential callback and generated TLS roots only.
- Synthetic session above 2560 bytes, partial/mixed-generation refusal, expiry/revocation/rotation, and cleanup reporting.
- Every selected wire dialect's positive and hostile response shapes, usage overflow, refusal/truncation, source binding, credential-echo escapes, and preallocation budgets.
- Parent ledger per-attempt batching, one-attempt failures, unknown billing, partial results, cancellation and shutdown.
- Legacy providers/Ollama/OpenAI-compatible/Anthropic/Azure tests unchanged; no real service calls in the default suite.

Passing fixtures demonstrates native protocol behavior. Real AWS acceptance remains separately unverified unless explicitly authorized and observed later.

The additional actual Windows batch gate creates three captured Rust functions and explicitly selects `embeddingBatch: { maxCalls: 3, maxEntities: 3, maxTotalSourceBytes: 15000 }` with Titan V2 dimension 256. It observes three distinct signed TCP requests through three managed workers, validates the exact source/entity set, verifies one completed legacy vector collection and one completed batch record, and confirms three unknown-cost reservations totalling 30 local micros. Only the unique fake credential generation is removed afterward. This gate passed; it is not a live AWS or pricing certification.
