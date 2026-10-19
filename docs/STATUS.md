# Release status — native 0.5.0 and retained TypeScript 0.3.0

## Current integration work

Local branch `integrate/native-parity-github-app-20261018` combines the separate
`c5178b2` parity/Wave 14 and `7b8eabb` GitHub App checkpoints. Both remain absent
from `main` at the start of this integration. Their historical evidence is
preserved; it is not evidence for the combined source. The integration repairs
inherited Windows process-cleanup regressions and adds Git-compatible bounded
committer-date history selection. See [digest development](native-digest-development.md)
for remaining digest functionality. The frozen combined baseline executed all
255 native test targets: **2,272 passed, 11 failed, 37 ignored**, with unchanged
source hashes. Ten failures occurred while creating real Windows symlink fixtures
(OS 1314 on EVY); one cancellation fixture incorrectly required EOF instead of
also accepting a closed connection reported as reset/aborted. This is not a
whole-native pass, and the security fixtures remain unchanged.

The subsequent repair passes **389 library tests / 7 ignored, 28 worker tests and
four actual installed-server navigation tests**. Format and all-target Clippy
with warnings denied pass. Five actual Windows credential/TLS gates, three
expanded language-server gates, two source-execution permission gates and the
actual native HTTP browser fixture also pass separately; these counts overlap
the default ignored/scoped tests and are not added to whole-run totals. The
retained TypeScript suite passes 254 / 3 skipped and its typecheck passes.
[Integration evidence](validation/integration-2026-10-19/README.md) preserves the
baseline, repairs, source hashes, browser inspection and explicit boundaries.

CI selects Rust 1.94.1 and runs all targets despite individual failures. At
`670951b`, the actual hosted Linux run records **1,903 passed, 70 failed,
23 ignored** across 256 groups. Windows-only guarded host installation and
registry access account for most refusals; inherited Windows-specific debugger
and shell fixtures also fail. The macOS workflow now canonicalizes its temporary
root without weakening link checks. Hosted acceptance remains pending.

The integration adds an explicitly host-permitted Unix registry reader using a
pinned, read-only SQLite transaction with committed WAL visibility. The strict
Windows sharing guard and default profile bytes remain unchanged. Five local
SQLite-reader tests pass; nine CLI/session targets record **55 passed, one
failed** with unchanged sources. The failure remains an actual Windows symlink
fixture setup error (OS 1314), not a suppressed security check. Actual Unix
validation is pending. The separate permission allows SQLite coordination
sidecars and grants no registration, source acquisition or provider execution.
Portable debugger/REPL fixtures pass 28 tests with one unconfigured actual
debugpy gate ignored. The fresh TypeScript typecheck and 254 tests pass, with
three existing skips. These scoped results are not whole-native acceptance.
See the [registry contract](../native/docs/collection-registry-access.md) and
[scoped evidence](validation/integration-2026-10-19/registry-coordination/index.json).

At `d4edf7d`, the complete hosted runs remain failed: Windows **2,283 passed /
6 failed / 37 ignored**, Linux **1,944 / 38 / 23**, macOS **1,912 / 70 / 23**;
each executes 256 result groups. Windows runs the real symlink fixtures without
OS 1314. Remaining failures include provider/source binding and runtime fixture
issues on Windows, guarded host-install availability, initial SQL deadlines,
macOS PDF workers and process-group cleanup. The raw platform logs and individual
failures are [preserved separately](validation/integration-2026-10-19/platform-d4/index.json).
These results do not establish platform acceptance.

The subsequent Windows repair reproduces the React URI and registered-root
failures using a real 8.3 temporary path. Host-selected root aliases now retain
their registered query scope without filesystem lookup, persisted authority or
Unicode lowercase expansion. The protocol fixture compares equivalent decoded
file URIs and honors the requested document. The console fixture uses an owned
native kernel probe, and positive setup/session fixtures retain independent
operation/absolute-expiry checks. A further reproduced oversized-header reset
is repaired with bounded TCP shutdown after the framed refusal: at most 16 KiB
and 50 ms within the original deadline, without parsing discarded bytes.
Frozen validation passes **479 native tests / 0 failed / 7 ignored** across 12
selected groups, format and all-target Clippy with warnings denied. Eight actual
short-path checks pass ten overlapping tests. TypeScript typecheck and 254 tests
/ 3 skipped pass in the first run; its five-file native-only delta to the final
run is preserved. That first run retains its Clippy and HTTP failures. The Unix
positive archive CLI fixture now explicitly grants registry coordination.
[Repair evidence](validation/integration-2026-10-19/windows-ci-repair/index.json)
does not replace whole-native or hosted platform acceptance, which remain open.

A further scoped Windows run passes **483 native tests / 0 failed / 7 ignored**
across ten groups, plus TypeScript typecheck and 254 tests / 3 skipped. Root
selector tests now distinguish POSIX literal backslashes/prefixes/case from
Windows spellings; the Unix filesystem case awaits CI. An actual SQLite oracle
distinguishes nonblocking WAL reads from exclusive rollback-journal contention,
including expiry, cancellation and lease recovery. The digest CLI lock fixture
now establishes that real precondition without changing its 25 ms budget.
Final format/all-target Clippy pass after relocating an unchanged test module;
the exact sole inter-run delta is [proved in the evidence](validation/integration-2026-10-19/root-sql-platform/index.json).
The R8 thematic tracker reflects integrated App functionality while retaining
its remaining discussion/handoff/live gates and all inventory-wide boundaries.

The exact hosted `328ea51` follow-up executes 258 result groups per platform:
Windows **2,310 passed / 1 failed / 37 ignored**, Linux **1,964 / 39 / 23** and
macOS **1,930 / 73 / 23**. Five previous Windows failures pass; the explicitly
selected PowerShell Agent-REPL operation still exceeds its fresh deadline. The
six-job TypeScript matrix and its clean `npm ci` steps pass. These runs predate
`bf6471a` and the optional bridge repairs; they are not acceptance of those
changes. [Exact-head logs and metadata](validation/integration-2026-10-19/platform-328/index.json)
preserve every failure and keep all platform acceptance gates open.

The optional retained TypeScript bridges now have actual installed Windows
structural gates for **Graft 0.21.1 and Graphify 0.9.87**. Both preserve exact
non-BMP UTF-8/CRLF source anchors and original files. Graphify uses a separately
owned disposable cache with its scan root unchanged. Source verification now
uses the existing ordinary-file/read-only-dependency admission rules for every
captured input. Six new contract cases cover omitted-file mutation, cache/root
separation, relative-cache refusal and cleanup. Typecheck and **260 TypeScript
tests / 0 failed / 3 skipped** pass on a real 8.3 TEMP parent; two actual upstream
gates pass separately on the long owned fixture path. Published provider payloads
are unchanged. [Scoped evidence](validation/integration-2026-10-19/upstream-compatibility/index.json)
records isolated dependencies and the locally built Kotlin binding, without
upgrading full upstream parity, Serena, model/media/cloud or native acceptance.

The Windows process-environment follow-up retains `SystemDrive` in seven native
reduced-environment launchers. A real .NET Framework probe reproduces known-folder
errors without that variable; adding only `SystemDrive` removes them. Two new
process regressions fail before the patch and pass afterward, preserving secret
filtering and explicit host overrides. Normal and actual 8.3 TEMP process/REPL
runs each pass 29 tests, including the unchanged 20-second PowerShell operation.
The scoped native validation records **481 passed / 1 failed / 10 ignored**;
the unchanged media security fixture fails at real Windows symlink creation
(OS 1314). Short-TEMP counts overlap. Format, all-target Clippy, typecheck and
**260 TypeScript tests / 3 skipped** pass with 618 frozen inputs unchanged.
Four actual installed Python/TypeScript/Rust server gates pass separately.
This reproduces an environment defect, not the hosted PowerShell timeout itself:
fresh remote validation remains required. [Evidence](validation/integration-2026-10-19/windows-environment/index.json)
retains the original red tests and the unsuppressed fixture failure.

