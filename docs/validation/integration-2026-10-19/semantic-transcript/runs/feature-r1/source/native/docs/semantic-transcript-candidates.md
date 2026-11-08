# Explicit semantic transcript candidates

`knowledge::feedback::semantic_transcript` and `feedback semantic-mine` make one
explicitly trusted host-provider attempt over a selected contiguous span of an
already imported original UTF-8 transcript. The span may cover the entire import
(at most 1,000,000 bytes). No context within that selection is silently clipped.
The original transcript remains untrusted data, including any instructions in it.

These are model-inferred candidates for host review. They create no knowledge,
plan or Meaning outcome events and establish no outcome truth, corroboration,
current applicability or observed execution. Duplicate candidate observations
and ordered repeated quotes remain distinct. Original ordinals qualify candidate
identities. A later event still requires its existing independent source proofs,
host-selected subject and explicit authored input.

## Host CLI

```text
yoke --root REPOSITORY --allow-writes feedback semantic-mine --request REQUEST.json --config PROVIDER.json --policy POLICY.json --trust-provider
yoke --root REPOSITORY feedback semantic-status --job JOB_ID
yoke --root REPOSITORY feedback semantic-load --job JOB_ID
yoke --root REPOSITORY feedback semantic-jobs --task TASK_ID --offset 0 --limit 25
```

Write and provider permissions precede host-file reads and Store access.
`semantic-mine` returns a small durable job reference. Keep its `jobId`; inspect
status under a fresh deadline if publication or stdout delivery is interrupted.
There is no automatic retry, resume or cache reuse. Another explicit build is
a new nonce-bound job and attempt. Read commands accept no provider configuration
and start no provider. Existing literal `feedback mine` is unchanged.

`REQUEST.json` is closed JSON (duplicate and unknown fields are rejected):

```json
{
  "schemaVersion": 1,
  "source": {
    "kind": "transcriptUtf8",
    "transcriptId": "IMPORTED_TRANSCRIPT_SHA256",
    "contentHash": "ORIGINAL_UTF8_SHA256",
    "startByte": 0,
    "endByte": 1234
  }
}
```

Offsets are absolute UTF-8 byte offsets into the registered import, with an
exclusive end and both boundaries on complete characters. Transcript IDs and
bytes never become native file paths, code snapshot IDs or source authority.

The closed provider file uses `command`, optional `args`, `modelId`,
`modelVersion`, `taskId`, `currency`, `maxCostMicros`, `taskLimitMicros`. It has
no JSON trust field. The closed policy has `contractHash` (a SHA-256 host contract
identity), optional absolute `providerInputFiles` for fingerprinting, and optional
`maxResultBytes` (default 4,000,000; supported range 4,000,000..16,000,000).
Host `feedback` input/source/output/candidate limits can lower capacity and
refuse the whole request or result. CLI output is limited to 4,000,000 bytes.

The configured executable and declared host inputs are fingerprinted using the
existing native host-provider checks. The process runs in an owned temporary
directory. Explicitly trusted providers have host-account access: filesystem or
network isolation is not enforced. `sourceWritten:false` applies to the native
parent, as the report's `sourceWriteScope:native_parent_only` states.

## Owned wire contract

One bounded JSON request on stdin uses schema 1 and operation
`extract_transcript_feedback_v1`. It carries `model`, `modelVersion`, `requestId`,
`inputHash`, `currency`, `taskId`, `costLimitMicros` and `payload`. The payload
contains the exact selected `text`, original transcript header, `source`,
workspace/contract identities, `candidateLimit`, `omittedBytesWithinSelection:0`
and the actual number of `unselectedTranscriptBytes`.

The single closed JSON reply repeats `schema`, `operation`, `model`,
`modelVersion`, `requestId`, `inputHash`, `currency` and exact `sourceBinding`.
It may include `actualCostMicros`, `usage`, `data` and `error`. Missing/null cost
means unknown, never zero. Usage contains `inputTokens`, `outputTokens` and
`cachedInputTokens`. The data fragment is:

```json
{
  "candidates": [{
    "outcome": "success",
    "interpretation": "Unverified model interpretation for host review.",
    "sourceEvidence": [{
      "transcriptId": "IMPORTED_TRANSCRIPT_SHA256",
      "contentHash": "ORIGINAL_UTF8_SHA256",
      "startByte": 20,
      "endByte": 24,
      "quote": "text"
    }]
  }],
  "coverage": "candidates"
}
```

Outcomes use the existing `success`, `failure`, `inconclusive`, `corroboration`
or `contradiction` labels. Every quote must match exact selected original bytes.
Empty candidates require `coverage:no_candidates`; this is the provider's
bounded response, never a claim of complete semantic understanding. Limits:
128 candidates per job (or fewer under the host cap), 2,048 interpretation bytes,
1..8 quotes per candidate, 16,000 bytes per quote, 128,000 quoted bytes across
all occurrences. Payload, raw reply and fragment limits are respectively
8,000,000, 2,000,000 and 1,000,000 bytes. Oversized data is refused, not clipped.

## Durable accounting and replay

The single-use ledger reservation, payload registration and job intent commit
together before dispatch. No transaction spans the provider process. The bound
original reply and responded phase are published before domain validation;
checked candidates publish atomically under exact previous-row preconditions.
Fully bound valid costs survive invalid usage/data, provider error, nonzero exit
and reservation overrun. An overrun is recorded in full and blocks subsequent
task sends. An unbound/incomplete response keeps its unknown reservation; only
a durably never-sent attempt may be cancelled. Failures name the durable job.

`semantic-load` replays the registered original import/header/CAS, exact payload,
reply envelope, task/attempt ledger binding and reconstructed result. Tampering,
capacity overflow or missing dependencies refuses a checked capability.
`CheckedCandidates` has no JSON constructor; its bounded immutable export also
works after Store loss. It contains original response billing. Later ledger
reconciliation is separately labelled in status and never upgrades an original
unknown cost/usage. `semantic-jobs` checks all admitted job headers, reports
pagination totals and explicitly does not claim dependency replay.

Typed retention validates complete, failed and in-flight jobs before recognizing
their transcript/payload/reply/result roots. Arbitrary strings grant no roots.
Owned Node fixtures qualify protocol/accounting/replay behavior; they establish
no real model quality, external billing verification or configured live-provider
acceptance. Broader outcome surfaces, independent corroboration and actual
execution evidence remain separate work.
