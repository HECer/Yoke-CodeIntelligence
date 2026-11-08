# Fixed-root Git capture over authenticated HTTP - 2026-10-23

Source `2674a2f53763a5869a57264eb8e700fea78859ea` adds explicit per-principal Git hosting to the existing
HTTP transport. [The contract](../../../../native/docs/git-repository-http.md)
describes independent CLI permissions, metadata preparation and full source
policy before authenticated capture. Registered projects within a Git target
remain unavailable; distinct fixed-root principals remain separate.

[index.json](index.json) pins 1,094 frozen inputs, seven source/test Git blobs and
153 reopened members of [raw.zip](raw.zip). Every size and SHA256 is
verified. ZIP SHA256 `a91b9a9f526bda09847944d89871228191548005653c7a1d9cc7e6fb201714e8`; 368,017 bytes.
Private temporary credentials/client files and executable images remain local.

| Check | Result |
| --- | --- |
| Full library | 578 passed, 7 existing ignored |
| New HTTP integration | 7 passed, 0 ignored |
| Total 30 Rust groups | 777 passed, 0 failed, 8 existing ignored |
| All-target Clippy with warnings denied; locked build | passed |
| Typecheck; npm | passed; npm 260 passed, 3 existing skips |
| Formatting | 356 metadata roots pass; fmt-all OS206 retained |

The targeted and broader phases share exactly the same frozen inputs. Tests
cover actual TCP authentication and principal isolation, cold invalid metadata,
source policy before first publication, stale preference refusal in existing/new
sessions, first-capture shared byte budgets and foreign-root/registration denial.
Adjacent HTTP/CLI/collection-grant, Git scope/preference/host/profile, source
policy, session/state/permission, protocol and facade targets pass. Original
tests are unchanged and no new ignore or skip is added.

The first prototype compiles, then retains one pass and six failures. These
combine incorrect new fixture timeout/schema/empty-state assumptions with a
premature capture path during Principal configuration validation. The correction
retains existing prelistener host/session validation, limits preparation to Git
metadata there, and publishes source only after authenticated full policy setup.
The new fixture uses the existing 30000 ms transport maximum and generic schema
error. Frozen main, HTTP implementation and new test bytes change between that
attempt and final qualification; this is not described as a fixture-only fix.

The live trial runs 8 owned parent processes. A
separate Rust host helper links the current library and exercises actual HTTP
initialization/catalogs for native/session/Serena/Trail/Graft/Graphify. All six
catalog name arrays match preceding captured profile catalogs. A real Serena
read returns the included nested source; the sole saved snapshot excludes the
host-denied file and its capture proof keeps the crossed repository read-only.
All original source and Git file hashes remain exact.

Pinned Claude 2.1.306 performs actual mcp-get and mcp-list HTTP connection health
for every profile, twelve calls total. Its header-bearing configuration and raw
stdout/stderr stay private; only booleans/catalogs are recorded. No model turn or
account login occurs. Each parent is assigned to its Windows Job before resume;
all helper/client children inherit it, and every owned Job ends empty.

Final CLI SHA256 `4483d2e3f3fc93d12cb921a4360fcd0d6588e657a38fc10d52c6acc74ffe5713`. The original 353-entry reconciliation
ledger and summaries are deep-equal apart from one additive checkpoint. No
Developer Mode, privilege, execution-policy, original OS1314 fixture or protected
failed-artifact change occurs. This is scoped qualification, not a whole-native,
full upstream, Windows/main or release acceptance claim. No product test is
claimed after documentation updates.