The checked Store/ledger SQLite follow-up replaces relative busy sleeps with
connection-owned absolute deadline/cancellation checks. Original regressions
reproduce a **2.26-second wait after cancellation**, an additional **632 ms**
after serialization exhausted the deadline, and an uncontended record published
after serialization triggered cancellation. Generic create/replace/delete now
recheck after serialization. Nine integration regressions and a callback-lifetime
unit case cover cancellation, finite waits, accounting recovery, exact Unicode
publication and a connection moved out of a dropped Store to another thread.
All 17 selected native groups execute: **573 passed / 2 failed / 7 ignored**.
Both failures are unchanged real symlink fixture creation (OS 1314), in
federation and maintenance. The prior contention/cleanup/shutdown tests pass.
Format, all-target Clippy, typecheck and **260 TypeScript tests / 3 skipped**
pass with all 619 frozen inputs unchanged. [Preserved evidence](validation/integration-2026-10-19/sqlite-absolute-deadline/index.json)
records both the original failures and final results.
That checkpoint leaves writable-constructor/raw-connection handling separate;
the [current deadline contract](../native/docs/sqlite-deadlines.md) records later
opening work and the pending hosted platform gates.

Writable Store opening now shares one inherited absolute deadline across
client admission/lease locks and initial SQLite schema/identity work. Five
original opening regressions fail before correction, including a 120 ms parent
ceiling permitting 3.20 seconds of setup. Seven final opening tests pass,
covering cancellation, zero/extreme budgets before state creation, brief
contention, identity/Unicode preservation and setup-policy disposal on success.
The Git mapping fixture also sets its physical executable mode on Unix before
staging and asserts clean tracked bytes/modes before capture/checkout. An owned
Windows Git probe with explicit filemode checking reproduces the original
index/worktree disagreement; all 13 Windows mapping tests pass. Actual Unix
execution remains pending. The two final native commands execute 18 disjoint
groups: **574 passed / 2 failed / 7 ignored**. Both failures remain unchanged
Windows symlink fixture creation (OS 1314). This supersedes the preceding
constructor limitation; other raw SQLite connections remain separate.
Format, all-target Clippy, typecheck and **260 TypeScript tests / 3 skipped**
pass with all 620 frozen inputs unchanged. [Evidence](validation/integration-2026-10-19/opening-and-git-mode/index.json)
preserves the original red attempts and the explicit Unix validation boundary.

The digest now has an explicit captured-history mapping path. Verified Git tree
facts describe changes over the captured path union; source-replayed Rust
associations retain original IDs, producers and exact byte evidence. Qualified
module names, duplicate candidates, uncaptured sources and absent parents remain
distinct. Captures share source, metadata, proof, analysis and encoded-row
budgets. The broad local run passes **515 native tests / 7 ignored**, typecheck
and **254 TypeScript tests / 3 skipped**. The final row-budget follow-up passes
**409 native tests / 7 ignored** with unchanged sources; these runs overlap.
The [contract](../native/docs/digest-history-mapping.md) and
[source-bound evidence](validation/integration-2026-10-19/digest-mapping/index.json)
retain their scope. That checkpoint predates the separate discussion component
below. Actual service publication, uncaptured-history integration, full upstream
export equivalence and the wider R1–R9 work remain open.

Closed-PR discussion acquisition now has an explicit host network/text grant,
bounded GitHub GETs and typed unsigned-capture replay. Description, conversation,
review and review-comment observations preserve exact UTF-8 text hashes, reported
coordinates and reply/review IDs. Repository/PR scope, rechecked PR state,
receipt queries, coverage and unacquired selections stay explicit. Offline
composition replays native source and distinguishes a verified history commit
from a selected immutable source capture; provider text grants no source, model
or execution authority. A reviewable handoff preview excludes discussion text,
diff hunks, author/title strings, commit messages and native implementation or
initializer bodies. That discussion checkpoint leaves service sending and
approval/outcome recovery unavailable; the subsequent workflow below adds an
independently owned receiver contract. See the [discussion contract](../native/docs/github-discussions.md).

The frozen discussion checkpoint passes **544 scoped native tests / 0 failed /
7 ignored** across 11 groups, **17 Python tests**, typecheck, format/all-target
Clippy and **260 TypeScript tests / 3 skipped**, with all 626 tracked validation
inputs unchanged. Three actual anonymous native HTTPS cases pass: this
repository's currently empty closed-PR list, canonical Trail-Code PR 623, and
Rust PR 163746 with **97 observations** (description 1, conversation 35, reviews
25, review comments 36). Raw foreign texts are not published. The
[evidence index](validation/integration-2026-10-19/discussions/index.json) preserves
metadata, binary/source hashes, original development failures and final logs.
These selected targets do not rerun or accept the preceding federation/
maintenance symlink setup failures or the whole-platform suite. The R8 thematic
tracker at that checkpoint keeps actual service publication, full service
acceptance and all inventory-wide certification gates open.

The subsequent [digest service client](../native/docs/digest-service.md) generates
fresh source-bound proposals and requires approval for the exact payload,
endpoint, workspace and App. Separate service/network/write host grants and
current App/service repository scopes precede publication. A domain-separated,
signed and hash-chained intent is committed before one PUT; unknown outcomes
block retries and new submissions for that repository. GET-only recovery checks
the same exact receipt, while 404 remains unknown. SQLite durability, reserved
journal/output capacity, source corruption, caller SQL policy and typed
retention are checked. Ordinary digest preview/history remains offline.

The final frozen service checkpoint passes **769 selected native tests / 0 failed /
12 ignored** across 20 groups, **17 Python tests**, typecheck, format/all-target
Clippy and **260 TypeScript tests / 3 skipped**. All **629 inputs are unchanged**.
Eighteen new integration cases and a typed MAC-manipulation unit case pass.
A separate actual Windows Credential Manager CLI gate creates/removes only its
unique signing credential and proves lost-response status/recovery across
processes without a second PUT. This is an owned socket receiver fixture.
The first broad run's 122 failures all occur in existing RSA fixture setup because
`pwsh.exe` is unavailable. An isolated, official-checksum-verified PowerShell 7.6.7
prerequisite fixes the test environment; that rerun has identical source inputs.
A subsequent two-file follow-up makes the pending App/repository fence independent
of installation changes and adds a real regression case. The complete prior
768-test checkpoint is archived separately, and all checks rerun for the final
769-test source. The new installation cannot bypass an unknown write or recover
a revoked original scope.
[Evidence](validation/integration-2026-10-19/digest-service/index.json) preserves
the initial failures, final logs, exact source/binary hashes and explicit scope.
The earlier federation/maintenance OS1314 fixture setup failures and full
platform suite are outside this selected run and remain unaccepted.

This independently owned receiver-v1 client is not an undocumented proprietary
service adapter. Receiver acceptance does not establish processing, source
execution or model quality. Configured production receiver acceptance,
automatic historical acquisition, actual App installation/deployment, full
platform acceptance and the remaining STATUS/R1–R9 requirements stay open.

The subsequent [native digest receiver](../native/docs/digest-receiver.md)
implements authenticated TCP/TLS intake for the independently owned v1 contract.
Bearer-hash grants bind the sender App/workspace/installation/repository. Exact
wire bytes, handoff identity and endpoint-bound proposal identity are checked;
an immutable record and receipt commit together before HTTP success. Identical
PUTs and GETs retain the same receipt after restart. Revocation is rechecked
after upload and serialized with commit. Original host cancellation follows the
worker, while partial uploads and shutdown keep their request bounds. Imported
snapshot/hash labels remain inert and never pin matching local captures.
The sender can explicitly select an additional host CA without disabling
certificate or hostname checks or changing the system trust store.

