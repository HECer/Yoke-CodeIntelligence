# Explicit Git project sets in the stdio session CLI

```sh
yoke --root /absolute/primary serve-session --host-profile /absolute/host.json --git-projects /absolute/git-projects.json --allow-git-index-read --trust-git --id owned-session --profile native
```

The selection file is the [registered-project map](git-repository-projects.md)
itself, for example `{"default": {}, "secondary": {"followNestedRepositories": true}}`.
It must name `default` and every project already registered in the explicit host
profile. Roots come from the primary CLI root and those registrations; selection
JSON cannot add a root or permission. Existing disjoint-root, collection, program,
limit and preference-lifetime checks remain in force. Follow settings are optional
overrides of saved host preferences. Git program selectors are absolute host paths.

Both independent Git flags are required. Using either without `--git-projects`
refuses before source state. Unknown fields, incomplete/unknown names, imported
repository scopes and a global `--snapshot` refuse. Git selection grants no source
writes, model, Python, shell, LSP or analyzed-project execution. Standard ordinary
`serve-session` without a selection and fixed-root `git repository-scope` hosting
retain their preceding behavior and saved bindings.

This command's `native` profile is the existing nine-tool Session view. Other
choices are `serena`, `trail`, `graft` and `graphify`, with existing 53/6/6/10 tools.
The separate plain native `serve` command remains its eight-tool view. Project
queries, activation, startup selection and adapter root choices follow the
[registered-project contract](git-repository-projects.md); source/read-only/
query-only/write rights remain independent.

Primary Git metadata is prepared before its cold Store. Global capture options
and the selected analyzer epoch are then installed, explicit language servers
registered and the full per-project host policy applied by Session. Only then is
the selected startup source captured, before any protocol output. No summary JSON
is printed in front of MCP frames. Startup in a secondary project can publish its
own filtered snapshot while leaving primary source unpublished. Secondary metadata
and initial/final byte budgets retain their separate root binding.

Existing `--capture-config` filters, `--analyzer-epoch` and explicit
`--language-server-config` settings retain their roles; an imported repository
scope cannot replace the selected drivers. Git capture explicitly refuses
ignored-source overrides, including `--include-ignored-dir`, `includeGitIgnored`
and physical ignored-directory selectors; it does not silently widen index
membership. A host policy requesting ignored sources has the same restriction.
Language-server configuration requires independent `--trust-servers`. Project
exclusions apply before first publication. Crossed repositories stay read-only.

Optional Serena `--python`/`--trust-python` and explicit shell settings retain the
existing trusted-runtime path. In Git mode its specification is validated before
metadata/state; a Git flag cannot satisfy runtime trust. No runtime is started
unless its separate explicit inputs select it. The runtime remains trusted host
execution, not an operating-system sandbox. See [provider authority](provider-authority.md).

Named sessions resume after changing file membership under the same host binding.
Changed programs, limits, follow settings or preference lifetime refuse the old
label; select a new matching host/session explicitly. Current preferences and exact
persisted state remain checked during operations, including before notes. Failed
startup publishes no source proof; empty application/session state may remain, and
correcting a bound host policy may require a new label. No silent migration follows.

Qualification uses actual CLI/stdin/stdout source calls across all five profiles,
old/new ordinary registered-session payload/row comparisons, private pinned Claude
2.1.306 stdio health and a separately selected Python callback trial. No model turn
or analyzed-code execution is requested. Registered [collections](git-repository-collections.md)
retain separate registry and import authority. Broader client/format/quality
and current whole Windows/main/release acceptance remain open.
