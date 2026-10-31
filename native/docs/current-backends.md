# Current upstream backend identifiers

The source candidate adds explicit host-selectable `vbnet`, `typescript_tsgo`
and `graphql` entries. Their public identifiers and file matching are pinned to
[Serena's declarations at ede329aa](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/solidlsp/ls_config.py).
The independent [data contract](../../docs/parity/current-backends-2026-10-31.json)
records all 80 current identifiers and the prior 77. The original historical
353-entry inventory and its assessments are unchanged. No Serena application
implementation is copied into Yoke.

All 77 preceding entry blocks and their order are retained. The only production
change is three appended metadata entries; registration, path selection,
permissions, passive/source-execution admission, resource limits and runtime
negotiation use their existing code. A catalog name never installs a server,
selects a workspace-controlled executable or grants permission to execute it.
The displayed catalog grows; earlier whole-output byte identities do not cover
this additive catalog response.

- `vbnet` covers Visual Basic and C# files (`.vb`/`.cs`) through a host-selected
  Roslyn server. Its first native language is `vbnet`; the host may supply the
  server's document-language mapping, such as `vbnet` to `vb`. Ordered selection
  remains the host's choice, including overlaps with `csharp`.
- `typescript_tsgo` covers the existing TypeScript/JavaScript file variants with
  an explicitly selected native TypeScript language-server command. It does not
  replace the separately pinned TypeScript compiler worker or its source proof.
- `graphql` covers `.graphql` and `.gql`. The frozen default capture-language map
  and native grammar registry are unchanged. Explicit host capture overrides
  (`{"languageOverrides":{"graphql":"graphql","gql":"graphql"}}`) admit
  these source views without claiming a native GraphQL AST. The real experimental
  provider requires schema configuration. A catalog entry does not authorize
  reading configuration outside captured scope or executing project code.

Real processes still require an absolute host-selected registration and trust,
plus the original measured passive profile or explicit source-execution grant.
Neither YAML declarations nor MCP requests can create those grants. Unknown IDs,
unregistered IDs, unsafe command/workspace paths and oversized configuration
remain refusals. Provider results retain their source bindings and negotiated
scope; missing runtime or parser capability cannot become an empty complete
result.

The public-data tests first failed for exactly these three missing IDs and pass
after the appended entries. They also compare the old entry blocks and current
language/extension data. They parse catalog literals and never execute native or
provider code. Four new native fixtures are authored: metadata/path matching,
trust/path/capacity refusal, unregistered selection before process startup and
owned protocol selection for VB, TypeScript and explicitly captured GraphQL.
The protocol fixture uses the existing controlled LSP peer, Unicode/CRLF source,
explicit language-ID mapping and source-preservation checks. It is not a real
Roslyn, tsgo or GraphQL server qualification.

Native compilation and these fixtures remain unrun locally and require guarded
hosted evidence. Real vendor runtimes, all historical 77 backend qualifications
and these three new runtime qualifications, broader current upstream parity,
platform/VSIX/main and release acceptance remain open.