The frozen receiver checkpoint passes **814 selected native tests / 0 failed /
13 ignored** across 24 groups, including 14 new default receiver cases and the
existing ingress/publication/page/retention/source regression targets. **17
Python tests**, **260 TypeScript tests / 3 skipped**, typecheck, format and
all-target Clippy pass with all **634 inputs unchanged**. Both separate actual
Windows OS-credential gates pass. The new TLS gate uses real receiver/sender CLI
processes and three unique token/TLS/signing credentials, removes those entries,
and proves one accepted intake plus GET recovery after local acknowledgment
failure. Wrong trust/name, scope/revocation, restart, framing, corruption,
capacity, no late commit and detached cancellation have actual owned fixtures.
[Receiver evidence](validation/integration-2026-10-19/digest-receiver/index.json)
preserves all 34 raw artifact hashes, development failures and final logs.
That checkpoint supplies receipt-backed intake with `processingAvailable:false`. Downstream
processing/query integration, public production deployment, automatic history
acquisition, live App installation and full platform/STATUS acceptance remain
open; prior OS1314 fixture failures are not rerun or accepted by this scope.

The subsequent [received metadata query path](../native/docs/digest-query.md)
adds deterministic processing on read. HTTP queries require separate host and
per-principal query grants, both disabled by default. Local CLI queries open
existing read-only receiver state; the remote CLI makes one authenticated TLS
POST with only the service token, without a sender Store or App signing key.
Receipt-qualified JSON positions preserve duplicate rows and directed parallel
commit-parent, reply and review relationships. Literal Unicode matching,
pagination/counts and work/output/deadline limits retain unverified sender
provenance. Source authority, semantic identity, model calls and derived record
writes remain unavailable; `complete` remains false. When separately enabled,
`processingAvailable:true` denotes this metadata projection only.

The frozen query checkpoint passes **825 selected native tests / 0 failed /
13 ignored** across 24 groups, **17 Python tests** and **260 TypeScript tests /
3 existing skips**, plus typecheck, format and all-target Clippy with warnings
denied. Both separate actual Windows OS-credential gates pass. The extended TLS
gate uses real receiver/sender/recovery/query CLI processes and three unique
temporary token/TLS/signing credentials. One PUT, one recovery GET and one query
POST leave exactly one accepted record; query runs from an empty directory with
a nonexistent App signing credential. All **637 inputs remain unchanged** during
the final run. [Query evidence](validation/integration-2026-10-19/digest-query/index.json)
preserves 24 raw artifacts, frozen source hashes and Git materialization checks
for all 637 inputs: 86 raw-equal blobs and 551 explicitly recorded CRLF-only
checkout materializations. All nine changed runtime/test files are raw-equal.
Counts
overlap prior checkpoints and are not whole-suite totals. Public production,
remote source verification, semantic reasoning, automatic historical acquisition,
live App installation, full platform/STATUS acceptance and universal upstream
equivalence remain open. The R8 tracker retains all original inventory and
certification states.

The [atomic captured-history acquisition path](../native/docs/digest-history-acquisition.md)
adds explicit automatic selection/capture of one bounded history batch and direct
parents under the proven head epoch/disk-origin scope. Shared acquisition limits
precede Git bodies, and fresh source mapping plus exact output admission precede
one immediate WAL/FULL transaction committing snapshot, graph, FTS, inherited
scope, artifact registration and source receipt together. Ordinary single-commit
Git capture now shares that atomic boundary. Checkout files, Git refs/index,
latest pointer, credentials, model calls and provider/network state are unchanged.
At that checkpoint, persistent automatic continuation beyond a batch and
unattended App integration remained open; it did not close the original
full-history requirement.

The frozen full default Windows run executes **all 262 testable Cargo targets**
plus the example harness, producing **263 result groups: 2,389 passed / 10 failed /
39 ignored**. All ten new acquisition cases pass. Every failure is independently
classified as **Windows error 1314 during real symlink-fixture creation**; the
underlying safety assertion is not reached. No test is skipped or weakened.
Format, all-target Clippy with warnings denied, doctests (zero registered),
**17 Python tests**, **260 TypeScript tests / 3 existing skips**, typecheck and
both separate actual Windows credential/TLS CLI gates pass. All **640 tracked
runtime/test/configuration inputs remain byte-identical** during final validation.
The [history evidence](validation/integration-2026-10-19/digest-history-acquisition/index.json)
also preserves two earlier linker disk-space failures (zero native tests run),
the original invalid FTS-trigger fixture and Clippy finding, all successful
follow-ups and reversible build-cache compression with exact content hashes.
Only Cargo build/link concurrency changes from two to one; all target selection
and the two test threads remain intact. The default native run is not accepted.
A [hash-bound elevated replay helper](validation/integration-2026-10-19/digest-history-acquisition/WINDOWS-REPARSE-GATES.md)
for the ten prerequisite failures is prepared and parsed, but has not been run.
Windows rights are unchanged. Original inventory/certification states, the wider
STATUS/R1–R9 gaps, hosted acceptance, packaging and release remain open.

The [persistent local history jobs](../native/docs/digest-history-jobs.md)
add durable explicit continuation across batches and process restarts. Raw
hash-verified commit objects and ordered visits reconstruct parent discovery and
date priority; persisted queue/exhaustion labels carry no authority. Current host
and App grants, a frozen head/disk origin/epoch, finite cumulative quotas and exact
revisions fence each operation. Lookahead can checkpoint at object or byte-batch
boundaries without selecting a partially verified frontier. Source capture,
fresh batch mapping, job progress and output admission publish atomically.
Read-only existing-state status labels capture references as recorded progress;
it does not claim fresh replay of every historical source body. Typed retention
checks local source selectors; commit text and imported labels never become roots.

The frozen full default run executes **all 263 testable Cargo targets** plus the
example harness, **264 result groups: 2,402 passed / 10 failed / 39 ignored**.
All **13 new history-job cases** pass, including real restart, SHA-1/SHA-256,
epochs 8/9, Unicode/CRLF, branching clock skew/equal dates, cumulative budgets,
corruption, revocation, stale revisions, rollback, retention and the real CLI.
Separate **13 actual CLI host-path/protocol cases** pass, including a genuine
Windows junction ancestor, alongside a **two-process CLI revision race** with
exactly one publication and one stale refusal. Their binary hash matches the
full run. These separate gates are not added to Cargo totals and do not replace
the original symbolic-link fixtures.

Format, all-target Clippy with warnings denied, doctests (zero registered),
**17 Python tests**, **260 TypeScript tests / 3 existing skips**, typecheck and
both separate actual Windows credential/TLS CLI gates pass. All **645 recorded
runtime/test/configuration inputs** remain unchanged. Every native failure is
independently confirmed as **Windows error 1314 at real symlink-fixture creation**;
the underlying safety assertion is not reached. The default run remains
unaccepted. The [job evidence](validation/integration-2026-10-19/history-jobs/index.json)
preserves raw logs, input/binary hashes, original development failures and the
corrected proof-byte-boundary regression. Ordinary reversible compression of
3,472 owned older release build files preserves every SHA-256; no deletion,
compiler/test-option or system-security setting change occurs. A fresh
[hash-bound prerequisite replay](validation/integration-2026-10-19/history-jobs/WINDOWS-REPARSE-GATES.md)
supersedes the prior descriptor and has not been elevated/executed by the agent.
Unattended App integration, production/live App installation, remote source
verification, semantic reasoning, hosted/full STATUS acceptance and original
inventory/certification boundaries remain open.

The explicit [Git conflict triage](../native/docs/git-conflicts.md) now observes
an actual selected worktree's unmerged index, preserving every stage occurrence.
Separate host grants and checked native captured-path lineage constrain exact
Git-stage source pages. Imported/foreign-derived scopes cannot grant local Git
reads; captured read-only dependencies remain readable. Raw blob hash/UTF-8/CRLF
proofs and matching index observations do not claim canonical graph identity,
working-file resolution, predicted merge safety or automatic conflict repair.

