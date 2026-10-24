# Native signed offline source federation

The transfer unit is an immutable UTF-8 source envelope. No live SQLite database, private signing key, approval receipt, editor session, model cache or user account is transferred. Import writes only the receiver's private native store and rebuilds structural analysis locally. It never checks out source files or executes sender code.

This module supplies bounded offline exchange and explicit remote queries. [The separate native HTTP/TLS module](federation-http.md) transports these same signed envelopes with host-controlled roles/grants and revocation. Hosted identity management and multi-user operating-system isolation remain separate concerns.

## Host APIs and trust

All functions return `anyhow::Result`; JSON-facing results use `serde_json::Value`.

```rust
export_snapshot(&Store, &Snapshot, &ExportScope, &[u8; 32], &Deadline)
import_snapshot(&Engine, envelope: Value, &ImportGrant, &Deadline)
list(&Store, &Deadline)
read(&Engine, remote_id: &str, path: &str, &ImportGrant, &Deadline)
snapshot(&Engine, remote_id: &str, &ImportGrant, &Deadline) // -> Snapshot
graph(&Engine, remote_id: &str, &ImportGrant, &Deadline)    // -> Graph
```

`Scope` contains `tenantId`, `projectId`, `senderId` and `keyId`. `ExportScope` adds `ttlMs`; `ImportGrant` contains the exact expected scope, a lowercase hexadecimal Ed25519 `publicKey`, the expected `senderWorkspaceId`, and optional receiver `exclude` globs. Scope labels are bounded ASCII identifiers.

These are trusted host inputs. A remote envelope cannot supply its own grant, key or privileges. Host exclude patterns are denial rules with case-insensitive matching; negation cannot re-enable denied paths. Imported source comments, manifests, `.gitignore` text and configuration files remain data.

The library borrows a 32-byte private seed directly from the trusted caller. The private seed is never serialized, logged or persisted. The pinned signing library's default `zeroize` feature clears its local `SigningKey` copy on drop; the caller remains responsible for its own seed-buffer lifetime. Production callers must obtain secrets from the host's protected credential store. Private seeds must not be placed in command arguments, environment examples, source files or JSON configuration. Tests generate ephemeral keys in memory.

## Signed envelope

The strict envelope contains `{manifest, signature, contents}`. The manifest includes schema, complete scope, sender workspace, source snapshot ID/kind, analyzer identifier, random nonce, issue/expiry milliseconds, sorted source-file metadata and sender omissions. Each file declares relative path, exact SHA-256 content hash, UTF-8 byte size, permission mode and source-language label. `contents` supplies one canonical standard-base64 payload for each manifest path in the same order.

Ed25519 signs the domain prefix `YOKE-SOURCE-FEDERATION-V1\0` followed by compact UTF-8 JSON with recursively sorted object keys. Array order is preserved and file arrays must already be sorted by exact path. Source bytes bind to the signed hashes and sizes; content payloads cannot be changed independently of the manifest.

