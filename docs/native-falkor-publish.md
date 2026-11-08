# Native FalkorDB publication (0.5.0 development)

`yoke falkor-publish` explicitly publishes an existing saved native base graph
to a host-selected FalkorDB endpoint. Preparing and inspecting plans is offline.
Live credentials and guarded journals currently support **Windows**. The actual
development profile is FalkorDB **6.0.2**, graph module version **60002**. Other
versions are refused rather than treated as qualified. This finite connector
does not establish full upstream, production, RBAC or non-Windows equivalence.

Use a closed target file, for example this documentation-only endpoint:

```json
{
  "baseUrl": "rediss://graph.example.invalid:6379/",
  "address": "203.0.113.18:6379",
  "namespace": "reviewed-code-graph",
  "user": "publisher",
  "useSystemRoots": true,
  "rootCertificatesDerBase64": []
}
```

The socket address is literal and pinned. TLS verifies the URL hostname using
static platform roots and/or explicit base64 DER anchors. There is no DNS
fallback, proxy, redirect, reconnect, automatic write retry or opportunistic
TLS downgrade. URL credentials, paths, query strings and fragments are refused.
Plain `redis://` requires both a literal loopback URL/address and the separate
`--allow-loopback-plaintext` flag. TLS always uses `rediss://`.

Import exact UTF-8 password bytes through a private stdin pipe using
`import-password --config <target> --allow-credential-create`. Never place a
password in a command argument, target, plan, repository or log. Windows imports
are create-only, bounded by the original deadline and bound to the canonical
endpoint, literal address, user and TLS policy. Namespaces share the endpoint's
credential binding; users have separate bindings. Private input rejects control
characters, invalid UTF-8, terminal input, excess capacity and a pipe that stays
open past the original deadline. A plan never contains or grants credentials.

The normal workflow is:

1. Create `bootstrap-plan --config <target> --output <plan> --allow-output`.
   Inspect it with `inspect --plan <plan>` and review the exact ID, endpoint,
   namespace, operations and trust policy.
2. Record approval using `approve --plan <plan> --expected-plan-id <id>
   --journal <absolute-parent>/falkor-<id>.jsonl --allow-approval --allow-journal`.
   The parent must exist and the journal must be fresh. Approval defaults to
   ten minutes; `--valid-for-ms` permits at most one hour.
3. Apply the bootstrap with `apply --plan <plan> --expected-plan-id <id>
   --journal <journal> --allow-network --allow-credential-lookup --allow-write
   --allow-schema --allow-journal`. Add the loopback flag only for that profile.
4. Prepare publication using `prepare --config <target> --source-root <root>
   --snapshot-id <saved-id> --expected-revision absent --output <plan>
   --allow-output --allow-source-coordination`. This uses the existing saved
   snapshot and graph; changed disk bytes do not replace captured source.
   Existing Store coordination is explicit and missing state is refused.
5. Inspect, approve and apply that separate publication plan with network,
   credential, write and journal grants. Publication needs operational schema;
   it never silently bootstraps or repairs it. For updates, replace `absent`
   with the observed prior revision ID.

Bootstrap has nine durable operations. `WATCH` and `EXISTS` admit a fresh
dedicated graph key; `MULTI` queues exactly one root initialization and `EXEC`
conditionally performs it. No schema writes are queued with initialization.
An existing graph is refused. Five indexed properties and three unique
constraints follow separately. `PENDING` or `UNDER CONSTRUCTION` never means
ready: fixed read-only scalar projections must observe `OPERATIONAL` within the
original deadline before a schema step is confirmed. Unexpected schema is
refused without destructive repair. Interrupted bootstrap can continue only
after positive recovery of its pending operation under the same reviewed plan.

`probe --plan <plan> --allow-network --allow-credential-lookup` performs only
fixed server/profile/schema/root reads. Host flags do not grant server ACL
privileges. The endpoint must allow the selected graph and the commands needed
for its reviewed role, including profile inspection. Role minimization and
deployment policy remain host responsibilities.

Publication is one fixed Cypher transaction. Native values enter a compiled
parameter preamble with bounded, typed literal encoding; repository text never
becomes a statement, identifier or permission. All original entity, evidence and
observation JSON, source hashes and byte ranges, producer/resolution labels,
parallel directions, unresolved targets, coverage and diagnostics are retained.
Unresolved endpoints are marked data nodes and do not become resolved entities.
The common native export data codec retains its earlier Neo4j-named v1 kind;
that serialization kind conveys no Neo4j endpoint or network authority.

Immutable scoped revisions use expected-revision, known-property and actual
row-count guards. Explicit relationship counts preserve parallel observations.
A conflicting transaction rolls back its writes, including its coordination
counter. Same-revision replay preserves logical data and may advance that
counter. Older revisions remain present. A confirmed operation in the same
journal is not resent. Lowerable limits reject the whole payload rather than
truncate it: defaults are 5,000 entities, 50,000 observations, 50,000 evidence
rows and 16 MB payload, with separate 32 MB frame/plan/response bounds.

Each write has a flushed local intent before it can execute. Native RESP2
parsing preserves nil, empty, integer and binary distinctions; fixed response
checks require exact receipt fields, types, cardinality and identities. Schema
completion additionally requires a positive operational postcondition. A server
error, malformed response, lost connection or expired deadline leaves a sent
operation pending. `apply` cannot reset or replay it automatically.

Inspect local state using `journal --plan <plan> --journal <journal>
--allow-journal`. Recover with `recover --plan <plan> --expected-plan-id <id>
--journal <journal> --allow-recovery --allow-journal --allow-network
--allow-credential-lookup`, plus the explicit loopback flag where applicable.
Recovery uses only fixed reads and a matching positive postcondition. A
publication also requires its immutable revision header and actual row counts;
a later active revision does not erase a valid historical result. Absence does
not prove rollback or authorize resend. Client cancellation cannot guarantee
server execution cancellation.

The bounded hash chain is integrity metadata, not authentication or global
coordination across different journal paths. Windows retains nonlinked ancestor
handles and an exclusive, single-link journal handle. Reparse points, aliases,
hardlinks, torn records, altered chains and clock regression are refused. Lost
stdout after an effect preserves the plan and journal identity. Keep the
matching publisher build, reviewed plan and journal until pending work is
resolved: policy changes invalidate old plans rather than reinterpret approval.

This exports data; imported claims do not establish server, source, compiler,
call, execution or ownership authority. Semantic completeness remains false.
Default retrieval starts no connector, model, VM or analyzed project code.
The owned validation VM and external server binaries are test tooling, not
bundled runtime dependencies or code copied into the MIT core.

Provider references: [graph queries](https://docs.falkordb.com/commands/graph.query),
[read-only queries](https://docs.falkordb.com/commands/graph.ro-query),
[asynchronous constraints](https://docs.falkordb.com/commands/graph.constraint-create),
and [WATCH/MULTI/EXEC](https://redis.io/docs/latest/develop/using-commands/transactions/).