All **12 actual Git/SQLite/CLI conflict cases** pass, including real merges,
SHA-1/SHA-256, epochs 8/9, seven constructed stage-presence patterns, independent
linked worktrees, exact source pages, budgets and SQL-policy restoration.
Seven private protocol cases are included in the **413 library passes / 7
ignored**, not added again. The frozen **21 selected native groups** record
**577 passed / 2 failed / 7 ignored**. Both failures are unchanged real Windows
symlink-fixture creation (OS 1314), before their safety assertions. Separate
**18 actual CLI host-path/protocol cases** pass with the identical final binary,
including real junctions; they do not replace symbolic-link prerequisites.

Format, all-target Clippy with warnings denied, doctests (zero registered),
**17 Python tests**, typecheck and **260 TypeScript tests / 3 existing skips**
pass with all **649 recorded inputs unchanged**. The
[scoped evidence](validation/integration-2026-10-19/git-conflicts/index.json)
preserves the original foreign-scope refusal regression, malformed-page error
marker regression, all failed/aborted development attempts, both frozen runs,
source/binary hashes and exact Git materialization. A hash-bound manual replay
for the two current symlink prerequisites is prepared, not run by the agent.
R5's current tracker now recognizes existing modified-rename/quote and captured
hierarchy implementations. Area classification, wider semantic/ownership and PR
behavior, remote freshness, whole-native/hosted/full STATUS acceptance and
original inventory/certification boundaries remain open.

Explicit [captured impact areas](../native/docs/git-areas.md) now preserve the
complete replayed impact-detail parent and add side-qualified path, occurrence
and group references. Optional saved Meaning graphs must match the workspace,
exact source snapshot, canonical graph and evidence file hashes. Smaller
supported-file sets rank first; equal minima and identical-label distinct claims
stay visibly ambiguous. The fallback is exact captured directory spelling.
There is no source capture, current-file/Git access, provider/credential/network
call, graph rewrite, persistence or automatic latest selection.

All **8 new saved Git/CAS/SQLite/CLI cases** pass, including unchanged original
quotes and parallel positions, same-name concept ties, removed working files,
provider-free offline reads, corrupt/missing Meaning refusal without fallback,
exact output limits, zero quotas and SQL-policy/transaction/cancellation guards.
Meaning setup uses an explicitly invoked, labeled local Node protocol worker;
the worker is removed before offline reads. This is not live model-quality
proof. **24 additional actual CLI host-file/protocol cases** pass on the same
final binary, including a real junction and exact 8,192/8,193-byte boundary.

The frozen **27 selected native groups** record **642 passed / 2 failed /
7 ignored**. Both failures remain unchanged real symlink-fixture creation
(OS 1314), before their safety assertions; separate junction cases do not replace
them. Format, all-target Clippy with warnings denied, doctests (zero registered),
**17 Python tests**, typecheck and **260 TypeScript tests / 3 existing skips**
pass with all **652 recorded inputs unchanged**. The
[area evidence](validation/integration-2026-10-19/git-areas/index.json) retains
raw attempts, the corrected fixture assumptions, input/binary hashes, Git
materializations and a parsed-only manual prerequisite descriptor. The R5
tracker recognizes this bounded implementation without upgrading original
inventory/certification states. Wider ownership/semantic interpretations,
compiler binding, upstream schema/quality equivalence, whole-native/hosted and
full STATUS/R1-R9 acceptance remain open.

Saved [native change plans](../native/docs/plan-integrity.md) now replay their
released identity, complete captured file delta, exact diff bytes and test-change
warnings before approval, new application, verification or recovery. This closes
five reproduced integrity gaps under the explicit condition of modified saved
plan records; it is not a demonstrated remote or cross-principal exploit.
Analyzer 8/9 IDs and serialization remain compatible. New preparation uses the
same finite serialized/source limits as replay, before derived result publication,
while retaining the native 100,000-file manifest ceiling and existing CAS/publication
boundaries. Caller transactions, SQL policy and active cancellation/deadlines survive.

All **13 integration cases**, **2 library capacity cases** and **14 separate
actual CLI cases** pass. CLI verification explicitly runs an owned Node check
against copied fixture bytes; prerequisite rows in fault-injection tests do not
assert compiler execution. The whole default Windows run executes every target
and records **2,426 passed / 28 failed / 39 ignored** across **267 groups**, with
all **654 frozen inputs unchanged**. Ten failures remain actual Windows symlink
setup (OS 1314). Eighteen are OS 32 ancestor-sharing refusals in two host-install
targets. Both complete targets pass **20 cases in a separate serial replay** of
the identical binaries, default TEMP/TMP, deadlines and assertions. Those counts
are nonadditive; the original failures remain recorded.

Format, all-target Clippy with warnings denied, doctests (zero registered),
**17 Python tests**, typecheck and **260 TypeScript tests / 3 existing skips**
pass. Two separate actual OS-credential/TLS CLI gates also pass. A post-freeze
CI control update selects one Windows fixture thread and keeps two on Unix;
in-test concurrency adversaries remain unchanged. YAML syntax was parsed with
the pinned native dependency; fresh remote CI verification remains required.
The [plan-integrity evidence](validation/integration-2026-10-19/plan-integrity/index.json)
retains all REDs, the interrupted pre-test build, the corrected CLI-fixture
connection cleanup, exact source/binary hashes and the unsuppressed whole-run
failures. Directory moves, default-safe file usage proofs, propagated deletion,
the remaining STATUS/R1-R9 work, platform/upstream certification and release
remain open. The separate directory implementation is outside this checkpoint.

Complete [captured text directory moves](../native/docs/directory-moves.md) now
bind empty directories, modes and physical parent/tree identities into one plan.
A matching host-selected standard-LSP folder request contributes checked text
repairs. Copied verification checks directory shape; approved workspace apply
and conditional recovery use one actual filesystem rename. File-only plan IDs,
host permissions, stale-source checks and recovery journals remain compatible.

The initial frozen full Windows serial-fixture run records **2,463 passed /
11 failed / 40 ignored** across **269 groups**, with every testable
target executed and **661 inputs unchanged**. 10 failures occur while creating actual Windows symlink fixtures (OS 1314), before their safety assertions. One additional failure exposed file-only jb.rename entering the directory-enabled shared helper and starting a provider. The initial full run remains failed.
All ten new library directory cases and the folder-facade cases are included in
these totals. Scoped development counts are nonadditive. A separate actual
`rust-analyzer`/`rustc` gate repairs Rust module declarations and uses, compiles
the copied result, and passes approved application and exact recovery.
Format, all-target Clippy with warnings denied, doctests, **17 Python tests**,
typecheck and **260 TypeScript tests / 3 existing skips** pass.

A separate two-input repair confines directory selection to `jb.move` and retains
file-only `jb.rename` refusal before provider startup, including a folder-capable
provider. Its scoped run records **496 passed / 1 failed /
7 ignored**; the retained failure is the unchanged OS 1314 file-delete
setup. The original failing rename case and the new analyzer 8/9 omitted/null
cases pass. These counts are nonadditive. Format, all-target Clippy, the actual
Rust provider/compiler gate and the post-repair TypeScript checks pass. Exact
source bindings identify both repaired inputs; no full native rerun after this
repair is claimed.

The [directory evidence](validation/integration-2026-10-19/directory-moves/index.json)
retains the Windows alias RED, earlier fixture assertion, Clippy diagnostic,
raw logs, exact source/binary hashes and the real junction/empty-shape cases.
The selected tree admits 1–1,000 captured text files, 2,000 directories and
16 MB counted per physical file. Multi-provider/binary trees, host-scope migration,
cross-filesystem fallback, exhaustive reference/IDE guarantees, default-safe file
usage checks, propagated deletion and the wider STATUS/R1–R9 work remain open.
The isolated file-usage follow-up is outside this checkpoint. Full local/hosted
platform acceptance, packaging, main merge and release remain open.

