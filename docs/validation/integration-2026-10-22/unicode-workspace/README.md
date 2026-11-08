# Distinct Unicode workspace folder identities

Code Intelligence 0.5.0, Windows. Source 383e3defa1a1a432b136d1b944b119e12590eca5.

Windows workspace-folder identities now fold ASCII case only while preserving
opaque Unicode bytes. U+0130 and ASCII i plus U+0307 are real distinct owned
Windows directories, but full Unicode lowercasing merged them. Primary symbol
selection wrongly rejected them as duplicates, and the additional semantic-root
union silently dropped one root. Both paths now share the same conservative
identity helper. Known ASCII aliases still refuse or deduplicate; exact captured
membership, protected-path checks, budgets and original source authority remain.

At source 383e3def, 697 Rust functions pass across 17 scoped groups,
including the full library (569 passes), ten semantic-workspace and nine primary
symbol-workspace functions. Two new regression functions fail 0/2 against the
unchanged published implementations, then pass after correction. 8 existing
ignores and zero failed functions occur in the qualified scope. All ten original
symlink fixture Git and prior frozen working bytes remain exact; their Windows
prerequisite acceptance remains open. No system/privilege changes or new skips.
All-target Clippy -Dwarnings, native build, typecheck, npm 260 passed/3 existing
skips and 345 actual format roots pass on 1061 frozen inputs. Cargo fmt --all
retains OS206. This is scoped evidence, not whole current Windows certification.

Actual provider initialization and workspace callbacks retain both distinct roots.
The symbol projection binds both original source hashes and exact UTF-8 byte
ranges without changing capture or the base graph. An actual installed
rust-analyzer1.94.1 also resolves an exact reference between these Unicode paths
in an owned single crate without source-execution authority or external
dependencies. Actual old/new default configuration/MCP53 and literal LSP data
JSON remain exact; whole SQL before/after is unchanged. Only Native requestId and
duration are excluded after an independent repeat of the original binary.
See [Unicode workspace evidence](README.md)
and [configuration contract](../../../../native/docs/session-config.md#fields-and-boundaries).

This does not certify all filesystem alias rules, all77 runtimes or broader
semantic quality. External/foreign roots, other project-setting extensions,
original353/R1-R9, authenticated Google, current whole Windows, main/release and
the other listed status work remain open.


The owned Windows probe creates both directories exclusively, verifies different
filesystem IDs and different original contents, and proves their full Unicode
lowercase strings collide. It performs no symlink, system or privilege operation.
RED runs two genuine tests against symbol_workspace.rs/semantic_workspace.rs
byte-identical to published1c595a9c: one fails duplicate-folder admission, and the
other observes only one initialized provider root. They remain recorded 0/2
failures. R1 and final qualify the corrected exact source, with reused rows
explicitly marked and bound to the identical frozen input manifest.

The owned fixture negotiates workspaceFolders and verifies the actual callback
matches its initialized folders. The new tests check both native symbol source
hashes/UTF-8 name ranges and unchanged immutable capture/base graph, and prove
ASCII app/APP aliases still refuse on Windows. Existing scope/path/budget,
provider forgery, refresh, staged/owned/HTTP and independent project regressions
remain covered. No Unicode normalization of source or captured paths is applied.

The installed rust-analyzer binary SHA256 is 8780a0a408169100c7b4df49ba2c9dcc4b40b48dd375e3985bd585f1a4fe0a57.
A standard owned crate supplies independent modules at the two Unicode paths;
the actual provider finds a reference with exact target hash/range. No external
dependencies or source-execution authority are supplied. This is one controlled
runtime case, not all77 or general semantic/cross-package quality certification.

The actual default comparison preserves configuration/MCP53 and literal LSP data
RAW against the old published3fb image in its original owned fixture, including
every Native envelope field except requestId/metrics.durationMs after an original
repeat proves those vary. Whole current fixture SQL is exact before and after;
no persisted database is reset or edited. Source/Git/host/old-image bytes stay exact.

All owned Jobs reach zero. All ten original symlink fixture Git/working bytes,
protected failed linker output and unrelated protected processes remain untouched.
Seven later explanatory edits are recorded in post-validation-docs.json; the
configuration-contract change only relocates the tested Unicode paragraph after
the existing paragraph to preserve its grammar. No tested behavior changes.
Original inventory assessments stay unchanged. Raw member counts are packaging,
not parity counts. Whole current Windows and other STATUS acceptance stay open.
