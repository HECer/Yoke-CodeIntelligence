# Project-selected LSP workspace settings

Explicit `HostProfile.languageServerSettings` entries additionally accept an
optional `settings` object, alongside `initializationOptions`. The host still
registers each backend and separately grants `trustedServerSettingsProjects`.
Pinned YAML `ls_specific_settings` projects the same data and cannot create
runtime registrations, project grants or source-execution authority.

For each selected project/backend, the object overlays the ORIGINAL registered
`HostConfiguration.settings` at the top level. Nested objects and explicit null
values replace that key; this is not a recursive merge. Absent/null `settings`
preserves previous serialization, fingerprints, wire replies and host defaults.
An explicit object requires a null/object original value. Whole-backend
project-local/project/global precedence is unchanged; another project's overlay
never becomes the starting point. Empty backend entries keep registered defaults.

The input and merged workspace object each have at most 64,000 serialized bytes,
32 value levels and 4,096 value nodes. Existing backend/project/profile and
argument limits still apply. Initialization options retain their prior limits.
Invalid shapes, combined overflow, unregistered backends and missing project
trust refuse before the provider starts. Changed host policy requires a matching
new named-session binding; concurrent/stale state and HTTP revocation still refuse.

The existing LSP transport sends the selected data in
`workspace/didChangeConfiguration` and answers `workspace/configuration` callbacks
from that provider's settings. Direct queries, staged refactor actions and provider
refresh use the same selected registrations. This adds project/backend selection;
it also supports explicit captured-file configuration via
[resourceSettings](lsp-resource-settings.md). It does not add dynamic configuration
editing in a running session. The official
[notification contract](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/workspace/didChangeConfiguration.md)
and [configuration callback contract](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/workspace/configuration.md)
describe the two distinct message routes.

Runtime trust, passive profiles, source-execution, workspace and write permissions
remain independent host values. Common passive admission validates the effective
settings before process startup; mandatory transport safety settings still apply.
A project setting cannot enable Rust build scripts, provider plugins or another
native permission. Launch overrides retain their existing separate requirements.

Owned Node protocol fixtures test actual push/pull messages, Unicode/nested/null
values, staged actions, project/backend isolation, capacities, stale bindings,
YAML host trust and real authenticated TCP/revocation. Actual CLI trials compare
the preceding/current binaries on the same owned source and Store, and check YAML/
JSON binding equivalence. Fixture protocol evidence is distinct from real vendor
runtime/version certification. No model or analyzed project code executes.
Broader vendor aliases/resource scopes, all 77 runtime/operation reviews,
other clients/platforms and current whole Windows/main/release acceptance remain
open. See [session configuration](session-config.md).