Default [captured file usage checks](../native/docs/file-usage.md) now prepare
deletion plans for bounded TypeScript/JavaScript ES modules of named functions.
Each declaration needs an exact source-bound provider definition and references
including itself, with no reported usage outside the selected file. Captured
graph/path/name candidates independently refuse possible usage. Missing, empty,
foreign or truncated responses refuse before plan publication. Provider policy,
source omissions and unknown consumers remain visible; `complete=false`,
`exhaustiveCompilerProof=false` and `remainingUsagesMayBreak=true` remain.
Explicit forced deletion, host edit permission, exact approval, executed
verification, stale-source checks and recovery retain their separate gates.

All **nine new cases** pass, including omitted/null selectors at both analyzer
epochs, 2,000/2,001 files, 16 MB/16 MB+1 byte, eight/nine declarations,
permissions, malformed/absent provider results and actual guarded recovery.
The final scoped run records **63 passed / 1 failed / 0 ignored** across six
groups; the sole failure remains real OS 1314 symlink setup in the unchanged
forced-delete fixture. A separate actual TypeScript language-server/compiler
gate exercises both TypeScript and JavaScript: copied compilation, approved
application, exact recovery and used-module refusal pass. It is one Rust test
with two owned fixtures and is nonadditive.

The initial 2,000-file test exceeded its unchanged 30-second deadline while
rebuilding source scope per read. An exact diagnostic RED is preserved.
Bulk reads now reuse one checked captured scope while retaining all per-file
policy, size, hash and cancellation checks; the original boundary assertion
and deadline pass. Final all-target Clippy passes. **550 scoped runtime inputs**
match the integrated checkout with recorded CRLF materialization; formatting,
**17 Python tests**, mandatory typecheck and **260 TypeScript tests / 3 existing
skips** pass with **665 integrated inputs unchanged**. The earlier 425/0/7 library
observation uses the previous private loop and remains separately labeled.
No whole-native rerun after this addition is claimed. The
[file-usage evidence](validation/integration-2026-10-19/file-usage/index.json)
retains exact attempts, source hashes and real provider/compiler outputs.

Other file/declaration/language shapes, propagated deletion, broader R1–R9,
hosted/platform acceptance, packaging, main merge and release remain open.

[Symbol deletion boundaries](../native/docs/symbol-deletion.md) now derive
TypeScript/TSX/JavaScript/JSX declaration ranges from exact captured AST/entity
bindings. Export and ambient wrappers belong to the deletion only with their
owning declaration; stored symbol and usage-evidence coordinates stay intact.
Unsupported sibling/consumer ownership refuses before plan publication.

The original actual TypeScript RED left `export` behind and failed copied
compilation with TS1030; its frozen source and output are preserved. The repair
passes **466 selected native tests / 0 failed / 7 existing ignored gates** across
seven groups, including eight new default cases. One separate actual provider/
compiler test covers four owned TypeScript/JavaScript named/default-export
fixtures: copied compilation, approved application and exact CRLF/Unicode
recovery all pass. Counts are nonadditive. Format, all-target Clippy, **17 Python
tests**, mandatory typecheck and **260 TypeScript tests / 3 existing skips** pass
with unchanged frozen inputs. [Evidence](validation/integration-2026-10-19/symbol-deletion/index.json)
binds every attempt, integrated source and Git materialization. No whole-native
rerun is claimed. Other-language attachments, general propagation, wider R1–R9,
hosted/platform acceptance, packaging, main merge and release remain open.

The subsequent [declaration attachment repair](../native/docs/symbol-deletion.md)
extends checked symbol deletion to Rust outer attributes/documentation and
Python decorators. Actual REDs show Rust moving `#[inline]` onto a following
constant and Python silently attaching a decorator to the retained function.
The Python check compiles and inspects the stdlib AST without executing project
modules. Rust reuses the existing owned attachment grammar. Exact kind binding
includes the nearest declared owner; nested functions remain functions. Python
refuses deletion that would otherwise leave a required body empty.

The final frozen source passes **545 selected native tests / 0 failed /
7 existing ignored gates** across eleven groups, including seven new
default cases and existing symbol-move contracts. Separate explicit gates pass
three Rust harness tests covering six owned Rust/Python/TS/JS fixtures: actual
compiler/AST/provider checks, approved application and exact recovery. Counts
across attempts/gates are nonadditive. The initial repair's two method-binding
refusals remain preserved with the original compiler/AST REDs.

Format, all-target Clippy, **17 Python tests**, mandatory typecheck and **260
TypeScript tests / 3 existing skips** pass with all **670 integrated inputs
unchanged**. [Evidence](validation/integration-2026-10-19/declaration-attachments/index.json)
records raw attempts, source hashes and Git/CRLF materialization. No whole-native
rerun is claimed. Other declaration/language shapes, general propagation, wider
R1–R9, hosted/platform acceptance, main merge and release remain open.

[Captured function propagation](../native/docs/propagated-delete.md) now handles
explicit symbol `propagate=true` in a finite TS/JS ES function module. Exact
provider-bound standalone call statements are removed, then newly unused
private dependencies can be pruned together, including private cycles.
Surviving reported/possible consumers and exported dependencies stay protected;
unrelated already-unused functions are preserved. Other expression consumers
refuse before publication. One ordinary guarded plan retains original byte
coordinates, permissions, approval, verification, stale-source and recovery
requirements. Unknown future/dynamic/external consumers remain explicit with
`complete=false` and `remainingUsagesMayBreak=true`.

All **20 new default contracts** pass, covering cycles/liveness, argument
dependencies, duplicate/nested calls, both epochs/force, BOM/UTF16, malformed or
omitted proofs, authority/paths, provider-time source changes/deadline and exact
declaration/statement/module/file/physical-byte quotas. The frozen selected run
passes **577 native tests / 0 failed / 7 existing ignored gates** across
fourteen groups. Five separate actual Rust harness tests cover ten owned
TS/JS/Rust/Python compiler/AST/provider/apply/recovery fixtures. Counts across
attempts/gates are nonadditive. The original capability refusal and initial
borrow-checker build failure remain preserved. A subsequent adversarial RED
showed an invalid provider comment-reference could select an unrelated unused
function. The first role predicate also admitted an Arrow parameter in a second
adversarial RED. Positive expression-role admission now refuses comments, strings,
bindings and unknown roles; all four role variants remain asserted.

Format, all-target Clippy, **17 Python tests**, mandatory typecheck and **260
TypeScript tests / 3 existing skips** pass with **674 integrated inputs unchanged**.
[Evidence](validation/integration-2026-10-19/propagated-delete/index.json) binds
source, raw attempts and Git/CRLF materialization. Maximum-inventory quota
probes intentionally stop at unavailable definition proof; they do not claim
a complete maximum-size provider workflow. No whole-native rerun is claimed.
General propagation beyond this finite policy, other R1–R9 work, hosted/platform
acceptance, main merge and release remain open.

Windows ZIP and VSIX development packages were built at `9f48353`: 313 dependency
licenses and 551 complete notice files were checked. The extracted CLI passes
index/find/context/exact Unicode-source reads with only Windows system folders
on PATH. The VSIX contains the identical binary; no new extension-host run is
claimed. These packages predate the registry follow-up and are not a release.
No live App installation, public comment, deployment or main merge is claimed;
full functionality and universal parity remain open.

## Current native Rust implementation

The default Rust executable independently owns immutable source capture/storage, 24-language structural analysis, exact UTF-8 evidence, incremental parse reuse, FTS/BM25 retrieval, bounded context plans, directed graphs/Leiden, offline exports, eight MCP tools, guarded changes/recovery and a host-selected LSP client. Graft, Serena and Graphify are unnecessary for these operations; Node/Python/model calls are unnecessary for the default native runtime.

Rust also implements service-scoped contract extraction, source-bound knowledge and statistical/external vectors, LCOV/runtime/SARIF imports, versioned unsaved overlays and authenticated loopback editor requests. Technical image/audio/video inspection, supplied transcript binding and actual PDF text extraction use native code. PDF decompression runs in a separate process with bounded memory/CPU/output and a shared deadline; it is not a general OS sandbox.

