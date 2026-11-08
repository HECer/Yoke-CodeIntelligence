# Received PostgreSQL catalog

The native `postgres-catalog` host command captures selected PostgreSQL metadata
through an explicitly trusted, SHA-256-pinned `psql` 18 installation. Rust owns
the fixed SQL, permissions, process lifetime and bounded pipes, validation,
immutable artifact, graph projection and frozen queries. This is an optional
host adapter, not a Graphify application-code wrapper or a default dependency.

Capture needs `--trust-client --allow-network --allow-output`. Plaintext trust
also needs `--allow-loopback-trust` and a literal loopback IP. TLS mode requires
an explicit server name, CA PEM, `verify-full`, SCRAM and channel binding; its
password is read from the OS store under a hash of the endpoint and TLS policy.
The separate `import-password --allow-credential-create` command accepts exact
private stdin bytes, never a password argument. Passwords contain 1–2560 UTF-8
bytes without control characters. On Windows the client receives them through
its private stdin prompt with PostgreSQL18's explicit non-console fallback,
avoiding lossy narrow argument/environment conversion. Other hosts use a private
child UTF-8 environment, with prompting disabled. Neither route uses password
arguments or password files.
Configuration and artifacts
cannot grant these permissions. No ambient `PG*`, service/password file,
psql profile or default client certificate supplies credentials.

The host configuration is a closed JSON object:

```json
{
  "client": "C:\\host-tools\\pgsql\\bin\\psql.exe",
  "clientSha256": "<64 lowercase hexadecimal characters>",
  "address": "127.0.0.1",
  "port": 5432,
  "database": "example",
  "user": "catalog_reader",
  "security": {"kind": "loopback_trust"}
}
```

For TLS replace `security` with `{"kind":"tls_scram", "serverName":"db.example",
"rootCertificatePem":"-----BEGIN CERTIFICATE-----\\n..."}`. The IP controls the
network destination; the server name controls certificate verification. There
is one endpoint, with no supplied URL/DSN, multi-host or DNS expansion from data
fields. Rust constructs a fixed-key ASCII libpq URI with individually percent
encoded UTF-8 values, so quotes, equals signs and Unicode remain literal values.

```sh
yoke postgres-catalog capture --config /absolute/host-config.json \
  --output /absolute/fresh-catalog.json --trust-client --allow-network \
  --allow-output --allow-loopback-trust
yoke --max-bytes 1000000 postgres-catalog read --input /absolute/fresh-catalog.json
```

Capture checks the destination and delivery budget before executing a client;
publication is create-only. Existing files and links are preserved. Lost stdout
after publication retains the file and reports its catalog/archive identity;
credential-creation delivery failures similarly retain the binding. The host
trusts the selected installation and runtime libraries. The executable hash
does not attest DLLs, the server, libpq internal allocations or server memory.
Windows holds executable/ancestor handles through execution; this is not an
OS sandbox or protection against a hostile equal-privilege host.

Fixed catalog reads run in a serializable, read-only, deferrable transaction
with statement/lock/idle timeouts, a `pg_catalog` search path, row quotas and
rollback. No application table contents, stored routine execution, user SQL,
model call or view deparsing occurs. PostgreSQL 14–18 metadata is accepted;
actual versions tested are recorded separately in validation evidence.

Tables, views, materialized views, foreign/partitioned tables and routines keep
catalog OIDs scoped to this received response. Overloads stay separate.
Foreign keys are directed declared constraints identified by constraint OID,
including parallel constraints and ordered composite column pairs. Columns
are metadata, not nodes. A target outside the selected relation set is an
explicit `unselected_relation_reference`. Names are not semantic identities.

Definitions may be hidden or exceed a cell quota. Such fields are null with
an explicit status. Visible routine bodies retain stored `prosrc` or opaque
`pg_node_tree`; visible views retain opaque server parse trees. These are not
original DDL/SQL source bytes. No synthetic placeholder SQL is substituted.
Evidence offsets address the original received UTF-8 JSONL record bytes only.

Default hard limits are 16 MB response, 1 MB record, 4096 relations, 4096
routines, 8192 foreign keys, 16384 nodes, 8192 observations, 65536 bytes per
definition and 32 MB archive. The optional closed `--limits` object can lower
them. Overflow refuses the capture; it does not return clipped complete data.

Archives preserve response bytes and hash and reconstruct the graph on load.
Capture claims are nested, unverified data. Projection network counters describe
received-byte reconstruction and frozen queries; live capture is separately
authorized network activity. A forged claim never grants server,
TLS, source, credential or execution authority. `complete` remains false.
Frozen `read --query` supports `inspect`, `node`, `degree` and outgoing `trace`
with explicit depth/maxNodes bounds. Parallel observations and direction remain
intact. Reads never run psql, look up credentials or contact a database, and do
not open a source Store or establish a canonical code SourceSnapshot.

Primary protocol references: [libpq connection parameters](https://www.postgresql.org/docs/18/libpq-connect.html),
[psql options](https://www.postgresql.org/docs/18/app-psql.html) and
[TLS verification](https://www.postgresql.org/docs/18/libpq-ssl.html).
