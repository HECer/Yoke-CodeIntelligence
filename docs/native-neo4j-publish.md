# Native Neo4j publication (0.5.0 development)

The explicit `yoke neo4j-publish` host workflow publishes an existing saved native
base graph through Neo4j Query API v2 and fixed parameterized Cypher 5. It does
not execute a received Cypher export, recapture the workspace, invoke a model,
or execute analyzed project code. Preparing or inspecting a plan is offline.
Live credentials and guarded publication currently support **Windows**.

This is a scoped connector, not full upstream or platform certification.
Neo4j Community 2026.09.0 is the actual development-server profile. Its owned
fixtures use JBR 25.0.3; that functional evidence does not qualify the JVM vendor
against Neo4j's supported-vendor table. Enterprise RBAC, other server releases,
Bolt and non-Windows publication remain separate work. FalkorDB now has its own
[separately qualified native workflow](native-falkor-publish.md).

Use a host-selected target file. For example, the following documentation-only
address requires an actual host-selected endpoint and public trust anchors:

```json
{
  "baseUrl": "https://graph.example.invalid:7473/",
  "address": "203.0.113.18:7473",
  "database": "neo4j",
  "namespace": "reviewed-code-graph",
  "user": "publisher",
  "useSystemRoots": true,
  "rootCertificatesDerBase64": []
}
```

The URL must have only a scheme, host and port. Credentials in URLs, paths,
queries, fragments, control characters, nonliteral remote plain HTTP and the
system database are refused. The supplied socket address is pinned; HTTPS still
checks the original URL hostname. Public platform roots are extracted once and
verified offline by WebPKI. Explicit added DER roots are supported. There is no
proxy, redirect, automatic retry, credential challenge forwarding or DNS
fallback. Plain HTTP requires a literal loopback address and the independent
`--allow-loopback-http` host flag.

Import exact private UTF-8 password bytes through a private stdin pipe with
`import-password --config <absolute-target-path> --allow-credential-create`.
Never put a password in arguments, configuration, a plan, logs or source files.
Imports are create-only in Windows Credential Manager and bind the password to
the canonical URL, pinned socket, database, username and TLS policy. A terminal,
control characters, invalid UTF-8, an oversized input and an open pipe past the
original deadline are refused. A credential change is an explicit host action.

The normal sequence is:

1. An administrator prepares `bootstrap-plan --config <target> --output <plan>
   --allow-output`. The five fixed constraints/indexes and fresh namespace are
   separate journaled operations. No schema is changed during preparation.
2. Inspect the exact plan with `inspect --plan <plan>`. Review its ID, endpoint,
   trust policy, namespace and schema requirement. A serialized plan grants no
   host capabilities and imported source claims are not attested.
3. Record explicit approval with `approve --plan <plan> --expected-plan-id <id>
   --journal <absolute-parent>/neo4j-<id>.jsonl --allow-approval --allow-journal`.
   The parent must already exist. The journal must be fresh. Approval defaults
   to ten minutes and `--valid-for-ms` can select at most one hour.
4. Apply the bootstrap with `apply --plan <plan> --expected-plan-id <id>
   --journal <journal> --allow-network --allow-credential-lookup --allow-write
   --allow-schema --allow-journal`. Add the loopback flag only for that profile.
   Existing incompatible named schema is refused, without destructive repair.
5. Prepare publication with `prepare --config <target> --source-root <root>
   --snapshot-id <saved-id> --expected-revision absent --output <plan>
   --allow-output --allow-source-coordination`. Source coordination permits
   SQLite WAL coordination for the existing Store; missing state is refused.
   Later disk changes do not replace the saved source. Use the observed prior
   revision ID instead of `absent` when preparing an update.
6. Inspect and approve the publication plan separately, then apply it with the
   network, credential, write and journal flags. Schema permission is not needed
   for publication. Required indexes must already be online; the publisher does
   not silently run an administrator bootstrap.

`probe` performs only fixed schema/namespace reads with explicit network and
credential grants. Different administrator/publisher users share a logical
namespace but have separate credential bindings. Server permissions remain
authoritative; host flags do not grant database privileges.

Publication is one implicit transaction. Scoped revision keys preserve every
entity, evidence row, original producer/resolution label, directed parallel
observation and unresolved target. The original row JSON, source hashes and
byte ranges remain available. Null targets use explicitly marked unresolved
nodes and do not become resolved entities. Coverage and diagnostics remain in
the immutable revision header. This is data export, not compiler, call,
execution, ownership or source authority. Semantic completeness remains false.

The transaction guards the expected active revision, immutable known row
properties and actual scoped row counts. A conflict rolls back the entire
transaction. Replaying the same revision preserves its logical data; the
namespace's coordination counter can advance. Existing revisions remain
immutable and retained. No other namespace is deleted or replaced. Lowerable
quotas refuse the whole payload rather than truncate it. The defaults allow
5,000 entities, 50,000 observations, 50,000 evidence rows and 16 MB of payload;
encoded plans/requests/responses also have independent hard bounds.

Every write has a durable intent before send. HTTP 202 is insufficient: the
client checks Query API errors, response shape, write/schema query type, bounded
post-commit bookmarks and the exact expected receipt. An open-transaction
response cannot confirm an implicit write.
Any sent operation without a matching confirmed receipt remains pending,
including transport, protocol, rejected-query or receipt failures. Repeating
`apply` cannot reset or resend that pending intent. Use `journal --plan <plan>
--journal <journal> --allow-journal` for local inspection and `recover --plan
<plan> --expected-plan-id <id> --journal <journal> --allow-recovery
--allow-journal --allow-network --allow-credential-lookup` for fixed server reads.

Recovery records only a matching positive server postcondition. It never
resends a write, repairs corruption, extends approval, drops schema or assumes
that absence proves rollback. Publication recovery also checks the immutable
revision header and actual scoped counts; a later active revision does not
erase a valid historical result. Observing the postcondition does not establish
which request created it or attest the server. Unresolved or contradictory
states remain explicit. The client deadline does not guarantee cancellation of
server execution after a connection closes.

The bounded hash chain is integrity metadata in host-selected files, not an
authentication mechanism or a global coordinator across different journal
paths. Independent host grants are always required. On Windows, exclusive file
locks and retained nonlinked ancestor handles guard the journal; hardlinks,
reparse points, torn records, changed chains and clock regression are refused.
Stdout failures after a durable effect retain its plan/output/journal identity.
Keep the reviewed plan, journal and matching publisher build until pending work
is resolved: policy/build changes invalidate old plans rather than silently
reinterpret their approval or recovery journal.

Provider contracts: [Query API transactions and errors](https://neo4j.com/docs/query-api/current/query/),
[UTF-8 Basic authentication](https://neo4j.com/docs/query-api/current/authentication-authorization/),
and [Neo4j HTTPS configuration](https://neo4j.com/docs/operations-manual/current/security/ssl-framework/).