Current evidence includes Windows MSVC builds/tests, actual rust-analyzer and TypeScript language-server definition/rename runs, historical-snapshot references through actual rust-analyzer MCP, real Leiden partitions, native PDF subprocess tests and an actual VS Code 1.140 extension-host run with native 0.5. The VS Code run preserves saved source, finds unsaved Rust symbols, ignores repository-selected executables and excludes an open protected buffer. Lifecycle/cancellation races have separate labeled protocol regression tests. The unchanged standalone HTML export retains earlier actual-browser evidence against captured source, including hostile text and a narrow viewport.

Version 0.5 adds Rust-owned configured compilation/virtual builds, structural dataflow, per-test Node coverage, model protocols/accounting, local Ollama integration, persistent Python debugging/REPL, signed TLS source transport, reviewed retention/purge and Docker execution. Actual TypeScript 5.8.3 compilation, Python execution, local Linux Docker policy/cleanup and Windows credential storage have dedicated gates. The native CLI TLS service/client round trip also passed using OS-held credentials, and all four actual rust-analyzer/TypeScript/Jedi language-server gates pass. Separate actual local-model runs validate embeddings, reranking and one concise source-bound summary; unknown billing retains reservations, and neural quality/vision remain unevaluated.

The original native 0.5 workflow checkpoint passed **288 tests, with 0 failed and 10 explicit host gates ignored by default**. A separate fresh run passed nine actual host gates; the remaining actual embedding gate passed in its isolated model run. Its exact totals, logs and artifact checks remain in [native workflow validation](validation/native-workflows.json); they are historical and do not describe the expanded development suite. Current Windows parity checkpoints are tracked separately in [development gates](parity/windows-development-gates-2026-10-15.json). The historical accepted wave4 TypeScript regression suite passes on Windows/Node 24: **249 passed, 3 skipped, 0 failed**, on the accepted wave4 source manifest. The [0.4 validation](validation/native-rust.json) also remains historical. Cross-platform CI definitions and source harness checks are present; hosted Linux/macOS full-app runs and native non-Windows key stores have not been exercised on this host.

Offline Ed25519 source exchange and conservative quarantine maintenance now have independent Rust CLI/library paths and real behavioral tests. Windows Credential Manager round-trip/overwrite/concurrent-creation gates also passed. No private keys are exported or persisted in project configuration; receiving grants remain explicit host inputs. Generic artifact reads cannot bypass foreign-source grants through an unpublished raw CAS hash.

**Accepted Wave 14 Windows development checkpoint.** Tag `pdf-mixed-view-windows-20261017-r1` passed 1928 native tests/0 failures/33 ignored across 241 Cargo targets/246 groups; Python 11 and TypeScript 254/3 skipped. Its 3970-file/431-Rust source map is `536f049021a3342eaa0b8c73a9eed7dd6b4fe445f33631fc058fbc577d29d5e4`. Schema2 mixed queries now add explicitly selected registered frozen PDF pages through the separately permitted bounded native worker. Original containers are rehashed and reparsed; decoded page UTF-8 coordinates, XML and native source bytes retain separate identities. Native View/overlay/Meaning provenance, global literal paging and pre-bulk combined budgets remain. Nine new PDF regressions are included, never added to whole-project totals. Queries publish no Store/source state or provider output. Mixed semantics, cross-source graph links, OCR and rendered layout remain open; host executable trust is not binary attestation. All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows development checkpoint; universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-17-wave14/index.json).

**Accepted Wave 13 Windows development checkpoint.** Tag `outcome-feedback-windows-20261017-r1` passed 1919 native tests/0 failures/33 ignored across 240 Cargo targets/245 groups; Python 11 and TypeScript 254/3 skipped. Its 3897-file/423-Rust source map is `bd3278301478b0326a68cdd7cc709b22bded1fef0d1a794344abeaaea65650e5`. Explicit knowledge feedback now replays original View/overlay/Meaning receipts, checks exact source/transcript quotes and preserves immutable ordered event history. Ranking exposes latest actor/session-label votes, integer hyperbolic decay and zero effective scores for stale or proved authored-superseded knowledge. Plain UTF-8 transcript import is host-selected; literal candidates create no events. Twenty-one new CLI regressions are included, never added to whole-project totals. No capture, provider, project execution or automatic session discovery occurs. Broader targets and semantic transcript understanding remain open; host labels do not prove independent corroboration or execution truth. All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows development checkpoint; universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-17-wave13/index.json).

**Accepted Wave 12 Windows development checkpoint.** Tag `mixed-view-windows-20261017-r1` passed 1898 native tests/0 failures/33 ignored across 239 Cargo targets/244 groups; Python 11 and TypeScript 254/3 skipped. Its 3846-file/412-Rust source map is `67505463621444b62fdb792fb56cec5ea54f8c3a1bfac51ed84329abe320c81a`. Checked mixed views now combine one pinned source View with explicit DOCX/XLSX graphs for typed original UTF-8, decoded-text and XML reads plus globally paged literal search. Original View/overlay/Meaning packet, receipt and coverage metadata remain visible; historical replay-only scopes stay outside current search. All registrations and combined source reservations precede bulk native reads. Fifteen new CLI regressions are included, never added to whole-project totals. Queries start no provider or capture and publish no state. Processed PDF spaces, mixed semantic extraction and cross-source graph links remain open. All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows development checkpoint; universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-17-wave12/index.json).

**Accepted Wave 11 Windows development checkpoint.** Tag `meaning-join-windows-20261017-c-durable` passed 1883 native tests/0 failures/33 ignored across 238 Cargo targets/243 groups; Python 11 and TypeScript 254/3 skipped. Its 3733-file/405-Rust source map is `47ecb688e6a3beb16f7a638a93b31f3725b05a948440bbcf44715081b2dee4ec`. Explicit checked Meaning joins now group selected native graphs on one pinned canonical snapshot and provide opaque lookup/degree/directed-trace consumers. Joint frozen source/receipt replay preserves all original graph-qualified claims, evidence, coverage, parallel relations, selfloops and ordered role slots. Thirteen CLI and three asymmetric query regressions are included, never added to whole-project totals. No provider, capture or persistence is introduced. Three earlier attempts retained GitHub, debugger and durable-recovery deadline failures; the second native matrix was also incomplete after a skill_takeaways allocator abort without a harness summary. Its five partial successes remain uncounted. The recovery case also failed alone on E and passed with the identical binary/argv on C. The final functional profile runs that exact case on C and serializes complete host-protocol/query targets on E, preserving every test, deadline and durable flush. HDD/concurrent-load latency is not certified. Diagnostic and failed totals are nonadditive. Host assertions do not establish semantic identity or authenticated review; automatic deduplication, cross-history/workspace and mixed-document joins remain open. All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows development checkpoint; universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-17-wave11/index.json).

**Accepted Wave 10 Windows development checkpoint.** Tag `declaration-moves-windows-20261017-r2` passed 1867 native tests/0 failures/33 ignored across 237 Cargo targets/241 groups; Python 11 and TypeScript 254/3 skipped. Its 3683-file/401-Rust source map is `6440b361f0ab6dbbc43834da0490a4e34aa2dd4de27efe371f434cb82bb021eb`. Checked own_v1 Rust symbol moves now admit structs, enums, traits, type aliases and constants alongside free functions. Explicit negotiated exact_text_v1 attachment transfer preserves outer documentation, attributes and intervening original bytes, while graph declaration/name anchors stay distinct from the checked text range. Final AST kind, exact placement, attachment ownership, copy provenance and other preambles remain independently checked. CRLF padding, old capability envelopes and nested moved syntax are covered. Eighteen new declaration regressions and scoped94 are included, never added to whole-project totals. Compilation, attribute expansion, exhaustive reference repair, directory/module moves and proprietary IDE certification remain open. All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows checkpoint; universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-17-wave10/index.json).

