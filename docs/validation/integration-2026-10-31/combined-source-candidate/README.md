# Combined current-source qualification candidate

This branch combines the current source increments from PR7–20 through normal
merge commits. It targets PR20's `t3/vscode-owned-job` branch at
`06267ee8c37b35f8d15eab8e5383882b740a0bea`; it does not merge a PR or publish a
release. Earlier status sections retain their historical tested source and
limitations. Their narrower evidence does not qualify this combined candidate.

The five additional constituent tips are:

| Tip | Source scope included |
| --- | --- |
| PR8 `541011a56386fb8895755da2ef7dbd32a02de977` | Host-authorized project activation, CLI/Session/configuration integration and its existing process/permission fixtures |
| PR16 `992630ed9b2a1189f109ba728adefe045a56c59a` | The selected encoding stack, including UTF-8-SIG, ASCII, Windows-1252 and UTF-32, strict raw-byte mappings and the quoted-YAML fixture correction |
| PR17 `60d4d2d28079f891e3352645ba566eb617860a4e` | Complete while/for/do condition inlining candidates, independent Rust reconstruction, original verification/writer gates and the for/do build/lint repairs |
| PR18 `d49cd0815d6c8573fdd5f225194b488abc202b6e` | Receipt-bound host-local original-preview verification, Store-opening observations and separate credential-backed Windows TLS CI |
| PR19 `2a205ac30f2b8295ebcadb3a669cc7f51a374d2b` | Bounded MCP/Explorer receive-failure observations and their existing adversarial fixtures |

PR20 supplies the owned VS Code Job launcher, same-run lifecycle probe, fresh
combined native/TypeScript VSIX production and both explicit/bundled real Rust
extension-host CI modes. The additive credential TLS job and activation-related
Windows probes are retained alongside the original default native shard,
aggregate, release/history and four-profile browser gates.

Merge conflicts occurred only in `docs/STATUS.md` and `docs/REMAINING_WORK.md`.
Both sides' historical blocks were retained. Two imported native-guide documents
received CRLF-to-LF alignment only. No production/test conflict was resolved by
discarding either feature, replacing a file wholesale, or changing an assertion,
permission, ignore, deadline, worker resource limit or release gate.

The automatic source overlaps are reviewed explicitly: the library retains the
activation module, observed Store opening and the complete encoding capability
list; the Rust workflow retains PR8's steps followed by PR18's separate TLS job;
the inline-body target retains PR17's recipes/tests with PR18's observed opening
helper. Codec sources and fixtures remain exact to PR16, activation integration
exact to PR8, inline implementation exact to PR17, receiver verification exact to
PR18 and the Explorer/MCP targets exact to PR19. This is source-composition
evidence, not proof of native typing or runtime behavior.

Allowed local validation covers the original npm typecheck/tests, Rust 1.94.1
formatting/source parsing with the declared Rust 2024 edition, Python ASTs,
JavaScript syntax and 33 source-extracted inert VSIX/Job admission tests. The
native Cargo build/check/test/Clippy, controllers/workers, native-image payloads,
actual editor and new combined VSIX are not executed or accessed locally.

Fresh hosted evidence is required for this source: complete default inventory and
original ignores, platform aggregate/build/lint/release/history, all four browser
flows and their actual visual inspection, owned credential TLS, owned Job
lifecycle, native/combined packaging and both real Rust extension-host modes.
The retained TypeScript runtime is bundled and byte-verified by that profile;
advanced TypeScript extension mode remains a separate unexecuted scope.

Strict macOS memory enforcement, full current upstream feature/backend parity,
the original 353/R1–R9/77-runtime requirements, remaining authorized live-provider
qualification, main/publication and release acceptance remain open. No successful
per-branch check or source merge is promoted to those broader certificates.
