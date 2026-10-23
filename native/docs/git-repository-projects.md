# Explicit Git hosts for registered session projects

Authenticated MCP HTTP may select a separate Git driver for every existing
host-registered project. The trusted principal uses `gitRepositoryProjects`
instead of `gitRepositoryCapture`, for example:

```json
{
  "id": "owned-service",
  "tokenCredentialName": "existing-private-service-credential-name",
  "profile": "session",
  "hostProfile": {
    "projects": [
      { "name": "secondary", "root": "/absolute/secondary", "readOnly": true, "queryOnly": false }
    ]
  },
  "gitRepositoryProjects": {
    "default": {},
    "secondary": { "followNestedRepositories": true }
  }
}
```

The principal remains inside the existing host file's `principals` array. Its
primary root is the principal root or the CLI root. Each map value uses the
[fixed-root HTTP selection](git-repository-http.md): optional absolute Git
`program`, follow overrides and bounded `limits`. Names must be exactly
`default` plus every registered project; at most 100 secondary registrations
are allowed. Missing/unknown names, duplicate registrations, overlapping roots,
collections, collection grants and imported repository scopes refuse. Single
and project-set selection cannot be combined.

Run the existing service with independent `--trust-service`,
`--allow-git-index-read` and `--trust-git` flags. The complete name selection,
program selectors, limits and authority flags are admitted before credential
lookup or foreign-root resolution. JSON contains selection data and credential
names, never executable authority or bearer values. Each project's root comes
only from its existing trusted registration; MCP arguments cannot register or
change it. Existing ordinary and fixed-root targets remain independent.

The Rust API exposes non-deserializable `repository_projects::Hosts` and
`Session::open_with_git_projects`; HTTP attaches a set through
`Target.git_repository_projects`. The set binds each name, canonical root,
selected Git program, limits/follow choices and observed preference lifetime.
The session also retains the existing host permissions, capture/source policies
and exact persisted state. Dynamic file membership is source evidence and does
not change that stable binding. Changing any selected policy requires a new
matching host/session; a changed preference in even an inactive selected project
refuses further operations until explicit host restart. Stale concurrent session
state refuses before note mutation.

HTTP configuration validation performs primary metadata preflight and existing
session/source-policy admission without publishing source. Successful
authenticated initialization captures the selected startup project. A secondary
engine acquires its own fresh metadata before its cold Store, applies complete
per-project source exclusions and other host defaults, and then publishes its
first source capture. Initial/final metadata consume the same original per-root
index-byte budget. Primary dynamic repository scope is never copied into a
secondary engine. A failed capture publishes no source proof. Existing-session activation commits
only after successful capture; failed activation leaves that state unchanged.
Empty application/session state may remain after initialization or final-recheck
failure.

Each secondary engine retains independent read-only/query-only/write permissions.
A primary read-only flag does not revoke an explicitly writable disjoint
secondary registration. Included nested/submodule repositories remain read-only,
also during guarded plan preparation. Registered query-only roots can be read
through existing query adapters and cannot be activated or used to prepare edits.
No Git setting grants source writes, Python, shell, model, LSP or analyzed-code
execution. A Git program path binding is not executable-byte attestation or an
operating-system sandbox.

Existing profile catalogs remain 8/9/53/6/6/10 tools. Native uses its primary
engine; Session can query/activate named registrations, Serena uses its existing
activation/query methods, Trail/Graft use their selected startup/active project,
and Graphify can select an already registered `project_path`. Tool and source
policies remain those of each adapter. Host startup selection can initialize a
secondary project without publishing primary source.

Owned tests and actual HTTP source calls cover the project set, independent
permissions, Unicode paths, stale bindings, source exclusions, exact budgets and
cold failure. Pinned Claude 2.1.306 connection health uses private owned settings
without model turns or account login. The ordinary registered-session path is
compared against the preceding executable with exact saved records and stable
source payloads. This does not certify all clients or full upstream behavior.
The existing fixed-root stdio CLI remains fixed-root; additional stdio selection,
collections, broader formats/quality and whole Windows/main/release acceptance
remain open. See the [transport](mcp-http.md) and
[profile binding](git-repository-profiles.md) contracts.
