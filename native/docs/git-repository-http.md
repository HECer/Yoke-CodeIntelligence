# Explicit fixed-root Git capture over authenticated MCP HTTP

```sh
yoke --root /absolute/project serve-http --config /absolute/host.json --trust-service --allow-git-index-read --trust-git
```

The trusted host file can select `gitRepositoryCapture` independently for each
principal. Its fields are optional `program` (an absolute UTF-8 Git executable
path), `followSubmodules`, `followNestedRepositories`, and `limits` (the existing
bounded repository-scope limits). For example, a principal may add:

```json
"gitRepositoryCapture": {
  "followNestedRepositories": true,
  "limits": { "maxRepositories": 8 }
}
```

This is selection data. Both independent CLI permissions above are required;
neither this object nor MCP arguments can grant Git execution/index reads.
`--trust-service` remains independent. Unknown JSON fields refuse. Limits and
fixed-root admission are checked before credential lookup. Omitted follow fields
reuse the current host preference; explicit true/false overrides apply only to
this host. Keep service credentials in the existing private credential store;
the host file contains credential names, never bearer values.

The Rust host API can attach an explicitly constructed
`git_workflows::repository_host::Host` through `Target.git_repository_capture`.
It has no deserializer. Principal validation requires the same canonical root,
no imported repository scope and no registered projects/collections or collection
grants. Separate principals can own distinct roots. An authenticated client
cannot change its root, selected Git executable, follow choices or permissions.
Ordinary HTTP targets without the driver retain their preceding behavior.

Configuration validation performs bounded Git metadata and physical-name
preflight before cold Store creation. It retains the existing source/session
policy validation and may initialize empty local state; it publishes no source
snapshot. Failed authentication starts no source capture. Successful initialize
reacquires metadata, attaches the prepared driver, applies the complete host
source policy through the session, and only then reads/publishes source. The
initial metadata and final index recheck consume one original index-byte budget.
Invalid metadata or exhausted budgets refuse explicitly, without a source proof.

Native/session/Serena/Trail/Graft/Graphify retain their existing 8/9/53/6/6/10-tool
catalogs. Subsequent indexing uses fresh metadata. Included crossed repositories
remain read-only, including through older writable snapshots. Source exclusions,
read-only settings, note permissions and independent write/runtime grants remain
in force. Git hosting grants no Python, shell, model or analyzed-code execution.

The stable root/program/options/limits/preference binding follows the
[stdio profile contract](git-repository-profiles.md). Current preferences and
persisted session state are checked on operations; concurrent preference changes
refuse both existing and newly initialized sessions until the host is restarted
with the current selection. No silent policy migration follows.

The [HTTP transport contract](mcp-http.md) still governs authentication, TLS,
Host/Origin checks, revocation, rotation, session isolation, framing and the
250-30000 ms host request budget. Source capture occurs within that request
budget. Selection of a Git executable binds a path, not executable bytes, and
is not an operating-system sandbox.

Qualification uses actual owned TCP requests and pinned Claude 2.1.306 HTTP
connection health in private configuration, without model turns or account login.
Multiple registered projects/collections within a Git target, wider client
behavior/formats/quality and whole Windows/main/release acceptance remain open.