**Accepted Wave 9 Windows development checkpoint.** Tag `view-comparison-cli-windows-20261017-r2` passed 1849 native tests/0 failures/33 ignored across 236 Cargo targets/240 groups; Python 11 and TypeScript 254/3 skipped. Its 3629-file/400-Rust source map is `024fe10476e773b373c7d3e53c6e9ade2da0c0368d4e9a01e8dc7b3567fa87ca`. The explicit read-only `graph compare-views --before HOST_PIN_JSON --after HOST_PIN_JSON` command admits both complete immutable selections before using the existing historical comparison core. Original directed parallel observations, source precision, selected provenance and explicit primary-file continuity remain visible; declarations do not prove renames or semantic identity. Host JSON, existing-store admission, four budgets and final newline are checked without capture, provider, logical publication or overflow artifact. Domain failures use static markers. Existing snapshot comparison, Graphify/MCP schemas and selected analysis previousSnapshotId remain unchanged. Ten new CLI regressions are included, never added to these whole-project totals; prior scoped group and Wave8 evidence remain separate historical records. A separate earlier full attempt retained three ACL-fixture setup failures under a Modify-only C: parent. The final profile uses the previously accepted user Temp parent without changing OS permissions or source; the diagnostic and failed attempt add no accepted counts. All 353 identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows checkpoint; universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-17-wave9/index.json).

**Scoped meaning group projection.** The read-only `meaning group --review HOST_JSON` CLI and library now project explicit disjoint host selections over one replayed graph. Original concepts/evidence, directed parallel relations, group-induced selfloops, every ordered hyperedge slot and original packet provenance are retained. Strict review/record/slot/output capacities refuse without persistence. This host-assertion projection does not authenticate human review or establish semantic identity. Fresh format/Clippy and 47 scoped native tests passed (25 unique group tests, nonadditive); this is not a new whole-native run. Wave8's separate 1814/0/33 whole-project evidence remains historical. All 353 identities, 73 unreconciled rows, 77 backend runtime unknowns and false broad certifications are retained. Automatic semantic decisions, cross-graph grouping and generic consumers remain open. See the [scoped index](validation/meaning-groups-windows-2026-10-17/index.json).

**Accepted Wave 8 Windows development checkpoint.** Tag `meaning-aliases-windows-20261017-serial-query` passed 1814 native tests/0 failures/33 ignored across 234 Cargo targets/238 groups; Python 11 and TypeScript 254/3 skipped. Its 3254-file/395-Rust source map is `ee3aa27d31684acd0c16e14af519929e603a187bf5a23d17ef977af2956d5934`. The read-only `meaning aliases` CLI and library replay a published meaning graph, then return deterministic lexical alias candidates with original packet-qualified concepts and exact source evidence. Unicode normalization retains combining marks and contextual Greek lowercase; comparison/output capacities refuse the whole request. No provider call, source capture, persistence or concept merge occurs. Host output security now reads stored legacy ACE flags directly through the pinned handle; mismatching historical journal hashes refuse without migration. Successful legacy one-shot LSP calls now await actual provider closure under their shared deadline before checking and removing materialized source. `semanticIdentityEstablished` and `complete` remain false. Reviewed semantic identity, cross-graph deduplication and model-assisted decisions remain open. Focused tests are nonadditive. All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications are retained. This is an unreleased Windows checkpoint; full universal parity and release remain open. See the [preserved index](validation/parity-windows-2026-10-16-wave8/index.json).

**Accepted Wave 7 Windows development checkpoint.** Tag `affected-r5b-verified-20261016-full` passed 1799 native tests/0 failures/33 ignored across 234 Cargo targets/238 groups; Python 11/0 ignored and TypeScript 250/3 skipped. Its 2873-file/385-Rust source map is `d9b2112398d06212d977212518c2377f6ed2d8df89c359a9925b1b7999bbfa31`; the [preserved index](validation/parity-windows-2026-10-16-wave7/index.json) binds all five actual phase receipts. R5B adds captured TypeScript parent/hierarchy and exact same-name method structural candidates, per-root incoming call/reference proofs and the canonical JSON `git affected` CLI. Explicit budgets retain true zero and refuse capacity without clipping proofs; recognized command/domain failures use static refusal markers. These candidates do not establish override identity, compiler binding or runtime dispatch: `complete`, `compilerBindingEstablished` and `runtimeDispatchEstablished` remain false. Full Graphify behavior/schema equivalence remains open. Focused evidence is nonadditive. Wave6 and all older records, 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain unchanged. This is an unreleased Windows-only checkpoint; universal parity, release and Linux/macOS certification remain open.

**Accepted Wave 6 Windows development checkpoint.** Tag `combined-r3-r5-20261016-1315-full` passed 1744 native tests/0 failures/33 ignored across 231 Cargo targets/235 groups; Python 11 and TypeScript 249/3 skips. Its 2419-file/373-Rust source map is `11f688628aa77bf3f50c440cd954aa49e0588800aeabb838bc17d09efe9ef30f`; the [preserved index](validation/parity-windows-2026-10-16-wave6/index.json) binds the five actual phase receipts. Historical selected-View comparison and source-bound Git rename candidates, exact UTF-8 quotes and JSON/HTML CLI are implemented, including repaired complete-store admission for healthy closed/live WAL stores and unchanged-sidecar refusal for incomplete stores. Structural matches and renames remain candidates; semantic/reference completeness, supertype/override expansion and runtime dispatch remain open. H18 and focused evidence are nonadditive. Wave5 and older records, all 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. This is an unreleased Windows-only checkpoint; universal parity, release and Linux/macOS certification remain open.

**Accepted Wave 5 Windows development checkpoint.** Tag `wave5-frozen-20261016-0654` passed 1635 native passes/0 failures/33 ignored across 218 Cargo targets/222 groups; Python 11 and TypeScript 249/3 skips. Its frozen source is 1528 files/336 Rust at `0373cf52584645b8a81d0fe3d9624b8073c9d15748f1db26a4ff53ca167a4863`; [preserved index](validation/parity-windows-2026-10-16-wave5/index.json) pins the raw five-phase receipts. Owned finite manifest_balanced_v1 retrieval admits captured strict npm/Cargo/pnpm markers and selectors, separates workspace membership from physical source ownership, uses scope-local BM25/graph scores and deterministic rank fusion, and retains exact source anchors with complete:false. Native absent/global inputs retain legacy bytes; Trail/Graft find_code privately selects observable automatic mode without new public schema/tool authority. Probe and fallback share the same source/deadline/cache/debit ledger; authority/integrity/denial/resource failures remain typed hard errors. Strict bounded saved-Graph admission and final logical compatibility-envelope admission precede their dependent decode/publication boundaries. manifest_balanced_v1 is an owned finite retrieval policy, distinct from the unrelated LSP own_v1 profile. Other repository/selector formats and richer discovery remain unsupported. Full pinned upstream ranking/ordering/quality, performance, compiler reference completeness, all-allocation bounds, whole-agent savings and non-Windows certification remain open. Earlier capture/indexing and inherited Snapshot loading retain separate boundaries. Qualified large/host fixtures remain separately labeled and nonadditive; an ignored default-suite case never becomes a default-suite pass. All 353 original inventory identities, false full-contract certifications and 77 backend runtime unknowns remain. Universal parity, release and Linux/macOS certification stay open. Older accepted matrices remain historical and focused/manual counts are never added. Only four separately reconciled metadata files follow the accepted freeze; the Serena map stays unchanged.

