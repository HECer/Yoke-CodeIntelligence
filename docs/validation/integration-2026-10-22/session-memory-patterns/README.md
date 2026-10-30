# Layered named-memory regular expressions

Source e3c65663 adds optional readOnlyMemoryPatterns/ignoredMemoryPatterns
through global/project/project-local/context/mode/CLI layers. Bounded Rust regex
sets use canonical project names and global/name. Original glob fields/public
notes API remain unchanged; empty new fields preserve serialization/bindings.
All session-backed read/list/mutation routes use the same effective note rules. Hidden/read-only
rename references preserve their original revisions and hidden names stay absent.
These are restrictions, never a source, write, trust or execution grant.

Actual published f96b85e3 CLI RED creates three real authored notes, then lists
and reads them successfully before refusing the unsupported regex settings.
GREEN uses the SAME original source/profile/requests/notes. The original legacy
list/read raw stdout and entire logical SQL Store dump remain exact. New policy
filters archive/hidden, marks keep1 read-only and keeps keep12 readable. Reads,
writes, rename targets and attempts to disable policy through tool arguments
refuse as appropriate. Actual MCP retains the 53-tool schema catalog and returns
the same filtered list; hidden read refuses. Changed binding and invalid regex
refuse. Every original record value, note head/revision/anchor and source/Git
byte remains exact; no model or analyzed source executes.

166 distinct Rust functions pass across 26 targets, including 9 new functions,
with 3 existing ignores. All 1020 current frozen inputs remain exact. The first full Rust guard passes
all 166 functions, then Clippy refuses a
redundant return binding in the new test helper. Only that separate test file
changes. Its 9 functions pass again; the other 25 targets import 157 functions
on 1019 identical relevant inputs. Exact before/after helper bytes and hashes
are retained; no production/dependency/fixture/other test input differs. Current
1020-input integrity,
Clippy --all-targets -Dwarnings, fmt/build/typecheck and npm 260/3 existing skips
pass. All owned Jobs end empty. Existing 15 direct note functions and the prior
namespace/API/startup steering tests pass unchanged.

The initial new test run retains 8 passes/one failure: its last assertion expected
the worker namespace to survive an external stale-session error. The existing
REPL deliberately closes on that error. The corrected test retains callback-
refusal namespace survival, proves a stale cell never creates its marker,
requires a fresh resumed worker and qualifies its allowed read. Production
code did not change to accommodate the incorrect namespace assumption.
The initial full-guard Clippy failure is retained separately from functional
results. The corrected helper changes no production behavior; current Clippy,
fmt/build/typecheck/npm all pass. Reuse uses the explicitly recorded relevant
input projection, not a false claim of an identical 1020-file manifest.
The next npm attempt retains 259 passes/one existing advanced-CLI failure: Git
listing refuses before the expected PROVIDER_UNTRUSTED result. That ORIGINAL
test passes isolated without source changes. A second standard attempt fails
an existing editor-dependencies case at the same GIT_LISTING_FAILED boundary.
A metadata-only child-process diagnostic full run passes 260/3 skips; its only
recorded Git failures are fast 26/32 ms status128 events, which do not establish
the preceding failure cause. The final UNINSTRUMENTED full npm run passes 260/3
skips on the same 1020 hashes; earlier current-input Rust/Clippy/fmt/build/
typecheck logs are reused unchanged. This does not establish a Windows Git
root cause or hide the failed attempt.

Unicode, canonical global/project distinction, original globs, all six layers,
project switching, aggregate-mode capacity/CAS refusal, invalid syntax, NUL,
duplicates, compiled-size limits, empty match-all expressions, read-only host
policy and real cached Python callbacks are qualified. The public fixed pin's
regex setting facts guide independent implementation; no GPL app/prompt code
is copied. Unsupported Rust-regex constructs refuse explicitly. Python regex
equivalence and physical upstream memory-file behavior are not certified.

All 353 inventory identities/assessments and false certification flags remain.
Wider settings/client/interface/optional API work and R1-R9/Windows/main/release
acceptance stay open. Ten original OS1314 fixtures stay untouched, with no
Developer Mode, elevation, system setting, substitute, ignore or false pass.
This is a scoped checkpoint, not a whole-native rerun or release.
