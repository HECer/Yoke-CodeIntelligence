# Host-authorized federation and signed snapshot synchronization

The unit of synchronization is an **immutable source snapshot**, never an open SQLite database. Importing does not check out files, change the receiving source tree or execute the sender's code. Imported snapshots are a separate `federated` kind and are queried only with their explicit ID. Indexes are rebuilt locally from that source. Existing analysis memories, editor sessions, arbitrary model caches, approval receipts and user accounts are not automatically synchronized.

## Configure trust explicitly

Copy `docs/examples/federation-host.json` and `sync-host.json` to a trusted location outside the source project. Replace absolute roots and key paths. Generate an **Ed25519** keypair with your normal key management tooling; retain the private key outside every indexed tree. A receiver only needs the expected signer's public key. The matching `{tenantId, projectId, keyId}` is part of the signature and receiver grant. No account is created, no provider subscription is required, and no keys are provided by the archive.

A random bearer token of at least 32 characters is supplied through the configured `tokenEnv`, never written into tracked examples. The daemon prints its endpoint but **does not print its bearer grants**. Each project must have a distinct canonical filesystem root/store. A single store is not a multi-tenant database. The host process itself is trusted; OS-level hostile-user isolation requires separate accounts/containers/instances.

```sh
node dist/cli.js federation --root /trusted/control-root --host-config /trusted/federation-host.json --port 0
node dist/cli.js sync export --root /project --host-config /trusted/sync-host.json --output /private/exchange.json
node dist/cli.js sync import --root /receiving-project --host-config /trusted/receiver-host.json --input /private/exchange.json
```

`export` refuses to overwrite an existing file and uses owner-only permission bits where supported. Windows ACLs are managed by the host, not implied by POSIX modes. The JSON export contains source code, so do not treat it as harmless metadata or upload it publicly. Excludes, obvious-secret detection, byte limits and receiving-policy restrictions apply; automatic secret detection is not exhaustive.

## Request protocol

`POST /v1` with JSON and `Authorization: Bearer <token>`:

```json
{"action":"find","projects":["one","two"],"query":"invoice","limit":12}
```

A caller must be granted every requested project before lookup. Results retain tenant/project/source identifiers and per-scope status. Missing scope coverage is reported, not silently treated as no matches. Querying an imported snapshot requires `snapshots:{"one":"<imported-id>"}`; signatures bind its original tenant/project. Export and import are separate permissions:

```json
{"action":"export","projectId":"one"}
```

```json
{"action":"import","projectId":"one","envelope":{"payload":{},"signature":"..."}}
```

Use the actual returned complete signed envelope, not this shape-only example. A host client file `{endpoint,tokenEnv}` can be passed to `sync request --host-config ... --input request.json`.

Remote, non-loopback endpoints require **TLS** and an explicit `allowedHosts` list. `tls:{certFile,keyFile}` is read only from the host config. The client refuses remote plaintext and redirects. Browser Origin requests are refused; this endpoint is for authenticated native clients, not arbitrary websites. The local editor endpoint has a separate token and cannot write source.

## Signature, replay and trust boundaries

Ed25519 signatures cover canonical serialized payload, source hashes, scope, issue/expiry dates and omissions. A maximum seven-day validity period is accepted. Modified, wrong-scope, expired, malformed or case-colliding sources are rejected before persistence. Re-importing the identical valid envelope is idempotent. Import does not imply that remote source is complete, safe, semantically correct or latest: an authenticated signer can send old source until expiry. Roll-forward/revocation policy is a host responsibility. Remove the relevant bearer grant/key and restart to revoke access; no identity provider or automatic rotation service is included.

Positive tests used real local HTTP servers, signatures, separate roots, denial cases and round trips. Internet/TLS deployment, hostile concurrent tenant load and an independent cryptographic/security audit were not performed here. Standard Node cryptography is used; no custom signature primitive was invented.
