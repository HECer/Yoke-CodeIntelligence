# Immutable configured compiler contexts and solution builds

Rust owns the request, immutable source binding, resource limits, worker lifecycle, validation and artifact publication. This optional semantic backend runs an explicitly selected Node executable and TypeScript **5.8.3** installation. Native indexing and the default MCP need neither. It does not invoke Graft, Serena, Graphify or the retained TypeScript application.

```powershell
yoke --root C:/project compiler --node 'C:/Program Files/nodejs/node.exe' --typescript C:/tools/typescript/lib/typescript.js --trust-compiler
yoke --root C:/project --snapshot <snapshot-id> compiler --node 'C:/Program Files/nodejs/node.exe' --typescript C:/tools/typescript/lib/typescript.js --trust-compiler --build
```

`projects::analyze` and `projects::build` accept a typed `CompilerSpec` with absolute host paths and explicit trust. Repository configuration cannot grant execution or choose a compiler. Source/configuration text comes from the persisted immutable snapshot. There is no source filesystem fallback. The sole declaration fallback reads the trusted compiler's direct `lib*.d.ts` files. Compiler plugins are never loaded, configuration is treated as data, and source is never executed.

The compiler's own config parser handles files/include/exclude/extends, path aliases and project references. Referenced configs with arbitrary names are discovered from captured references. Missing/outside references and cycles produce explicit diagnostics. Each configured project keeps its own options, bindings, diagnostics and context identity. Overlapping files may have conflicting bindings, which are reported without merging them into a single compiler truth. Navigation resolves captured referenced source even when emitted declarations are absent. Config-less analysis is explicitly inferred and nonexhaustive.

Bindings and local declarations use exact UTF-8 byte spans validated again by Rust against source. Standard-library and unavailable external declarations cannot become invented local source anchors. Diagnostics preserve context and compiler codes. Compiler errors remain visible; a structural graph is never relabeled as compiler binding.

Reports record the selected compiler hash and exact hashes of the standard-library declaration bytes actually used. Context identity includes those bytes and compiler version/options, so changing a trusted library declaration cannot reuse the previous context identity. The library reader streams bounded chunks and never loads an arbitrarily grown file merely to hash it.

Solution builds use the pinned compiler's solution builder with a virtual output filesystem. Referenced generated declarations are consumed within that build. Output leaving the virtual root, conflicting with immutable inputs or exceeding limits is refused. No generated file is written to the original workspace. Zero configured sources, no generated artifacts, missing context, errors, process failure, truncated output or deadline exhaustion cannot yield a passed build. Generated content is retained in an immutable registered artifact; returned metadata contains paths, hashes and byte counts.

Limits include 20,000 captured files, 10 MB source/configuration text, 128 projects, 500,000 visited AST nodes, 100,000 bindings per context, 20,000 diagnostics/outputs, 20 MB generated content, 16 MB request and 32 MB reply. The trusted worker has a 512 MB Node heap limit and one shared request deadline. Heap limits are not an OS sandbox. The host compiler process retains host privileges.

Run `cargo test --locked --test projects` for actual compiler tests covering historical source, Unicode anchors, inherited include/exclude configuration, conflicting path aliases, project-reference navigation/builds, cyclic/missing/empty projects, forbidden output and denied execution/plugin authority.

The separate [compiler inline adapter](compiler-inline.md) uses the same immutable
filesystem and pinned installation for an explicit host-selected variable-inline
action. It checks unpublished original-coordinate effects before an ordinary
guarded plan; analysis/build reports retain their existing public operations and
artifact contracts.