The implementation uses pinned BSD-3-Clause [ed25519-dalek 2.2.0](https://docs.rs/crate/ed25519-dalek/2.2.0/source/Cargo.toml.orig), including its [strict verification API](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/struct.VerifyingKey.html#method.verify_strict). Weak keys and noncanonical signature/key encodings fail. This is a standard cryptographic library, not a newly invented signature primitive or an independent security certification.

Export requires the exact canonical stored local snapshot and refuses implicit re-export of remote source. Accepted local kinds are `disk`, `derived`, `overlay` and the compatibility label `editor`. A captured editor overlay transfers its immutable source bytes, including unsaved buffers; the live editor session and saved workspace files are unaffected. Validity is at most seven days. Imports reject expired intervals, issue times more than five minutes in the future, wrong scope, wrong workspace, wrong key and invalid signatures.

## Receiving policy and immutable namespaces

Every manifest path passes native relative-path, portability, protected-credential and link/reparse checks. Duplicate paths and Unicode/case aliases are rejected. Binary/NUL-containing data, invalid UTF-8, incorrect hashes/sizes and obvious credentials fail even when signed by an authorized key.

Receiver policy uses the same configured ignore walker as saved-source indexing: ancestor/root/nested `.ignore` and `.gitignore` precedence, Git excludes, hidden files enabled, no link following, no global Git ignore and no Git-repository requirement. Its incremental matcher also checks nonexistent imported paths. Linked ignore files, unsupported external Git directories, malformed policy rules and policy limits fail explicitly. A nested whitelist cannot override a higher-priority `.ignore` or reopen an ignored parent directory. Signed remote policy text never enters the receiving matcher.

Accepted state lives in separate SQLite record namespaces:

| Namespace | Contents |
|---|---|
| `native_federation_receipt_v1` | Signed manifest, current import key attribution, receiver identity, immutable remote snapshot and local graph checksum. Source hashes are retention roots. |
| `native_federation_graph_v1` | Structural graph rebuilt by native local analyzers. |
| `native_federation_replay_v1` | Nonce-to-manifest/remote-ID binding. |

Ordinary `Store.snapshot(remoteId)` does not return remote snapshots. Their IDs bind receiver workspace, sender scope/workspace, source snapshot and manifest identity. Import does not move the ordinary local `latest` pointer. Remote FTS rows use the explicit remote ID. `federation::find` rechecks the current grant and receiving policy, then supplies a temporary read-only source capability to the shared search algorithms. Native query entrypoints still require native manifest proofs; passing a foreign snapshot to them does not grant access. Symbol bodies and text matches use the same bounded, hash-checked source reads. `skipIgnored` never overrides the receiver's current ignore policy.

Reimporting an identical still-valid envelope is idempotent. Reusing a nonce for a different signed manifest fails. Distinct fresh envelopes may describe older source: signatures and timestamps do not establish latest remote state or automatic roll-forward policy.

Every explicit source read rechecks the current host grant, pinned key, signed manifest, source hashes and receiver policy. Historical accepted receipts may remain readable after their import-validity interval expires when the current host grant still authorizes them; listing reports `importValidityExpired`. Removing or replacing the grant/key prevents those source queries. Local graph checksums reject altered cached graphs rather than returning a successful empty analysis.

## Bounds and evidence limits

Limits are 2,000 source files and 2,000 recorded omissions, 4 MB per file, 16 MB decoded source, 32 MB signed envelope and 64 MB per persisted receipt/graph. The remote namespace is bounded to 256 receipts, 512 MB of declared source sizes and 512 MB of serialized receipts/graphs. Graph bounds are 50,000 entities, 100,000 observations and 200,000 evidence records. Receiver policy permits at most 1,000 checked directories, 8 MB of policy text and bounded per-file rules. Database transactions and parser work share the caller's absolute deadline. SQLite lock waits are refreshed to the remaining budget immediately before transaction admission and commit, so prior decoding and analysis do not extend the import's timeout.

All files are validated before any source blob is persisted. Receipt, graph, replay binding and remote FTS rows commit together. A failed transaction can leave unreferenced content-addressed blobs for normal retention cleanup, while original source remains untouched.

A valid signature authenticates supplied bytes and scope. It does not verify source safety, completeness, remote authorship beyond key possession, compiler semantics, runtime coverage or freshness. The locally rebuilt graph retains native syntactic/heuristic/unresolved classifications and `exhaustive: false`.

Actual Windows behavioral tests use real Ed25519 keys and signatures. They cover Unicode round trips, the real editor open/update/capture lifecycle, source preservation, canonical snapshot enforcement, remote re-export refusal, namespace separation, idempotency, tampered envelopes, validly signed malicious sources, scope/workspace/key denial, nonce conflicts, expiry, current-grant revocation, receiver ignore precedence, case-deny rules, capacity accounting, corrupt graph refusal and actual SQLite writer contention after large-source preprocessing. No network transport or independent cryptographic audit is claimed by these tests.