The accepted wave4 Windows parity development checkpoint passed **1,515 native tests, with 0 failed and 27 explicit gates ignored**, across 213 Cargo targets and 217 execution groups. All 1070 source/control files, including 313 Rust files, stayed unchanged. Formatting, all-target Clippy with warnings denied, Python packaging/11 passes and TypeScript typecheck/249 passes/3 skips use manifest `df91fac4f3bc5c3e81be4a082cd795fc5bfcd7b8844421ff6b2075dd18625063`. The [wave 4 evidence index](validation/parity-windows-2026-10-15-wave4/index.json) preserves all five phase proofs, failures and scoped observations. Historical wave3 remains 1,399/0/27 with 416 pins; wave2 remains 1,269/0/25 with 210 pins and the earlier 160-pin bundle is unchanged. Bounded own_v1 free Rust function moves and complete native typed Meaning transport/Registry/host CLI are implemented. Process lifecycle and held-parent reader repairs have separate scoped proof; counts overlap the full matrix. Exactly the listed five metadata/documents and new evidence follow the accepted freeze; runtime source remains at accepted hashes. All 353 original identities/false full certifications and 77 backend runtime unknowns remain. Universal parity, release and Linux/macOS certification remain open.

The local replacement workflows have independently owned Rust entry points and detailed [contracts](../native/docs/workflows.md). Coverage is bounded: structural taint is intraprocedural, debugging targets main-thread Python, model outputs remain unverified inference, and supplied coverage/usage is not execution or billing attestation. All-language-server certification, proprietary hosted products, whole-agent-state synchronization and whole-agent cost/correctness comparisons require separate evidence. The [replacement matrix](RUST_REPLACEMENT.md) keeps those boundaries explicit.

## Historical TypeScript 0.3.0 report

The following report preserves the earlier environment's results. Its claims that Windows/Node 24, a real browser, native Leiden or a VS Code host were unavailable are historical and are superseded by the current evidence above. It does not establish the implementation or validation of a Rust port.

**A substantially extended working implementation, not certified completion of every originally discussed possibility.** This document distinguishes working code, real local evidence, optional-provider contracts and unexecuted external gates. No public publication was performed or attempted in this update.

## Implemented and exercised locally

| Area | Delivered behavior and actual boundary |
|---|---|
| Existing core | Immutable CAS snapshots, FTS, directed multiedges, provenance, source-first context, configured TS/JS, eight MCP tools, CLI, controlled changes/recovery, verification, source-bound memory and imports remain regression-tested. |
| TS dependency/build context | Installed declaration/metadata capture is read-only. In-memory solution builds produce and consume project-reference declarations without source-tree writes. No automatic installs, external symlinked package stores or custom compiler plugins. |
| Python / Go semantics | Actual Jedi cross-file definition/reference/rename plans and actual Go `go/types` binding/typechecked rename plans. Missing dependency/context fails explicitly. Reflection/dynamic dispatch/other build configurations remain nonexhaustive. |
| External LSP | Actual clangd definition/reference/rename-plan tests, in addition to adversarial protocol fixtures. Not an all-server compatibility claim. |
| Editor | Versioned unsaved-buffer snapshots and authenticated loopback API; actual CLI-to-HTTP overlay search and original-source preservation tested. VS Code extension supplied and packaged, but no actual extension-host test. |
| Federation / sync | Ed25519 source snapshot transfer, host-controlled tenant/project grants, immutable remote snapshots, local reindexing, federated search, replay handling and receiver excludes. Real HTTP round trips and denial tests. Not whole-agent-state synchronization, hosted user management or CRDT collaboration. |
| Incrementality / retention | Metadata-assisted capture plus strict mode, semantic-unit dependency keys, filesystem watch with reconciliation, opt-in periodic retention, exclusive-client mark/sweep, quarantine and pinning. Not a fully incremental compiler; audit roots can deliberately retain history. |
| Meaning / embeddings | Native statistical lexical vector generation/search and reviewed provider interfaces for neural encoding, reranking, source-bound inferred summaries and media descriptions. Model protocol/accounting tested with labeled fixtures. No model weights, embedding-model live run or LLM run included. |
| Communities | Real NetworkX Louvain execution. Official Leiden adapter implemented, but required igraph/leidenalg packages unavailable; no mislabeled fallback. |
| Contracts | Additional Prisma fields/mappings, SQL operations/migrations, literal HTTP/OpenAPI, GraphQL and Protobuf extraction plus selected OpenAPI change checks. Cross-links remain heuristic, not complete runtime contract verification. |
| Tests / dataflow | Actual isolated Node test-file and name-pattern execution with V8 per-selection line coverage and widening on unknown impact; structured summaries reject zero-case success. Bounded TS/JS intraprocedural taint candidates with explicit source/sink assumptions; not full heap/interprocedural analysis or formal verification. |
| Media | Actual PDF text/page, PNG metadata and WAV metadata tests. ffprobe and subtitle handling implemented. ASR, OCR and scene understanding need a reviewed provider or supplied transcript; metadata is never mislabeled as semantics. |
| Process containment | Optional digest-pinned Docker verifier with no network/pulls, limits and fail-closed cleanup. Argument/security contract checked; no live Docker daemon here. Ordinary trusted copied-tree processes are not an OS sandbox. |

Exact test totals, versions and package checks are in `validation/report.json`. The synthetic microbenchmark is not an agent-quality/cost benchmark. The paired-driver acceptance harness is implemented and its accounting/independent-verifier behavior is fixture-tested; no genuine multi-arm agent campaign was run.

## External compatibility gates and remaining limits

| Gate | Observed reason / status |
|---|---|
| Installed Graft 0.21.1 | Actual Windows/Node 24 structural gate passes with UTF-8/CRLF byte anchors and unchanged sources. The unmodified Kotlin binding was built locally with MSVC; this is not full upstream certification. |
| Installed Graphify 0.9.87 | Actual Windows/Python 3.14 structural gate passes with the same source checks and an independently owned cache. Full semantic/media/cloud functionality remains separate. |
| Installed Serena | No installed reviewed executable configured. Capability discovery, selected tools, safe-edit planning and explicitly authorized runtime gateway have protocol tests only. |
| Paid / hosted features | Graphify Cloud and Serena JetBrains require the user's external products/licenses; neither source nor service is bundled. Compatibility must be established against the actual installation. |
| Clean online install | The old `EAI_AGAIN` failure is historical. Exact-head hosted `npm ci` and six Node 22/24 TypeScript jobs pass at `328ea51`; the current follow-up still awaits its own hosted matrix. |
| Live browser end-to-end | The old managed-Chromium refusal remains historical. The owned native HTTP desktop/mobile browser fixture passes separately; a full live App installation is unverified. |
| Platforms / extension host | All three hosted native platforms execute and remain failed. An earlier actual Windows VS Code 1.140/native 0.5 checkpoint exists; the combined integration VSIX has no new extension-host acceptance. |
| Neural model / Leiden / Docker | Native Leiden has actual tests. Optional model weights, Docker and external community backends retain their separate availability/live gates. Missing dependencies do not pass a gate. |
| Independent audit / full agent benchmark | Neither performed. An internal review or protocol fixture cannot substitute for independent security certification or real agent cost/correctness evidence. |

## Important non-guarantees

The static graph is not the complete runtime. A successful reference query is not proof of absence elsewhere. Python/Go/TS resolution depends on captured configurations and dependencies; dynamic/generated code, unsupported languages and omitted files remain visible limits. Source signatures authenticate bytes/scope, not correctness. Filesystem metadata mode is weaker than a strict capture. Non-cooperating external writers, an untrusted tool executable and a hostile host need operating-system-level controls beyond a library.

The facade deliberately does **not** merge all upstream applications into one implicit privileged process. The native path does not need a model. Advanced provider/network/key/retention privileges remain host CLI/library operations, not automatically granted to an MCP caller. See `ADVANCED.md`, `FEDERATION.md`, `SECURITY.md` and `UPSTREAMS.md` for operation-specific boundaries.

Version 0.3 is therefore usable for continued development and bounded local workflows; calling it universally production-ready, fully externally verified or complete feature-for-feature parity with every upstream product would be inaccurate.
