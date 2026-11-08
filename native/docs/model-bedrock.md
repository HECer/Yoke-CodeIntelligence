# Native Bedrock CLI

`models-bedrock --config HOST.json --trust-network` uses the normal source-bound
model operations: `summarize`, `rerank`, `media`, `embeddings`, and `search`.
Converse supports the first three; embedding dialects support the last two.
An incompatible selection fails before source state or cost reservations.
No model invocation is enabled by default.

An explicit host profile contains no credentials or trust switches:

```json
{
  "endpoint": "https://bedrock-runtime.us-east-1.amazonaws.com/",
  "region": "us-east-1",
  "remoteModelId": "HOST_SELECTED_MODEL_OR_INFERENCE_PROFILE_ARN",
  "modelId": "host-model-namespace",
  "modelVersion": "host-pinned-version",
  "credentialName": "host-aws-session",
  "dialect": {
    "kind": "converse",
    "output": "jsonSchema",
    "maxOutputTokens": 1024,
    "vision": false
  },
  "taskId": "explicit-task",
  "currency": "USD",
  "maxCostMicros": 100000,
  "taskLimitMicros": 1000000
}
```

The values above are selectors and explicit budget ceilings, not a model
recommendation or price quotation. `output` is `jsonSchema` or `textJson`;
structured output must be supported by the host-selected model. Optional
`rootCertificatesDerBase64` adds bounded explicit DER trust roots. Plain HTTP
requires both `--allow-loopback-http` and a numeric loopback address with a port.
The parent pins the exact host profile SHA-256 in the managed child arguments;
the worker verifies it before reading credentials or sending requests.

Embedding dialect objects are `{"kind":"titanV1"}`,
`{"kind":"titanV2","dimensions":1024,"normalize":true}`,
`{"kind":"cohereV3"}`, or `{"kind":"cohereV4","dimensions":1024}`.
Dimension and text limits are validated by the native adapter. Titan accepts one
input per HTTP attempt; Cohere accepts at most 96. The worker never splits one
ledger attempt into hidden billable calls.

For corpus generation, explicitly add an `embeddingBatch` host policy:

```json
"embeddingBatch": {
  "maxCalls": 32,
  "maxEntities": 32,
  "maxTotalSourceBytes": 160000
}
```

`models-bedrock ... embeddings` then admits the complete selected corpus and
reserves its worst-case total cost before the first worker starts. Each chunk
has its own ledger attempt. The corpus and completed batch record publish in
one transaction after all chunks pass validation. Failures retain accounting
and source-bound response evidence without publishing partial vectors. The
policy is part of the exact pinned host profile. No batching is selected when
the field is absent; the existing single-request limits still apply.

`model-batch-status BATCH_ID` inspects an existing batch without launching a
provider or recapturing sources. A result that exceeds `--max-bytes` returns a
durable batch reference with the recovery command. See
[embedding-batches.md](embedding-batches.md) for admission, reconciliation,
resource limits and typed retention semantics.

## Windows credential import

Pipe private JSON into `credentials aws-import NAME`. The input fields are
`accessKeyId`, `secretAccessKey`, optional `sessionToken`, and optional
`expiresAtUnixMs`. Temporary session token and expiry must be supplied together.
Never put private material in host profiles, command arguments, source files,
shell history, or an echoed terminal. The CLI requires a private stdin pipe;
the secret DTO and owned buffers zeroize their material when dropped.

The dedicated namespace is `yoke-code-intelligence/service/aws-session/NAME`.
Versioned immutable chunks fit Windows credential blob limits; a final manifest
binds their generation, ordering, exact lengths, integrity and expiry. Reads
recheck the manifest after assembling the session. Import is create-only;
rotation uses a fresh credential name and an updated host profile. Interrupted
publication may leave unpublished OS chunks, and is reported as such. This is
not a transaction spanning multiple Windows Credential Manager entries.
The 64 KiB material ceiling is a local resource policy, not an AWS token maximum.

The adapter performs explicit SigV4 signing with service `bedrock`. It does not
discover AWS profiles, environment keys, IMDS credentials, SSO sessions or STS
refresh flows. Request-selected model identity is not a returned weights
attestation. Reported usage is retained; missing usage and monetary charges stay
unknown. Every managed attempt uses the existing parent ledger and conservative
unknown-billing reservation. There are no automatic retries.

Windows tests cover owned TCP/TLS, independent signature reconstruction,
hostile protocol responses, chunk integrity and the actual CLI with a 4,200-byte
synthetic temporary token. No real AWS service or real AWS credentials were
used. Live AWS acceptance and Linux/macOS credential runtime checks remain open.
