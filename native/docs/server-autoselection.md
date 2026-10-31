# Host-selected creation-time language detection

An explicit `HostProfile.languageServerAutoSelection` enables a single automatic
backend choice for each Session project when its native Engine is first admitted:

```json
{"languageServerAutoSelection":{"priorities":{"python":0,"python_basedpyright":3}}}
```

The global-only YAML equivalent is:

```yaml
ls_priorities:
  python: 0
  python_basedpyright: 3
```

Global `ls_priorities: {}` explicitly selects the compiled defaults. Omission
preserves the host policy; `null` clears it. Project, project-local, context and
mode declarations refuse this field. Unknown IDs, nonintegers, booleans and
integers outside signed 32-bit range refuse before Session construction or prompt
rendering. At most 80 overrides and 8,000 serialized policy bytes are admitted.
This uses the existing separately granted configuration projection; YAML cannot
register an executable, trust it or enable source execution.

## Selection and exact declaration data

The classifier uses the filenames in an ordinary policy-bound native capture.
Every file must have source-scope membership. Each positive-priority backend
counts each matching captured filename once. The highest count wins; priority
breaks count ties, and public declaration order breaks remaining ties. A higher
priority does not outweigh more matching files. Zero and negative priorities
exclude a backend. No matching positive-priority backend produces an explicit
empty decision.

The compiled data contains all 80 identifiers from pinned public revision
`ede329aaf0c824bea75455a2926f01420d7aa620`. Its literal suffixes, case policies,
default priorities and declaration order come from the MIT-licensed
[language-server registry](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/solidlsp/ls_config.py)
and the [global priority declaration](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/resources/serena_config.template.yml).
Both original Git blob IDs are retained in `server_autoselection_contract.json`.
No upstream application code is imported or executed.

Ordinary programming backends default to 2; VB.NET, Vue, Svelte and Astro default
to 1; experimental/secondary and nonprogramming backends default to 0. For
example, only `.cs` prefers C#; `.cs` plus `.vb` prefers VB.NET because it matches
more files. `.ts` alone prefers TypeScript, while `.ts` plus `.vue` prefers Vue.
Multicomponent suffixes such as Erlang `.app.src` remain intact. Only C++, CCLS,
Fortran and Ada use case-insensitive matching. All other matchers preserve case.
This classifier uses these exact declarations independently of the historical
catalog's approximate extension helper.

## Host boundaries and lifecycle

The winning backend must already be registered by the trusted host. A missing
registration refuses; it does not choose a lower-ranked installed backend.
Selection never starts a language server, downloads a package or executes
analyzed project code. Later semantic requests retain the original explicit
registration, passive-profile, runtime/source-execution, source-scope, deadline,
response and cleanup checks. A backend identifier is not runtime qualification.

The primary project is selected during Session construction, after original
activation admission. Each registered secondary project is selected on its first
Engine admission. The existing Session comparison-and-swap publishes the choice
and advances its metadata revision. Secondary activation retains that newly
published choice before publishing its activation revision. Retained choices
survive source edits and reopening the same Session: this is creation-time
selection, not a continuously recomputed per-file ranking. To change the policy
or host registry, use the ordinary fresh Session/binding workflow.

Explicit host `languageServers` selections supersede detection. A separately
granted dashboard backend/legacy selection also supersedes it; reset restores
the retained automatic choice. These explicit selections retain their existing
ordered extension-match/first-fallback behavior. In automatic mode, a file must
match the selected backend's exact filename declaration. An unmatched file or
empty decision never falls back to a legacy provider. Empty detection cannot
report a successful language-server restart.

Absent/null automatic policy preserves preceding profile and State bytes,
fingerprints and legacy routing. A selected policy binds its compiled contract,
overrides and all host backend registration settings into the Session
fingerprint. Current-state checks cover tools and persistent REPL APIs. An
inspection-only open cannot initialize a missing automatic choice.

## Evidence and bounded scope

Configuration reports the selected policy, per-project `automaticLanguageServers`
and whether automatic selection currently applies. Each decision records
`backendId`, matched/captured counts, omission count and an order-independent
filename-set hash. It is metadata, not a source hash, immutable snapshot pin,
retention root or independent proof of host selection.

Selection admits at most 100,000 captured paths, 100,000 omissions, 4,096 bytes
per normalized relative path and 16,000,000 cumulative path bytes. Duplicate,
absolute, traversal and invalid paths refuse. At most 101 retained project
decisions and 32,000 serialized metadata bytes are admitted. Cooperative deadline
checks surround admission and matching; capture keeps its existing limits.

Counts cover captured filenames only. Unsupported extensions, excluded/ignored
paths, oversized files and other capture omissions can affect the winner.
`automaticSelectionExhaustiveProjectCoverage` remains false, including when
the omission list is empty. The current GraphQL catalog entry still needs an
explicit capture configuration to make unsupported source files available.
Public interactive project-creation prompts are not implemented by this route.

Two new inert public-data tests passed after a verified missing-data failure.
Ten related inert contract tests and the required TypeScript checks are recorded
separately from native qualification. Native unit and integration fixtures are
authored for ranking, Unicode/case/supersets, malformed inputs, frozen selection,
host refusal, dashboard override/reset, metadata validation, secondary activation,
stdio and authenticated HTTP. They have not been compiled or executed locally.
No native runtime, real vendor, Windows kernel cleanup, combined platform,
browser/VSIX, main or release acceptance follows from these source checks.
