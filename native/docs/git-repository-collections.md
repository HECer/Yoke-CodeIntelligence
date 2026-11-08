# Collections with explicit Git project hosts

An explicit [Git project set](git-repository-projects.md) may also use existing
host-registered collections. HTTP uses `gitRepositoryProjects`; the stdio CLI uses
`serve-session --git-projects`. The complete project name set and disjoint source
roots remain required. The single `gitRepositoryCapture`/fixed-root hosting mode
keeps its original refusal of registered projects and collections.

Host collections retain separate canonical registry roots, exact `allowedMembers`,
`allowRegistration`, `allowRegistryCoordination` and optional `receiverProject`.
Git flags grant none of these authorities. Collection/receiver/member selectors
are statically admitted before foreign-root resolution; existing registry
ancestor, capacity and current-session checks still apply. A receiver must be
`default` or an existing host project. An MCP argument cannot register a receiver
root or provide an import grant.

Native and checked-view registration captures the selected project's own source
under its complete Git/source policy before copying immutable evidence into the
registry. A query-only/read-only project may supply captured evidence without
source-write permission. Registry changes still need independent registration
permission and exact expected-registration CAS. Stale expectations refuse before
opening/capturing source. Native snapshots retain exclusions, exact byte paths,
repository scope and crossed-root read-only evidence.

Native archive metadata, query and source-page reads replay the copied evidence.
They do not run Git, read original working files, or open an unused federation
receiver. Current host preference lifetimes and exact persisted session state
remain checked, including at the public collection capability entrypoints.
Changing an inactive selected preference refuses further operations before
registry effects; it does not silently mutate archived evidence or migrate policy.

Signed foreign archives remain a separate capability. The HTTP principal must
use the Session profile and its independently configured `collectionGrants` map;
the collection must name a registered receiving project and the receiving Store
must already exist. Each call borrows only the current principal-filtered grants
under the existing policy lock. Revocation/upsert invalidates old sessions. Signed
receipts, collection rows and client JSON cannot create a grant. Trusted Rust
hosts may supply the existing invocation-only capability directly.

Foreign authorization checks the receiving workspace, signed receipt/key/scope,
current grant exclusions and actual receiver ignore policy. Foreign source need
not belong to that receiver's local Git index. The receiver gets its own validated
host option projection with local repository scope removed; primary scope is
never copied there. Archive metadata/read does not capture local receiver source
or create a cold receiving workspace. Native-only selections bypass an unused
receiver even when the principal has grant entries. Existing federation limits,
including external/linked Git-policy layout refusal, remain in force.

Ordinary sessions/HTTP targets retain their previous bindings and source path
when no Git project set is present. Project-set fingerprints already include
existing collection configuration through the normal host binding. Local source
permissions, registry coordination, native copy authority and signed import grants
remain independent. No model, language server, Python, shell, analyzed-code or
workspace-write authority follows from collection/Git selection.

Qualification uses real owned TCP principal/grant calls, scoped native copy and
archive replay after Git/original working source becomes unavailable. Owned
binary/stdin/stdout and pinned-client health additionally exercise the integrated
host; original source/Git and archive bytes are checked. These are scoped tests,
not production federation deployment or full client/format/quality/current whole
Windows/main/release certification. See [HTTP](mcp-http.md) and
[stdio](git-repository-stdio.md) host contracts.
