# Release status â€” native 0.5.0 and retained TypeScript 0.3.0

## Session CLI answer-budget repair candidate (2026-10-31)

The Session batch path previously ignored the global `--max-bytes` selection,
counted only accumulated result bytes against 8 MB and printed an uncharged
outer wrapper. The candidate now applies the selected global cap (default
32000, allowed 1024..4000000) to every byte of the complete reply plus newline.
Wrapper, separators and changing activation metadata are charged under the
inherited deadline; final bounded serialization precedes stdout. A refused
result stops subsequent operations and reports that prior effects may already
have occurred. No batch result is persisted as a transport artifact, preserving
ephemeral statistics and private response data. Existing request/cardinality,
permission, source/state and non-atomic mutation rules remain.

Four deterministic Rust unit tests and four actual CLI fixtures cover exact
Unicode/escape/newline boundaries, aggregate/metadata charging, expired budgets,
preserved successful shape, no artifact fallback and retained earlier mutation
without a later write. An exactly full successful prefix refuses the next
operation before invocation. Native compilation and execution remain unrun locally;
this is an unqualified source candidate, without Windows/platform/main/release
acceptance. Existing PR28's browser/VSIX qualification continues independently.

## Corrected reset target and fresh TLS source confirmation (2026-10-31)

At `53a280147efaf96f8bc2f443c28aadecce3abefe`, the actual Ubuntu shard 7
passes all 20 `session_dashboard_management` tests, with zero failed, ignored,
measured or filtered. This executes the corrected malformed-label/foreign-label
refusals and all six selected statistics-management cases. The same source's
single explicitly selected Windows credential-backed TLS case passes again
(one passed, zero failed/ignored, 49 filtered). Independently verified text
artifacts bind source/control bytes, log hashes, the exact shard partition and
the TLS producer/Job lifecycle. [Receipts and scope](validation/integration-2026-10-31/history-vsix-followup/README.md#fresh-native-confirmation-at-53a28014)
preserve these as separate, nonadditive observations. Other corrected targets,
fresh Windows dashboard/statistics-browser/TSGo qualification and whole-source
Windows/platform/main/release gates remain open. The automatic browser
diagnostic was skipped because its explicit commit marker was absent; this
documentation publication supplies the existing `[browser-evidence]` marker
to request a separate fresh diagnostic without changing any workflow guard.

## Source-specific history browser and combined VSIX evidence (2026-10-31)

At `3ceb66ced10bacda9cd024d0f84a5345ea8cf80c`, four real Windows history
browser profiles pass their automation and independent artifact checks: native,
Serena, persistent Python and authenticated HTTP. The parent agent visually
inspected all 24 original desktop/mobile before/pending/after PNGs. The scoped
history visual checkpoint is accepted; human review, motion/visible-shell
acceptance, current-head acceptance and full Windows/release acceptance remain
open. The same source's combined VSIX also passes fresh package production and
both actual explicit/bundled Rust extension-host assertions in VS Code 1.143.1.
TypeScript is byte-verified as bundled content, without execution of its editor
mode. [Source-bound receipts and limits](validation/integration-2026-10-31/history-vsix-followup/README.md)
retain the exact run/artifact identities and original false broader flags.

The separate statistics browser fails before capture because its oracle counts
an MCP wrapper added after the actual compatibility result was measured. Its
candidate correction counts the original payload, adds a full-wrapper negative
case and fixed private-safe failure phases. Two native transport fixtures now
read the existing status/data envelope correctly. The decimal fixture separates
exact 599999 ms from the fractional value that rounds up to 600000 ms. The
TSGo fixture uses declared `queryProject`/`ci_find` with its nested 64000-byte
budget: `symbols.find` injects a search kind incompatible with LSP navigation.
Production counting, dispatch, validation, deadlines and permissions are
unchanged. Fresh native, TSGo and statistics-browser CI remain required.

For these repairs, local typecheck and npm tests pass (266 passed, three skipped),
as do 62 focused inert Python tests, Rust formatting and whitespace checks.
The full Python run has 318 passed, one skipped and two Cargo-required failures
because Cargo is absent from the selected PATH. No local native build or image
read was used to bypass the existing qualification boundary.

## Reset project-binding fixture correction (2026-10-31)

At `3ceb66ce`, the actual Ubuntu `session_dashboard_management` target reaches
19 passing tests and one failure: its Unicode foreign-project label expected
HTTP 409, although the existing ASCII label validator correctly returns 400.
The fixture now separately requires malformed `other🌍` to return 400 and valid
foreign `other` to return 409 with `notApplied: true`. Both must preserve the
exact statistics snapshot. Product validation and permissions are unchanged;
the corrected fixture still requires fresh native execution.

## Source-specific Windows credential TLS confirmation (2026-10-31)

At `3ceb66ced10bacda9cd024d0f84a5345ea8cf80c`, the
[Windows credential-backed TLS job](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41184768572/job/124489341264)
passes its explicitly selected existing receiver/sender recovery case using OS
credentials and the selected CA: one passed, zero failed/ignored, 49 filtered
out. Artifact `13036678194` has SHA-256
`b0c175b2c6cf9b631d388ca002a34670f8b27976dfabb0190bfd9ad065731a3f`.
Independent text-artifact checks bind the successful paired CLI/test producer,
source revision, case/log hashes, named Job membership, natural worker/tree
exit and cleanup. No native image was read or executed locally. This is one
source-specific case, without whole-target, platform, VSIX or release acceptance;
subsequent candidates retain their own fresh-source qualification gates.

## Dashboard reset compilation repair candidate (2026-10-31)

Hosted `b5acd974` Windows Clippy, credential-TLS and VSIX producers, plus three
Ubuntu shards, fail at the same `E0277`: the reset's final binding check returns
a private HTTP failure that cannot be propagated into `anyhow::Result` with
`?`. Five digest-verified text artifacts contain the same compiler failure,
including 143 shard group logs; these are compile failures, not executed test
failures. The unchanged binding check now explicitly maps refusal to a fixed
internal error. Its position after the reset, outer unconfirmed-outcome reply,
permissions and source/state checks remain. Fresh hosted compilation and full
runtime qualification remain required for subsequent candidates. The selected
`3ceb66ce` Windows TLS producer above now confirms one successful paired CLI/test
build; local native compilation remains unrun.

## Real tool statistics browser CI candidate (2026-10-31)

A separate guarded `tool-usage` browser variant now authors real owning Serena
calls, desktop/mobile filter/sort and exact count checks, Cancel without a
request, stale HTTP-409 reset refusal, and a fresh explicit reset to zero. It
retains the original fresh-producer, Windows Job, resource, privacy, lifecycle
and six-image admission guards; the four history variants remain independent.
Focused inert tests pass. Actual new-source native execution and all six CI
screenshots with visual review remain pending. This is source wiring, without
native/browser, visible-shell, full Windows, VSIX or release acceptance.
The local full Python suite reports 315 passed, one skipped and two failed
Cargo-dependent checks because Cargo is absent from the selected PATH; no
native build was attempted to bypass the current source-only qualification.

## Decimal duration projection repair candidate (2026-10-31)

The preceding binary-float multiplication can widen a decoded `2.007` second
budget to 2,008 ms. Tool timeout and symbol-information policy now share checked
decimal scaling of the canonical decoded number, yielding 2,007 ms while still
rounding genuine fractional milliseconds upward. Existing positive/zero/null,
600,000 ms, host-deadline and persisted integer-policy semantics remain separate.
An independent IEEE754/Decimal reproducer confirms the old arithmetic mismatch;
it does not execute Rust. Three shared native unit functions cover 20,000 exact
millisecond grid inputs, exponent/subnormal/fractional/overflow/type boundaries.
One new YAML integration fixture checks both projected policies. Those native
fixtures remain uncompiled/unrun locally; fresh guarded native qualification is
required. Full Windows/VSIX/main/release acceptance remains open.

## Selected dashboard tool statistics candidate (2026-10-31)

An explicitly selected estimator now adds a Statistics page with exact decimal
u64 counters, BigInt totals, per-tool filtering/sorting and relative input/output
bars. Missing/malformed data remains unavailable rather than zero. A separate,
default-false host reset grant retains current `ci_session` steering and existing
private HTTP/Session/project admission. Confirmed resets compare a checked
instance revision; new counts, replay and ABA refuse without clearing. Source,
persisted State identity and cumulative remote-attempt diagnostics are unchanged.
The browser discards pre-management state replies and reads afresh after reset.
Eight extracted client-model/orchestration tests pass. Native exact-counter,
overflow and six HTTP/permission/staleness fixtures are authored but uncompiled
and unrun locally. Owned-data desktop/mobile HTML checks are separate from native
acceptance. Fresh guarded Windows/native/VSIX/main/release qualification and full
upstream/default representation parity remain open.

## Windows hover phase-observation candidate (2026-10-31)

Completed Windows facade-options CI on `ccd7a108` passed two cases and timed out
in the requested-hover case under its original 20-second limit. That owned LSP
fixture can now opt into bounded, nonblocking loopback UDP observations containing
only closed phase/method labels, worker IDs and elapsed time. The test reports
admitted observations only after failure; missing/dropped packets prove nothing.
Unavailable observer setup leaves the original hover invocation active and is
reported as unavailable if that invocation fails.
Three authored helper tests cover closed decoding, malformed/foreign/Unicode and
oversized packets, and an empty nonblocking harvest. Five pure extracted-logic
tests pass with fake UDP, without importing or running the LSP fixture. They
cover opt-in, closed labels, packet/byte caps, send failures and elapsed bounds.
Production code, existing
assertions and deadlines are unchanged. Fresh native compilation, actual Windows
phase evidence and runtime diagnosis remain pending; this is not a timeout fix.

## Session host-option plumbing repair candidate (2026-10-31)

Completed Linux Clippy on estimator source `ccd7a108` refused three Session
helpers with eight parameters. The existing private SessionHostOptions structure
now carries template, activation and token-count selections through ordinary,
stdio, persistent REPL and Git Session preparation. The same host-derived values
reach the original binding calls in the same order; no new serialized authority,
default, permission exception or lint suppression is introduced. Existing parser
and native runtime fixtures remain unchanged. Fresh exact-head Clippy and native
qualification are still required; no local native build or Clippy pass is claimed.

## CLI argument-group repair candidate (2026-10-31)

Completed Linux and Windows CLI groups on estimator source `ccd7a108` panic
before dispatch because the two flattened `Arguments` types derive the same Clap
group ID. The token-count options now declare a unique group ID, retaining both
independent required flags and all activation/host binding checks. Three authored
native parser tests check the complete command graph, unselected defaults,
one-sided refusal and paired global flags, and coexistence with activation flags
without binding hosts, reading credentials or executing commands. Fresh Windows
TLS preparation compiled that older source but its runtime consumer failed;
neither that producer nor these source checks qualify this corrected candidate.
The parser tests remain uncompiled/unrun locally. Exact-head native, Windows,
VSIX, main and release acceptance remain open.

## Persistent callback budget assertion repair candidate (2026-10-31)

The completed Linux global-budget group passed six of seven cases; the remaining
fixture attempted to read a Python cell's structured error as a string. It now
checks RuntimeError/message, absent result, one callback and preserved worker
state, followed by the original explicit answer override, whole-cell deadline
and lost-worker assertions. The successful override additionally checks exact
captured text. No runtime, budget, protocol or permission code changed. This
corrected native fixture remains uncompiled/unrun locally and needs fresh hosted
qualification; full native/Windows/VSIX/main/release acceptance remains open.

## Compiler do-first admission repair candidate (2026-10-31)

Completed Windows and Linux body-inlining CI each passed 30/34 integration cases;
the same four do/CLI cases refused an unexplained identifier. The later native
publication inventory now includes the private do-first marker already checked
by independent source reconstruction, canonical freshness and collision gates.
The production correction is one inventory entry; existing binding/target,
diagnostic, exact-byte, budget, permission and writer/recovery checks remain.
A new authored compiler-witness unit fixture covers two do frames plus missing,
aliased/reused, foreign-target and extra-observation refusal without source or
plan/journal writes. The original 34 integration fixtures remain unchanged.
The correction and new fixture are uncompiled/unrun locally; fresh native CI,
combined Windows/platform/VSIX/main/release acceptance remain pending. The
separately observed persistent-callback global-budget fixture failure remains
open and is not certified by this repair.

## Hosted VSIX inventory and Session assertion repair candidate (2026-10-31)

The completed Windows VSIX run on `6fef75be` stopped before native production:
Git listed 19,140 paths / 1,906,519 filename bytes, including historical evidence,
before the existing documentation filter. That exceeded the original 1MB name
response cap. Git pathspecs now exclude those historical evidence classes before
the same count/byte limits, preserving the exact intended selected file set.
An inert Git-index fixture reproduces the refusal with 19,000 excluded historical
paths and checks the selected text/Unicode inputs plus path/capacity refusals;
the test is part of the existing hosted VSIX admission step. No producer, disk,
payload, owned-process or package limits are raised. Two native Session fixtures
now assert the existing status/data transport envelope and
startupProject/projectActivationLocked metadata instead of nonexistent fields.
Typecheck, 266 npm tests/three skips, 35 inert admission tests, Python AST and
format checks passed; the new inventory test reproduced the prior refusal.
Their new native runs and actual VSIX/extension-host qualification remain open.
Four Windows compiler-inline cases also remain open after an independent
identifier-proof refusal; no assertion or proof gate was weakened for them.

## Hosted HTTP startup and catalog regression repair candidate (2026-10-31)

Completed Linux CI on creation-selection source `6b5a3da4` found two failures:
authenticated HTTP host preflight required the not-yet-created automatic choice,
and the catalog CLI fixture still expected 77 instead of 80 declarations. The
HTTP preflight now validates host/registration/activation authority without
publishing a Session or selecting a backend. Authenticated initialization keeps
the original construction and selection path; public passive inspection still
refuses an uninitialized choice. Added fixtures check policy rejection and
absence of preflight snapshots/Session rows/provider startup. Four inert
declaration tests and TypeScript checks passed; the corrected native fixtures
remain uncompiled/unrun locally and require fresh hosted results. Existing Git
host preparation, resource/permission gates and unavailable live-certification
claims are unchanged. Combined native/VSIX/main/release acceptance remains open.

## Selected tool-usage estimator candidate (2026-10-31)

Explicit global token_count_estimator now has independent native CHAR_COUNT,
compiled o200k_base TIKTOKEN_GPT4O and host-authorized count-only Anthropic paths.
Bounded count-only aggregates cover successful public Serena operations once;
no source text, history root or billing assertion is retained. Remote disclosure
requires a separate revocable host grant and OS credential, with current-state
checks before/between requests, paired atomic counters and no fallback/retry.
Native raw-envelope disclosures are marked nonlocal; original byte estimates
remain explicitly scoped. Passive statistics operations avoid activation hooks;
dashboard JSON refreshes immediately. Typecheck, 266 npm tests/three skips and
twelve inert contract tests passed. Eighteen unit and twelve integration native
fixtures are authored but uncompiled/unrun locally. Exact native/provider,
Windows/resource, packaging/browser/VSIX/main/release gates remain open.
[Methods, authority and evidence limits](../native/docs/tool-usage-estimators.md).

## Creation-time language-server selection candidate (2026-10-31)

An explicit global ls_priorities/host policy now chooses one of the 80 current
backend declarations by captured filename count, positive priority and stable
public order. Exact suffix/case/superset data is pinned; source edits do not
silently change a retained Session choice. Explicit host/dashboard selections
supersede detection. Every semantic entry point refuses automatic fallback to
an unselected legacy provider; missing registrations fail without startup.
Metadata is excluded from source-retention roots, total Session serialization
keeps its original 64KB bound, and read-only dashboard data reports the choice.
Typecheck passed; npm passed 266 tests with three skips; ten inert contract
tests passed. Twelve native unit and seven integration fixtures are authored
but uncompiled/unrun locally. Exact native/hosted qualification, real vendors,
combined Windows/platform/browser/VSIX/main/release acceptance remain open.
[Selection contract and evidence limits](../native/docs/server-autoselection.md).

## Explicit global tool-budget candidate (2026-10-31)

Host-selected global tool_timeout and default_max_tool_answer_chars now project
into a separately bound Session policy. Tool/facade/persistent-cell entry points
intersect the selected clock with original host deadlines and cancellation;
only the 17 declared answer facades resolve missing/-1 to the selected default.
Per-call overrides, Unicode scalar counting, zero and independent byte ceilings
remain explicit. Empty selection retains previous profile bytes and bindings.
Two inert contract tests passed RED/GREEN; four native unit and seven integration
tests are authored but unrun locally. Source/npm checks do not establish native
deadline, Windows cleanup, platform or release acceptance. Exact hosted runtime
qualification and broader parity/VSIX/main/release gates remain open.
[Policy, admission and remaining evidence](../native/docs/global-tool-budgets.md).

## Selected context options candidate (2026-10-31)

The pinned ten-field context and seven-field mode declarations now have a
native candidate for literal description metadata, tool-description overrides,
host-startup-only single-project restriction and true/false/null structured
Serena MCP output. Null definition prompts do not invoke a renderer. Unknown,
invalid, oversized and wrong-scope inputs refuse; exclusions, source/runtime
permissions and recovery remain separate. Absent options preserve preceding
serialization and output. Structured text/object copies share the original
inline cap; no clipped success is substituted. Historical contracts/353 ledger
remain unchanged. Two inert declaration checks passed RED/GREEN; four native
unit and six integration tests are authored but unrun locally. Exact hosted,
full configuration/client/backend/platform/VSIX/main/release gates remain open.
[Actual behavior and bounds](../native/docs/context-definition-options.md).

## Current Serena Session parameter candidate (2026-10-31)

Current upstream activate_project and serena_repl declarations use session;
the historical pinned declarations used session_id. The native candidate now
advertises and accepts exactly one spelling while preserving the original
host-owned Session equality and activation/REPL runtime gates. Dual, foreign,
nonstring, unknown and oversized arguments refuse. The historical 53-tool JSON,
other 51 schemas, original 353 ledger and unrelated behavior remain unchanged.
Two inert public-data checks passed RED/GREEN. Three native unit and four
integration tests are authored but unrun locally; exact hosted qualification,
broader current parity, backends/platform/VSIX/main/release remain open.
[Current and legacy parameter contract](../native/docs/current-serena-sessions.md).

## Real Windows TSGo CI candidate (2026-10-31)

An additive hosted profile now selects the pinned real Microsoft TSGo preview
through the public native Session CLI and explicit host backend registration.
It reuses the same-run fresh release producer, private owned process trees and
unchanged VSIX gates. Vendor procurement binds SHA-512, version, archive/file
budgets and the complete extracted inventory. Unicode/CRLF fixtures require
nonempty hover, exact definition and reference byte/hash anchors, unchanged
source and natural process-tree exit. The original 353-entry ledger is unchanged.
Eleven inert admission tests cover this profile; local source/npm checks do not
qualify vendor execution. Exact hosted evidence, the other current backends,
passive TSGo behavior, broader parity/platform/VSIX/main/release remain open.
[Profile, fixed vendor and actual acceptance requirements](../scripts/host_tsgo.md).

## Current upstream backend-ID candidate (2026-10-31)

The current Serena public registry adds vbnet, typescript_tsgo and graphql to
the historical 77 IDs. A separately pinned data contract now admits all 80;
the old 77 entries/order and original 353 inventory assessments are preserved.
Explicit host registration/trust and original passive/source-execution, path,
resource and selection gates remain. GraphQL requires explicit capture language
overrides and no native GraphQL grammar is claimed. Data-contract RED/GREEN and
local source/npm checks do not qualify the four authored native fixtures or
real vendor runtimes. Combined/platform/VSIX/main/release gates stay open.
[Exact contract and remaining runtime scope](../native/docs/current-backends.md).

## Combined current-source candidate (2026-10-31)

The isolated integration branch now includes the current source from PR7-20:
trusted activation, selected encoding variants, while/for/do inlining, receipt
verification, HTTP/Store observations and both owned Windows CI profiles.
Five normal merges retain all constituent tips and both historical documentation
blocks. No production/test conflict, assertion, permission, ignore, deadline,
worker resource limit or release gate was changed during integration.
The combined product source passes typecheck, npm 266/0/3 skips, 33 inert
admission tests, Rust 2024 formatting/source parsing for 40 changed Rust files,
nine Python AST parses and three JavaScript syntax checks. These are local
source/npm results, not native compilation or combined runtime acceptance.
Fresh whole-source platform/browser/credential/VSIX/editor qualification, strict
macOS memory, full upstream/backend parity, main and release remain open.
Earlier entries retain their own source-specific scope and do not certify this
candidate. [Constituents, preserved gates and remaining acceptance](validation/integration-2026-10-31/combined-source-candidate/README.md).

## Fresh combined VSIX CI candidate (2026-10-31)

A new push-only Windows job depends on the same-run owned-Job lifecycle probe.
It requires fresh matching native release and TypeScript outputs, pinned editor
procurement, unchanged native/combined VSIX packagers, complete inventory/hash
admission, and separate real explicit/bundled Rust extension-host fixtures.
Both workspaces are fresh runner-temp roots outside the checkout. Original
test-host assertions, native importer caps and 180-second owned workers remain.
Local inert admission and source checks do not qualify actual production,
Windows kernel, package/editor execution or cleanup. Those hosted gates, full
feature/backend/platform parity, main and release acceptance remain open.
See [the exact profile and scope](../scripts/host_vscode.md).

## Owned VS Code launcher lifecycle candidate (2026-10-31)

The extension-host launcher now requires suspended Windows Job assignment,
natural zero root/tree exit before forced cleanup, and checked empty-tree/handle
cleanup before reading its result. A passed marker alone cannot establish
success. Original extension test assertions, workspace/profile fixture and
180-second ceiling remain; no PID/taskkill fallback or policy bypass is used.
Pure admission checks pass. A separate Python-only hosted lifecycle probe must
still qualify the actual kernel paths. Fresh matching native/combined VSIX,
explicit and bundled real editor execution, combined-source parity, main and
release acceptance remain open. No local Yoke/native/editor execution ran.
See [the lifecycle contract](../scripts/vscode_owned_job.md).
## Complete do-condition inlining candidate (2026-10-30)

The finite classic function-body route now admits complete do-while condition
calls with independent compiler/Rust ownership checks and a fresh first-iteration
loop recipe. The original body runs before the first test; continue reaches the
test after finally, while earlier break/return/throw leave without it. Original
source copies, labels, type checks, budgets and writer/recovery gates remain.
Three integration cases and two unit cases are authored, with an additional
CLI verification/approval/recovery fixture. Native compilation and execution
remain unrun locally; guarded hosted qualification and wider R1/full parity,
platform, VSIX/main and release acceptance remain open.

## Complete for-condition body inlining candidate (2026-10-30)

The classic body inliner has a separate ordinary-for-condition candidate atop
the while candidate. The original initializer/update header and loop body remain
source copies, preserving update/continue order and per-iteration bindings.
Rust independently checks the condition-expression and its semicolon field;
existing type/binding/source, resource and write/recovery gates remain.
Adversarial native fixtures are authored but uncompiled and unrun locally.
Both loop candidates still require guarded native qualification; full semantic
parity and release acceptance are not established by this source extension.
Local typecheck and npm tests pass (266 passed, three skipped), along with
Rust formatting, worker syntax and 20 source-only owned-fixture checks. These
checks do not compile or execute the native implementation or compiler worker.
[Candidate scope](../native/docs/compiler-inline.md#complete-for-conditions-candidate-native-qualification-pending).

## Complete while-condition body inlining candidate (2026-10-30)

The classic function-body inliner now prepares a source-reconstructed repeated
while-condition recipe. Each condition test reevaluates the original arguments
and expanded body before checking the result; labels and the original loop body
retain their ownership. Rust independently checks condition delimiters and call
ownership. Existing type/binding proofs, capacities, executed verification,
approval and recovery remain mandatory. Adversarial native fixtures are authored
for control flow, effects, predicates, byte preservation and the original writer
gates. Native compilation and fixture execution remain unrun locally; this is a
source candidate awaiting guarded CI, not new runtime or release acceptance.
Local typecheck and npm tests pass (266 passed, three skipped), as do Rust
formatting, worker syntax and source-only type checks of owned fixture inputs.
[Scope and remaining qualification](../native/docs/compiler-inline.md#complete-while-conditions-candidate-native-qualification-pending).

## Scoped f41 Windows/Linux and four-profile browser acceptance (2026-10-30)

At candidate `f41e5613`, run `41030808968`, all 371 default Cargo groups exit zero
on Windows and Linux. macOS has 360 zero-exit groups and 11 failures; strict
memory enforcement and unrelated generic startup causes remain open. All 24
shard ZIP digests, 2,226 referenced log hashes and selected source proofs were
verified. Group coverage does not imply individual test totals or every feature.

The Windows job also completes native, Serena, persistent-Python and HTTP browser
flows with all 24 original desktop/mobile before/pending/after screenshots.
Individual visual inspection confirms retained drafts, disabled pending writes,
keyboard focus and fresh post-resume saves. Independent original archive
readbacks, source/DOM hashes and owned-process exit/EOF precede cleanup.
Uploaded human/visual/visible-shell/release flags stay false; the agent review is
separate. Matching VSIX/extension host, later combined source, full parity,
main/publication and release remain open. Earlier checkpoints below retain
their original tested source and failures.
[Source-bound receipts and all 24 PNGs](validation/integration-2026-10-30/f41-ci-checkpoint/README.md).


## UTF-32 hosted checkpoint and quoted-YAML fixture correction (2026-10-30)

At source `38322c7`, all seven new UTF-32 cases and the entire 14-case transport
target pass on Windows/Linux/macOS. The 24-case core target passes 23 and fails
only a stale negative input expecting the newly supported quoted UTF-32 YAML
selection to refuse. This correction substitutes unsupported UTF-7 and adds
positive plain/quoted YAML and canonical JSON policy checks, retaining strict
JSON enum alias refusal and all original assertions/permissions/budgets.
Corrected full-core execution remains pending. Production codec code is unchanged;
strict macOS memory, full platform/VSIX/main/release acceptance remain open.

## Strict UTF-32 source candidate (2026-10-30)

The existing selected-codec stack now admits explicit BOM-selected UTF-32 and
UTF-32LE/BE source views, raw byte offsets and strict scalar/unit validation.
Existing source identities and all preceding codec behavior remain. New authored
core/stdio/HTTP/LSP/verified-writer fixtures preserve the original authority,
verification, approval, journal, deadline and cleanup requirements. Native
compilation and execution are pending; no native runtime or full encoding/parity
certificate is claimed. See [the exact contract](../native/docs/source-encodings.md#utf-32-source-candidate-2026-10-30).


## Strict Windows-1252 integration candidate (2026-10-30)

The selected Windows-1252 codec is integrated after the ASCII/UTF-8-SIG stack
at `272ec171`. It preserves all existing codec policies and refuses undefined
bytes and unrepresentable edits, while retaining original source hashes and byte
offsets. The source table matches all 256 Python strict-codec entries. New native
fixtures cover every defined scalar boundary, decoded expansion and cancellation,
YAML/capture selection, actual stdio reads/plans and the verified raw-byte writer.
Local Rust 1.94.1 formatting/source parsing, source-preservation comparisons and
whitespace checks pass. Typecheck and npm tests pass (266 passed, three skipped).
Those native fixtures have not run locally and require guarded hosted validation.
This source-only increment does not qualify other codec combinations, broader
encoding identity, macOS memory enforcement, VSIX/extension host, release or main.
## Separate Windows credential-backed TLS CI candidate (2026-10-30)

A dedicated hosted-Windows profile now prepares fresh matching CLI/test images
and explicitly selects the existing ignored receiver TLS/Credential Manager
case inside the unchanged assigned-before-resume Job launcher. Its own filtered
count stays separate from default shard coverage. Original Rust fixture bytes,
ignore, assertions, deadlines and default native jobs remain unchanged.
Synthetic admission tests pass; actual OS credentials, TLS/recovery/query
execution and cleanup still require hosted qualification. No local native
build/tests or credential access ran. Full platform/VSIX/main/release stays open.
See [the profile contract](../scripts/host_credential_tls.md).

## Windows REPL and Explorer fixture failure context (2026-10-30)

At `6c8bd90b`, [Windows shard 6](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41031531780/job/123982157572)
passes 46 of 48 target groups. A declaration-bridge REPL case exceeds its
20-second cell deadline without identifying which of four cells failed. Two
Explorer cases report readiness-channel disconnection before their HTTP
assertions; the underlying server-thread result is not retained. These logs do
not establish a receiver-verification defect or a precise startup cause.

The REPL fixture now reports a failed authored cell's SHA-256 and elapsed and
remaining time without printing its code. The Explorer fixture reuses the
reviewed PR9 startup-error forwarding, distinguishing engine opening from
serving failures through its existing channel. The original 20-second cell
deadline, five-second readiness wait, functional assertions and production
sources remain unchanged.

Rust 1.94.1 formatting/source parsing, source-preservation checks, typecheck and
npm tests pass (266 passed, three skipped). Changed native targets and Clippy
were not run locally. These diagnostics require fresh native CI qualification;
they do not establish a production fix or platform/release acceptance.
## Explorer HTTP receive observations await hosted execution (2026-10-30)

At source `85d0893d`, completed push run `41096155077` passes five of six
Windows Explorer HTTP cases; the rebuild/reload case fails because its received
bytes lack a complete HTTP header delimiter. The retained artifact cannot
distinguish empty from partial response bytes or establish the failing request,
receive duration or cause. Linux/macOS pass all six at that same source.

This candidate retains scalar receive elapsed time, byte count, read-failure and
raw OS-error flags/code, and delimiter presence in a closed formatter (maximum
169 bytes on 64-bit targets). It keeps the original read-to-EOF operation, success
parser, all six test bodies, requests, timeouts and production code unchanged.
Two added tests cover scalar extremes and an owned bounded TCP peer with empty,
partial and valid responses plus synthetic-content exclusion. Those native tests
and the eight-case target remain unrun locally and require guarded hosted CI.
Instrumentation may affect scheduling; no cause or repair is established.
Full platform, strict macOS memory, VSIX, main and release gates remain open.

## MCP HTTP incomplete-response observations (2026-10-30)

At PR9 source `9d0c4c72`, tested merge `0dc3f536`, run `41031494443`,
the MCP HTTP target passes 12 of 13 cases. The already-running reader in the
queued-principal revocation case receives EOF before a complete framed response;
the received prefix length and elapsed time were not retained. The queued
exchange's result and later authorization assertions are not established;
memory, socket and scheduling causes remain unproven.
The original 2,000 ms physical budget, 1,900 ms work ceiling, 100 ms delivery
reserve, ten-second client read timeout and all test bodies remain unchanged.

The test client now retains closed scalar connect/send/read timings, read count,
prefix length and declared frame size on failure. An added formatter adversary
covers maximum integer values and absent/zero/maximum expected sizes. An owned
TCP peer additionally exercises empty/partial EOF and response-content exclusion.
Native compilation and all 15 current target declarations require guarded hosted CI;
the observations are source-only until that execution and do not repair or
reclassify the earlier failure. Local Rust 1.94.1 formatting/source parsing and
source-preservation checks pass, as do typecheck and npm tests (266 passed,
three skipped). Full platform and release gates remain open.

## Native, Serena and HTTP browser evidence; Python focus correction (2026-10-30)

At source `abdbdb03`, diagnostic run `41013033443` completes the native, Serena
and HTTP browser flows, each with all six desktop/mobile before/pending/after
PNG/DOM pairs. All 18 successful images were individually inspected; the draft,
disabled writes, archival explanation, keyboard focus and fresh saves are
readable without blocking layout defects. Both authorities, browser/driver and
worker exit before forced cleanup. HTTP additionally proves principal/auth/origin
refusal, owning-Session revocation with the peer preserved, natural 120-second
service exits, EOF and deletion of its two fresh logon-scoped credentials.
The original three-variant history harness also passes, including all 42 original
records per variant and retained Python value/runtime exit checks. All five
artifact source maps agree: 1,101 selected Git inputs, 45 raw and 1,056 explicitly
identified LF/CRLF-only matches. This evidence qualifies those tested flows at
`abdbdb03`, not a later source or whole-project release.

Persistent-Python browser execution fails at `pending-click` on mobile, retaining
only three images and a failed worker exit. It remains unaccepted. A controlled
actual-driver-phase reproduction identifies a focus race consistent with that
checkpoint: the app can disable and blur the clicked control before its input
acknowledgement. The correction checks focus through real bounded Tab input after
the actual pending reply. It retains both exact response checks, disabled-write
assertions, keyboard skipping and final focus/screenshot requirements. The old
driver fails the regression; the correction passes all 13 protocol tests, 80
focused Python tests, typecheck and npm tests (266 passed, three skipped).
Fresh hosted qualification of this correction, full macOS memory enforcement,
VSIX/extension host, main and release remain open. [Artifacts, visual review and causal scope](validation/integration-2026-10-30/browser-abdb-checkpoint/README.md).

## Closed browser variant startup repair (2026-10-30)

At `d6b9da55`, run `41004163411`, native automation passes with all six PNG/DOM
pairs and independent zero exits/EOF/empty Job before force. The 1,099 selected
source hashes match (45 raw, 1,054 LF/CRLF-only). These new screenshots have not
been visually re-reviewed; the earlier `ed92e9f8` review retains its own scope.
Serena and persistent Python both fail before screenshots at driver startup.
Their failed Jobs require force cleanup and cannot establish independent exit.

The driver still admitted only the native evidence basename. The correction
binds a fixed payload variant to the worker environment and its exact repository
output path before creating a browser profile. A controlled old/new guard test
reproduces rejection of the three new variants and acceptance of all four exact
corrected mappings. Startup echoes the admitted variant and both Python receipt
checks require it. Failed startup closes its owned Node/channel handles.

All original browser interactions, assertions, image bounds, network admission,
waits and shutdown logic remain, as do the original history gate and copied Job
runner. Focused checks pass 80 Python and 11 Node tests; typecheck and npm tests
pass (266 passed, three skipped). These are mocked/guarded diagnostic checks,
not corrected browser runtime acceptance. Fresh four-profile CI and six-image
inspection per profile remain required. [Original failed/successful artifacts and causal scope](validation/integration-2026-10-30/browser-driver-variant-checkpoint/README.md).

## HTTP browser actor candidate and qualified ingress source integration (2026-10-30)

The opt-in HTTP browser fixture now uses two real public `serve-http` services
with fixed Session-profile principals and ephemeral CI-only logon credentials.
It preserves the original UI/archive/source assertions and distinguishes
owning-Session revocation from natural service exit. The old HTTP Session must
survive destination closure. Both roots must exit at their fixed 120-second
host lifetime with EOF and checked credential removal before Job cleanup.
No token enters arguments, fixture files or artifacts. The original stdio
profiles, history harness, CDP driver, 180-second worker, disk/output/image
bounds and existing CI commands/limits remain.

Focused Python suites pass 79 tests, including 21 HTTP parser/authentication,
credential and lifecycle adversaries. Typecheck and npm tests pass on the
integrated source (266 passed, three skipped). Six in-memory erasure checks fail as
expected. These are mocked protocol/native/credential tests, not actual Windows
Credential Manager, Yoke HTTP or browser qualification. New hosted execution
and six HTTP screenshot inspections remain open. [Candidate contract and review](validation/integration-2026-10-30/browser-http-candidate/README.md).

The monotonic ingress stimulus repair is now integrated byte-exact from the
native target that passes all 22 tests on Windows, Linux and macOS in PR8 run
`40986285723`, source `99b73b3f`. It preserves the 120 ms server deadline,
750 ms total fixture bound, three-attempt maximum and durable-state assertions.
Scoped Rust 1.94.1 formatting and source comparison pass. The combined source
still requires fresh whole-pipeline CI; strict macOS worker memory, broader
browser/backend coverage and release/main acceptance remain open.

## Guarded owning Serena and persistent-Python browser candidates (2026-10-30)

The browser harness now has separate explicit Serena and persistent-Python
variants. Both old and destination actors use the selected Serena protocol;
the real tool catalogs must match. The persistent variant also requires both
runtimes to retain 45 through the full browser/archive/save sequence, and to
exit independently before Job cleanup. The old Session must survive destination
shutdown. Variant/nonce/output and final receipt admission cannot borrow the
native flow's proof. The original history harness, CDP driver, native default
operations, six-image gate, producer guard and 180-second worker remain.

The focused Python suites pass 58 tests. The mocked exercise tests also supply
the Windows-only subprocess flag inside their mocked scope; removing that host
constant reproduces the Unix environment and now passes without skipping cases.
Actual Unix CI previously stopped before shard zero at that missing attribute. Five in-memory erasure checks fail as
expected when profile selection, runtime exit, retained value, receipt admission
or owned teardown checks are removed. Workflow YAML parsing confirms every
original job/step/command/limit is preserved; added profile invocations each have
their own original four-minute ceiling. Typecheck and npm tests pass on unchanged
TypeScript/package inputs (266 passed, three skipped). No local Yoke build or
execution was used for these new checks. [Scope and source review](validation/integration-2026-10-30/browser-profile-candidate/README.md).

New guarded hosted execution and all twelve Serena/Python screenshot inspections
remain open. HTTP browser transport, full platform qualification and release/main
acceptance remain open. The native browser result below retains its tested
`ed92e9f8` identity; this candidate does not retroactively extend it.

## Native stdio browser automation and agent visual review passed (2026-10-30)

At `ed92e9f8`, diagnostic run `40995799274`, job `123868515540`, passes the
release producer binding, all three original owning-Session history variants
and the actual installed Edge/CDP browser exercise. All six desktop/mobile
before/pending/after PNG/DOM pairs were verified and visually inspected by the
assistant. Draft retention, disabled writes during archival, readable blockers,
explicit host recovery and fresh guarded saves match their actual observations;
there is no horizontal overflow or visible layout defect requiring correction.

Browser/driver raw exit zero and EOF, both native authority exits, and an empty
worker Job precede forced cleanup. All 1,099 selected source hashes match the
recorded Git revision (45 raw, 1,054 LF/CRLF-only). The separate agent review
does not rewrite automated human/visual flags or claim visible-shell interaction.
[Exact successful artifacts, source/exit audit and six-image review](validation/integration-2026-10-30/browser-success-checkpoint/README.md).

This completes the single native owning stdio dashboard browser flow at that
source. Broader Serena/persistent-Python/HTTP browser flows, complete native and
macOS resource qualification, matching release/VSIX/extension host and main/
publication acceptance remain open. Nothing has merged.

## Earlier pending-state browser input candidate (2026-10-30)

At `42b94aca`, diagnostic run `40983256567` passes the release build and all
three original owning-Session history variants. The real-browser worker fails
at `browser-pending` after both before-review captures. Both new PNGs were
inspected and remain readable without horizontal overflow. Four later states,
successful browser automation/shutdown and full visual acceptance remain open.

An isolated owned Edge diagnostic confirms focus can leave a temporarily
disabled recovery button after the original click already observed it. The
candidate returns through bounded actual Tab input after the real pending reply;
it preserves the product and every original pending/write/recovery assertion.
Closed internal failure checkpoints retain no private endpoints or capabilities.
The actual hosted failure assertion and corrected native-browser behavior still
require fresh CI. Focused Python/Node checks, the separate copied-helper Edge
diagnostic and npm gates pass; none substitutes for the missing product states.
[Exact current checkpoint, archive/member hashes and inspection limits](validation/integration-2026-10-30/browser-pending-checkpoint/README.md).

## Partial actual desktop/mobile browser evidence (2026-10-30)

At `76333e2e`, diagnostic run `40974744304` captures both actual before-review
desktop/mobile states. Both PNGs were inspected: readable controls and content,
with no horizontal overflow. The worker then exits 1 without the earlier junction
diagnostic; its later failure phase remains unresolved. Four states, complete
automation, successful natural shutdown and full browser/visual acceptance remain
open. The next controller adds failure-only closed stage labels while preserving
all original operations, assertions, budgets and admission/cleanup requirements.
[Exact partial artifacts, source-byte checks and inspection limits](validation/integration-2026-10-30/browser-before-checkpoint/README.md).

## Scoped macOS cleanup evidence and fixture candidates (2026-10-30)

PR8 run `40950941727` at tested merge `788913c0` covers all 372 default macOS
Cargo groups: 360 exit zero and 12 fail. All 744 logs and 1,109 raw source hashes
match their receipts and candidate Git bytes. The actual library passes 506
tests with six original ignores, including all 12 strict cleanup regressions.
Process and runtime targets pass five and 15 tests respectively. This is scoped
evidence at that exact tree; the remaining failures prevent macOS acceptance.

One ingress failure has a 25 ms writer sleep that wakes after about 169 ms,
after the server closes at 134 ms. The candidate qualifies continuous traffic
and admits at most three sequential stimulus attempts within the original total
750 ms bound and unchanged 120 ms request deadline. A guarded std-only comparison
passes normal and delayed pacing and refuses renewed deadlines and invalid
stimuli. This does not execute the modified application target.

A local guarded comparison also identifies a private `INetCache/Content.IE5`
junction. The browser profile candidate precreates that exact role as an ordinary
directory; all later links still refuse. Its actual hosted route, repair and six
visual states remain unverified. Python passes 180 tests; typecheck and npm tests
pass (266 passed, three skipped). Strict macOS memory enforcement, full new-source
native CI, integrated PRs and release/main acceptance remain open.
[Source-bound receipts and 787 checked text members](validation/integration-2026-10-30/platform-stimulus-checkpoint/README.md).

## Source-bound Windows/Ubuntu native and owning-Session history confirmation (2026-10-29)

Run `40858654238` at `76035e4be3d9a9f9984364190eae9b49764a4209` passes
complete default Cargo group coverage on Windows and Ubuntu: eight complete
shards and 371 unique groups per platform, with every group exiting zero and
all 742 referenced logs per platform matching their byte counts and hashes.
Build-only examples and original ignored gates retain their own scope; group
counts do not establish individual test totals or nondefault-feature coverage.
The 1,099 source hashes match Git exactly on Ubuntu; Windows retains 45 exact
and 1,054 explicitly identified LF/CRLF-only matches.

Separate source `8f6878ae0da9197df268cac45f24fed4b24b2891` in run `40864175630`
passes the unchanged release build and all three original owning-Session history
variants, each with two packages and 42 original records read back. Root, native
authority and persistent-runtime exits precede forced cleanup. The later browser
gate still fails at private-fixture link refusal, with no accepted PNG/visual
results. At `43fc1b73`, the closed-label diagnostic confirms a junction beneath
the private home. The coherent profile at `83227913` still refuses a home junction
at depth seven. Its exact entry and origin remain unknown. Bounded route categories
and opaque component fingerprints are prepared for diagnosis; all targets remain
unread and all links still refuse. The six visual inspections remain pending.

[Retained source-bound receipts, all 2,000 text members and limits](validation/integration-2026-10-29/native-history-checkpoint/README.md).
Strict macOS memory enforcement, current-head browser/visual qualification,
integrated PRs, matching VSIX/extension host, main/publication and complete
project/release acceptance remain open. Earlier failures remain historical;
no assertion, deadline, memory ceiling, ignore or permission is relaxed.

## Owned overlay/publication admission corrected after platform CI (2026-10-27)

At published `b55064b9`, actual platform evidence identifies two construction
cycles: editor overlays request their receipt before the existing publication
step creates it, and fresh configured V9 publication uses the active V8 reader
before the new manifest exists. A private editor route now requires its exact
published legacy disk base; first disk publication validates the typed producing
receipt through the existing complete scope checker. Public historical readers,
overlay receipt ordering and all original scope/refusal assertions remain intact.
LSP admission checks immutable source/graph bindings before codec/profile lookup.
The new historical-read assertion still requires refusal before publication.

Two codec fixtures now use a portable NUL-content filename and the existing typed
snake-case approval schema. Production path protection and serialization remain.
These corrections pass scoped formatting/diff checks. Clippy first found a
missed internal Git-construction caller; the existing call boundary is restored.
The second attempt is stopped by the original disk reserve before compiler
completion, with no diagnosis observed. Neither attempt is accepted; current
typing and runtime qualification remain pending. The original 3-second DAP timeout is
unattributed and unchanged; macOS strict address-space capability remains open.

The prior-head Node CI succeeds on Ubuntu/Windows/macOS with Node 22.16.0 and 24
for both PR layers, including typecheck, tests, smoke and packaging dry-runs.
Both inspected Windows-22 logs report 267 passes, two skips and zero failures.
The exact-source Ubuntu library passes 490 tests, six original ignores and zero
failures. Full native acceptance still refuses its integration failures.
[Indexed platform evidence and limits](validation/integration-2026-10-27/b550-regression-audit/README.md).

An additional 1,943 individually checked Cargo cache files were evicted without
payload reads; their prior reported allocation totals 1,221,591,040 bytes. Sources,
selected images, PDBs and retained receipts remain. This does not demonstrate
12 GB reclaimed. The subsequent local library retry lost its process handle
during interruption and has no final lifecycle receipt; it is not acceptance.

## Legacy native construction corrected; full consumer interrupted by disk reserve (2026-10-27)

The full guarded Windows library consumer at source `bcfcb250` reports 751
passes, 38 failures and seven original ignores, with natural root/tree exit.
All four new Deadline regressions and the original source-binding test pass.
The failures share prepublication construction of genuine epoch-8 snapshots:
codec lookup used the current reader, which correctly required a published
historical manifest before the new manifest had been published.

Owned analysis now resolves its existing scope proof at the caller-supplied
construction epoch and passes that checked codec to publication. Historical
reader checks remain intact. The original directory, graph/archive and retrieval
assertions are unchanged. A fresh guarded producer at `d6cd3b4a` succeeds, with
unchanged selected inputs/protected metadata and naturally empty owned Jobs.
Its whole-library consumer is interrupted after 246.329 seconds by the unchanged
disk reserve, at a minimum 305,274,880 free bytes. No failed test is observed
before interruption, including restored directory and graph cases; there is no
complete test summary or full-suite acceptance. Forced owned-tree cleanup and
complete streams are retained. Independent post-failure source-byte checks pass,
but do not replace the controller's skipped final admission checks.

The current source has not repeated Clippy, typecheck or npm tests: storage is
below the existing admission floor. Earlier checks retain their recorded source
identities. Three individually proven, private outputs from failed producers
were removed without payload reads; their prior allocation totals 390,250,496
bytes, not the requested 12 GB. No foreign project or system setting changed.
[Raw failed/partial runs and indexed receipts](validation/integration-2026-10-27/inherited-deadline-native-construction/README.md).

## Nested Session query deadline admission corrected (2026-10-27)

Session source-query routes now intersect their parsed request timeout with the
actual inherited host Deadline instead of recreating a sampled duration without
its cancellation flags. The original ceiling and captured/live cancellation
remain attached throughout the nested operation. Public Engine timeout defaults,
numeric-range validation and document-apply provenance retain existing behavior.
Four deterministic unit regressions pass in the failed full `bcfcb250` run. They require no
provider process and retain a thread-local, one-shot test hook with RAII cleanup.

This is a separate admission defect found while reviewing the Windows provider
query timeout. It does not establish that timeout's cause. The original
source-binding assertions and 20-second request deadline remain unchanged.
Values below 10 ms were already rejected by argument validation; they did not
establish a successful 120-second provider operation. The regression control
retains that refusal and tests cancellation with valid request timeouts.

## Selected source encodings and browser harness integrated; runtime acceptance open (2026-10-27)

Strict selected UTF-16/LE/BE and Latin1 support now retains original source bytes,
hashes and ranges through decoded capture/index/read/search, reviewed encoded
changes and private LSP navigation copies. Parent review corrects symbol-body
decoding and rejects unsupported UTF-8 proof consumers before reinterpretation.
At `b55064b9`, the codec integration targets execute with 14/1 core and 4/1
transport pass/fail counts; the two fixture defects are corrected above and await
fresh execution. Guarded Windows all-target Clippy passes at `ceb23aa9`; its 759
artifact observations are metadata only and retain that source identity.

The separate real Edge/CDP history harness preserves the original three-variant
gate, permissions, deadlines, resource caps and ownership proofs. Fifteen focused
Python checks, including six in-memory CDP shutdown regressions, pass. Actual
browser interaction and all six desktop/mobile PNG inspections remain pending.
Typecheck/npm pass at integration source `6719c3dc`: 266 passed, three original
skips, zero failures, unchanged selected inputs and natural owned-tree exit.
The full Python suite remains failed on two unavailable-Cargo PATH prerequisites.
[Source-bound review evidence](validation/integration-2026-10-27/encoding-browser-review/README.md).

Published baseline `1c8d3072` / merge `23284fe6` passes all eight startup-channel
tests on Windows and macOS and final Ubuntu acceptance. Windows still fails its
original 20-second source-bound provider query, preventing release/history;
macOS still has original memory/timing failures. New integration platform,
encoding/browser/visual, matching VSIX and full release acceptance remain open.

The separate combined activation check at `dc9a4f0f` has Cargo/Clippy exit zero
without diagnostics, but remains failed under the unchanged owner guard: a
membership-confirmed MSVC `vctip.exe` helper survives the original drain until
forced cleanup. No native image is read and no tests execute.
[Incomplete combined receipt](validation/integration-2026-10-27/encoding-browser-review/activation-combined-clippy.json).

## Page-capacity fixture preparation batched; Rust acceptance pending (2026-10-26)

Source `33dfe1db` seeds the existing 999/1000 signed foreign page revisions in
one fixture transaction. The original inherited 30-second deadline, production
operations, hard bound and all success/refusal assertions remain unchanged.
Scoped formatting, diff checks and independent source review pass. Actual
typecheck and npm pass: 266 passed, three original skips, zero failures, with
1,136 selected inputs unchanged and naturally empty owned Jobs before force.

The prior `47142096` CI artifacts cover 369 unique Windows groups at PR merge
`5a94df35`: 368 exit zero; only the page-publication group fails its capacity
fixture at preview with an expired deadline. All eight ZIP digests and 738 log
receipts match. This locates the failure without proving its sole cause or the
new fixture's runtime success. Full Rust/shard, platform and release acceptance
remain open. [Scoped evidence](validation/integration-2026-10-26/pages-capacity-fixture/README.md).

## Owner-retirement editor instruction corrected (2026-10-26)

At product source `6a8a2b10`, a retirement blocker retains its distinct editor
reason instead of instructing ordinary pending-journal recovery. Drafts, reviewed
plans, disabled writes, original permissions and stale-binding checks remain.
Six actual VM/DOM protocol cases first show five passes/one intended failure on
the old message, then all six pass after the two-line fix. Typecheck and full npm
pass: 266 passed, three original skips, zero failures. Both changed product blobs
match the checked working bytes; no current native build is inferred.

Visible inert Edge fixtures confirm the corrected editor message and retained
Unicode draft/review at desktop width and an actual 390-CSS-pixel viewport. Later
runs have complete streams and naturally empty owned Jobs before force under the
original deadlines/profile caps. Earlier controller/lifecycle failures remain
retained. A host dialog occludes part of the desktop image; full-layout acceptance
is not claimed. These fixtures start no native Session or real write/recovery.
Current native/backend/browser, platform, release and full project acceptance
remain open. [Scoped status](validation/integration-2026-10-26/dashboard-retirement-message/README.md).

## Actual purge, generated-owner retirement and retained reads (2026-10-26)

A separate source-`704b647d` Windows consumer passes 95 actual CLI cases in each
native/Serena fixture: 88 success exits 0 and seven expected refusal exits 1.
Its 72 protocol/helper guards pass with zero skips; those are not extra runtime
coverage. Each fixture archives 42 original records as 28+14 and performs 70 exact
record reads, including all 14 remaining records after purge and after retirement.

First-frame purge requires fresh review, exact confirmation, host write permission
and separate state-deletion permission. Missing grants, wrong confirmation and
stale review refuse without mutation. Actual purge confirms 31,666 frame-content
bytes removed per fixture, zero unverified removal, unavailable exact record
evidence and retained receipts. It claims neither measured disk space nor secure
erasure. The second frame remains exact.

The migrated owner's before/after generation retains the migration nonce through
logical retirement. Applying its previously reviewed second-purge plan refuses;
all sibling reads and historical purge/archive inspections still succeed. Exact
configuration/source/owner identity and owning HTTP boundaries remain intact.
All four owning Sessions exit 0 with stdin open and the Job is empty before force;
complete source/control/protected/image and producer/version checks pass.

This qualifies the recorded terminal combination at source `704b647d`, not a new
current-head release. All 20 selected maintenance/dashboard/history-harness sources
match `711adf54`. Actual interruption, recovery/cancellation, unverified unlink
attribution, older binaries, browser/mobile and full project/release acceptance
remain open. [Actual cross-maintenance evidence](validation/integration-2026-10-26/cross-maintenance-live/README.md).

## Actual owner-retirement CLI and owning HTTP checks (2026-10-26)

The existing qualified `704b647d` release CLI now passes a separate live logical
owner-retirement consumer in two independent native/Serena owning Sessions. Each
performs eight actual CLI cases: plan/apply/inspect succeed; five intended
permission/selection/marker/replay refusals retain raw exit 1 and empty stdout.
Permission, second-selection and pending-marker causes are checked privately.
Its 66 protocol/helper guards pass with zero skips; guard counts are not runtime coverage.

Actual retirement commits the schema-3 tombstone and retained terminal marker,
preserving the fixed owner identity, configuration bytes/identity, all seven
original journal records per fixture and inert analyzed source. Owning HTTP reads
stay exact; cached/new writes and ordinary recovery refuse. The explicit blocker
reports incomplete state. Both Sessions actually exit 0 with stdin still open,
and the owned Job becomes empty before forced cleanup. Complete matching producer,
version, source/control/protected/image checks and context closure succeed.

The owned foreign-marker case proves presence-based blocking, not an interrupted
retirement. Actual interruption/event-reviewed recovery, migrated generations,
older binaries, retained archive reads after retirement, purge and desktop/mobile
visual checks remain open. Relevant maintenance source bytes match `2c49bf18`;
the executable and this acceptance keep source `704b647d`, with no current-head
release or full-project qualification. [Actual source-bound receipts](validation/integration-2026-10-26/owner-retirement-live/README.md).

## Current Windows unit checks pass; platform acceptance remains open (2026-10-26)

At exact clean source `b3a87d5d`, fresh private producers and complete harnesses
pass 773 library tests (seven original ignores, zero failures or filters) and
16 CLI unit tests (zero failures, ignores or filters). Workspace/all-target Clippy
passes with warnings denied; its 757 Cargo artifact observations are metadata.
Owned roots/Jobs exit naturally before force, streams complete, and the selected
1,050 source inputs, controls and protected metadata remain unchanged.

A separate strict metadata controller corrects the known CPython path/handle
mode-domain mismatch while retaining both raw observations and exact identity,
attribute and mode checks. Its 116 protocol/helper guards pass with no skips;
the actual metadata-only preflight covers all ten protected roles. Original
controls and assertions remain intact. The earlier pre-Cargo refusal and later
disk-reserve build failure remain failed; a separately inventoried reproducible
cache eviction enabled the fresh accepted producer.

The actual `771833bb` typecheck/npm results (260 passed, three original skips)
have identical selected 1,135 source files and seven controls at `b3a87d5d`;
those commands were not rerun. These local unit checks do not certify hosted or
Unix platforms, integration targets, live maintenance, a matching release/VSIX,
or full project completion. Historical `704b647d` release evidence keeps its
original source identity. [Current source-bound evidence](validation/integration-2026-10-26/current-windows-unit-metadata/README.md).

## HTTP completion-path diagnosis pending (2026-10-26)

At source `52b746dc`, macOS shard 2's backpressure fixture reports no returned
failure, 131,072 accepted prefill bytes and 257.354 ms elapsed. This does not
identify the selected HTTP status: serialization expiry can select the existing
best-effort HTTP 408 path and return `Ok`. Ordinary response completion also
returns `Ok`; earlier `WouldBlock` and post-result elapsed time do not choose
between them or establish a production deadline defect.

Source `771833bb` adds only a test success receipt with selected status,
serialization success and byte count to distinguish those paths on failure.
Non-test success remains unit; no production branch, socket operation, clock
sample, original 250-ms deadline, payload, buffer/fill/wait bound or timeout
assertion changes. The original source restores exactly after removing these
explicit diagnostics. Scoped formatting, typecheck and npm pass (260 passed,
three original skips), with 1,135 unchanged inputs and natural owned-tree exit.
At this earlier diagnostic checkpoint the changed native library, all-target
Clippy and actual macOS diagnostic were unrun locally. The current Windows unit
and Clippy results above supersede only those local unrun checks. Known history
timing, PDF memory and template/prompt failures
remain separate; no current platform or release acceptance is claimed.
[Original failure and diagnostic checks](validation/integration-2026-10-26/editor-http-response-completion/README.md).

## Inspection deadline and ingress response evidence pending (2026-10-26)

Source `34068f53` adds failure-only observations to two owned integration-test
helpers. The inspection helper records method/range, Session-open and facade
phases, elapsed/remaining budget and full error context. The HTTP helper keeps
read error kind/OS code, retained response byte count, complete header boundary
and a 503-prefix classification. Read errors still fail; the original 20-second
deadline, socket timeouts, exact 503, 750-ms capacity and queue assertions remain.

The triggering Windows failure at `f8b02d4e` is one unidentified inspection
request deadline; its 35.88-second group duration covers all four tests. The
separate macOS push has seven incomplete shards and one complete shard, with
identical selected sources. Its capacity case reports a connection reset, in
addition to known memory-limit failures. The receipt does not establish whether
the client received any complete 503 response or which close sequence occurred.

Scoped Rust formatting, typecheck and npm pass (260 passed, three original
skips), with 1,135 unchanged selected inputs and natural owned-tree exit. The
affected native integration groups and all-target Clippy remain unrun locally
at this diagnostic source. No runtime repair or platform acceptance is claimed.
[Bounded historical evidence and current checks](validation/integration-2026-10-26/ci-boundary-diagnostics/README.md).

## Runtime startup ownership candidate (2026-10-27)

Startup failures now request cancellation and wait for the process owner's
checked cleanup receipt before returning. Missing receipts preserve typed
cleanup uncertainty; receipt errors retain both causes. The owner checks the
original deadline and cancellation before spawn, and pipe setup, disconnected
startup delivery and normal observation share one checked post-spawn cleanup.
The existing two-second cleanup allowance and Session lifetime are unchanged.

Eight scripted-channel regressions cover successful startup without consuming the
live owner receipt, reported failure, expired/disconnected startup, refused or
missing receipts, owner observation errors, cancellation after selecting a wait
budget, and cancellation alongside successful pipe delivery. Startup receives
recheck the original cancellation/deadline in at most 10-ms channel waits and
refuse late pipes without extending the cleanup allowance. They exercise startup
admission without launching Python. Runtime execution and the new tests remain
unqualified until current-source library and actual runtime/CLI integration checks pass.
The older evidence below applies to its recorded source only.

## Direct worker resource-error evidence candidate (2026-10-27)

Direct Unix `setrlimit` and `getrlimit` failures now retain their immediate
`std::io::Error` cause and a fixed `RLIMIT_AS` phase; installation also records
the already validated requested byte ceiling. The original top-level refusal
messages, limit values, pre-input ordering and Windows behavior are unchanged.
Static validation of this candidate does not invoke either resource API.

This does not attribute a generic fork/exec error to resource setup or establish
a conforming macOS backend. Static source/format checks are separate from actual
current-source worker execution, which remains pending. The original memory and
timing acceptance gates remain intact.

## Combined platform checkpoint before the cancellation correction (2026-10-27)

At PR8 source `d8533de2`, checked out as merge `48fe814d`, the
[Windows native acceptance job](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40439659150/job/121990930582)
passes all 370 default target groups exactly once across eight shards, two actual
rust-analyzer checks and a release build. The library passes 788 tests with seven
original ignores, including all six original startup-channel tests. Actual
activation, process, runtime and agent-REPL groups pass 27, 8, 15 and 22 tests.
This evidence precedes the two newly authored cancellation regressions above.

The retained Windows history receipt verifies 162 CLI command outcomes and 126
exact archive-record readbacks across native, Serena and persistent-Python owning
stdio Sessions. Both authorities refuse pending archival, a fresh destination
save succeeds, and roots plus managed descendants exit before forced cleanup.
Loopback dashboard state/management calls are included; separate serve-http and
browser qualification, matching VSIX and full release publication remain open.

The same run's Ubuntu native acceptance and all six Node jobs succeed. macOS
activation passes 28 tests, runtime 15 and the original six startup-channel cases.
Its six failed shards retain 43 typed PDF address-space-limit refusals, 25 prompt
failures and one independent history timing-fixture failure. The latter's first
40-ms sleep returns at 120.487 ms, past the unchanged 65-ms deadline; production
correctly refuses the subsequent body read. Darwin 25.6 arm64 still refuses the
original 268,435,456-byte address-space ceiling with errno 22. No alternative
memory contract, timing-gate relaxation or full macOS acceptance is claimed.

## Runtime response and batch shutdown checks (2026-10-26)

Source `704b647d` checks the original Session lifetime again after terminal
cleanup. A typed `Close` requires an exact `closed` response before state
publication. The CLI confirms shutdown of a paused finite batch before output,
including an empty batch, and retains only the original requested observations.
Cleanup refusal keeps precedence; the existing two-second stop bound remains.

On that exact clean Windows source, the whole library passes 773 tests with
zero failures, seven original ignores and zero filtered tests. The whole CLI
unit harness passes 16 with zero failures, ignores or filters. Both use fresh
matching private producers, complete streams and empty owned Jobs before force.
These unit suites do not stand in for actual Python/CLI integration acceptance.

Typecheck and npm pass (260 passed, three original skips). Workspace/all-target
Clippy with warnings denied passes; its 757 Cargo artifact observations are
metadata. Separate unit/history/runtime/package protocol guards pass
72/41/25/53 tests respectively; the 72 unit-controller guards were run at
the earlier `809bac07` source with the same frozen controls. The 21 visual
guards pass at `704b647d`. Guard counts are not additional runtime coverage.
The matching fresh release CLI reports `yoke 0.5.0` and completes naturally in
1138.484 seconds. Native, Serena and persistent-Python history each read all
42 original records in 28+14; the owning Sessions and selected persistent
runtime exit before cleanup. Six actual CLI cases pass, including two intended
Close failures with raw exit 1, no successful output and checked empty trees.

The matching Windows native archive (1,339 members, 316 dependencies and 557
license texts), native-only VSIX (1,344 members), and actual bundled-runtime
VS Code host pass. The first package attempt refuses a missing base64-simd
license; a fresh run uses the existing explicit license-fetch option. A separate
visible run passes after actual inspection of overlay JSON and dirty Rust source.
The parent observes acknowledgement validation at 58.759 seconds within the original 90-second
window, followed by natural root/Job exit and unchanged final pins. An earlier
visible attempt expires and remains failed. Full platform/contracts, main and
release publication remain open.
[Runtime, package and inspected-image evidence](validation/integration-2026-10-26/runtime-shutdown-windows/README.md).

## Fixed LLD CLI and original history pass (2026-10-26)

At source `e89ae42c`, a separate fixed final-bin bundled-LLD transaction passes
50 compiler guards, a fresh CLI build and matching version. Natural inner and
outer Job emptiness precedes forced cleanup; the original 3600/60/1/2-second
limits and resource floors remain. The build takes 505.378 seconds, including
0.010459 seconds of natural post-Cargo quiescence. Release settings remain thin
LTO, one codegen unit and symbol stripping. No old result is relabeled.

The matching history gate passes 41 guards and actual native, Serena and
persistent-Python variants. Each reads all 42 original records across 28+14;
owning Sessions and the persistent runtime exit before cleanup. All source,
control, tool/profile and version receipts agree. Independent byte-level
review verifies the 273-member text/metadata evidence packet.

The two earlier e89 MSVC producers remain failed and unadopted. Caches and
installed tools remain trusted; the linker experiment does not exclusively
attribute their helper behavior. This checkpoint does not qualify a new VSIX,
current-head/full-platform acceptance, main, publication or the whole project.
The accompanying response-boundary source changes preserve cleanup precedence
and original CLI events; their unit/live acceptance is a separate scope.
[Scoped evidence and retained failures](validation/integration-2026-10-26/lld-cli-original-history/README.md).

## Windows native package, bundled editor and desktop inspection pass (2026-10-26)

A separately reviewed paired-source gate now qualifies the admitted f8 runtime
with the e89 packager. Both clean roots retain 1,050 exact inputs: 1,048 common
records and all 1,006 native entries match; only two pinned packaging Python
files differ, with the exact three-file Git delta. Initial and late untracked
Cargo configuration refuses. Full locked dependency identities, features and
edges match through an explicit root-package mapping. No build is relabeled.

All 62 guard tests pass without skips. Actual native ZIP, VSIX and bundled
VS Code 1.143.1 host stages pass with root exit and empty Jobs before cleanup.
The native archive has 1,337 members, 316 dependencies and 557 license texts;
the native-only VSIX has 1,342 members. Both carry the same admitted 0.5.0 CLI.
The original JavaScript host assertions pass under a derived launcher that
refuses internal forced cleanup. A separate visible fixture passes after actual
inspection of generated overlay JSON and dirty Rust source, then ends naturally.

The first guard fixture setup error, missing-PowerShell package run and expired
visual acknowledgement remain failed. Portable host tools use private test
locations and per-child environment selection; no global policy changes occur.
These are Windows development packages with runtime source f8b02d4e and packaging
source e89ae42c. Exact-current-head build, full platform/contracts, live
maintenance/mobile, main and release publication remain open. These paired-package receipts predate the separate LLD CLI/history checkpoint above.
[Evidence, inspected images and retained failures](validation/integration-2026-10-26/paired-runtime-vsix/README.md).

## Local release CLI and original history pass; current build and VSIX remain open (2026-10-26)

At runtime source `f8b02d4e`, a separately versioned compiler transaction
passes with Cargo exit 0 and natural inner/outer Job emptiness. Its original
one-hour build limit, at most 60 seconds of natural inner quiescence,
one-second outer drain and sole two-second abort remain explicit. The
earlier direct-Cargo producer stays rejected; the new policy does not
retrospectively pass its timing gate. The admitted CLI reports `yoke 0.5.0`.

Actual native, Serena and persistent-Python history variants each read back
all 42 original records from packages of 28 and 14. Both owning Sessions and
the persistent runtime exit before cleanup. HTTP pending refusal under both
authorities, exact owner/configuration bytes and a fresh destination save pass.
Compiler guards pass 35 tests; current history-admission guards pass 33.

At packaging source `e89ae42c`, the manifest-selected Windows executable may
occupy 160 MB; ordinary/Unix entries remain 128 MB and aggregate payload
remains 256 MB. All 17 packaging tests, typecheck and npm 260 passed/3 original
skips pass with natural tree exit. These are separate, nonadditive scopes.
Both later e89 release builds fail natural compiler-helper quiescence after
Cargo exit 0; neither private image is read, admitted or executed. Current-head
runtime, actual VSIX/bundled host, browser, maintenance, full platform, main
and project/release qualification remain open.
[Evidence and retained failures](validation/integration-2026-10-26/release-cli-original-history/README.md).

## Owned Windows fixtures declare their Utility dependency (2026-10-26)

At source `6c522411`, positive PowerShell fixtures explicitly import the fixed OS
Utility manifest inside their original 20-second operation budget. Original
payload suffixes, assertions, quoting, authority checks and namespace persistence
remain. Missing import must exit 125 before payload entry; a foreign module path
cannot replace that selected fixture dependency. Production launchers and raw
diagnostic commands remain unchanged. Raw observations now always run/publish
on Windows shard 0, even when prepared fixtures pass. Cold lookup timeouts remain
unqualified; this is a disclosed test-setup correction, not a cold-start repair.

Typecheck, npm 260/0/3 original skips, Python 150 tests, all native format roots
and Actionlint pass functionally. Python lifecycle remains incomplete because
the owned compiler helper survives the original drain. Warnings-denied all-target
Clippy completes with natural owned-tree exit. A Cargo-0 process producer has the
same helper remainder and is rejected before image admission; local process/REPL
execution is unrun. Cargo also reports ancillary standard CLI/PDB rebuilds that
the controller's selected-image guard did not fence. Further local integration
producers require isolated outputs. Matching hosted acceptance remains pending.
[Evidence](validation/integration-2026-10-26/windows-fixture-initialization/README.md).

## Windows command lookup: stalled boundary observed (2026-10-25)

At `0d0b43e3`, the fresh cmdlet-only lookup does not return before 20 seconds.
A fresh fixed Utility import followed by lookup and command-object invocation
finishes in 0.572 seconds with natural tree exit. This changes two dimensions;
normal string-name invocation after import is not yet established.

A separate new control prepends that import to the exact original folder
script. Its ordinary `Write-Output`, environment, two snapshot Jobs, original
20-second deadline and two-second cleanup remain. It passes locally in 0.242
seconds with exact output, all original markers and empty Jobs before force.
Typecheck, npm 260/0/3 original skips and Actionlint pass. All 147 Python tests
pass, but the owned compiler helper still makes their lifecycle receipt
incomplete. Hosted results and any production repair remain pending.
[Evidence](validation/integration-2026-10-25/powershell-command-lookup/README.md).

## Hosted follow-up: dashboard and Darwin cleanup pass; platform gates remain (2026-10-25)

At `553e5dc1`, the PR Windows library passes 768/0 with 7 original ignores;
the complete dashboard and dashboard-management groups pass 13/0 and 24/0.
All four new contention regressions and the original pending-provider HTTP
case pass. macOS passes those four tests, dashboard 13/0, twelve cleanup
regressions, runtime 14/0, process 5/0 and ingress 20/0. Both Ubuntu workflow
runs pass all shards, coverage, actual rust-analyzer and release build.

Windows still fails two original Python PowerShell tests and the managed-folder
process case at 20 seconds; the push REPL case also fails. The isolated
LocalAppData control does not repair the failing hosts. A new separate lookup
and fixed-module-import control records scalar phase times, retaining the
original commands, limits, assertions and production environment.

All 43 macOS PDF failures now individually identify `SetAddressSpace` errno 22
before input. Twenty-five prompt-family cases include explicit spawn/direct
limit refusals and separately scoped generic/downstream symptoms. The remaining
library history fixture observes its first 40-ms sleep returning at 155.738 ms,
after the original 65-ms deadline; forcing a body read would be incorrect.
Its original gate and the strict memory contract remain unchanged and failing.

The new controls pass locally with checked natural tree exit. Typecheck, npm
260/0/3 original skips and Actionlint pass. Both full Python runs execute all
142 tests successfully, but an owned `vctip.exe` compiler helper survives
the original drain. Checked forced cleanup empties the Job; both controller
receipts remain incomplete. These are not complete platform or release passes.
See the [hosted follow-up evidence](validation/integration-2026-10-25/hosted-platform-followup/README.md).

## Document maintenance: two local Windows native groups pass (2026-10-25)

The host CLI/API now includes reviewed logical global-owner retirement and
separately approved physical purge of one committed document archive frame.
Retirement retains a restrictive fixed-name tombstone and the original generation;
purge retains receipts and distinguishes confirmed from unverified removal.
Windows namespace aliases refuse, and earlier cancellation receipts remain
inspectable during a later purge. Dashboard retirement blockers retain drafts,
disable writes/recovery and explicitly report incomplete recovery state.

At source `a27b5084`, the complete library passes 764/0 with 7 original ignores;
`session_dashboard_management` passes 24/0 with no ignores or name filters.
Two fresh producers and their executions prove root exit and empty owned Jobs
before forced cleanup. All 1049 selected inputs remain identical. Full
warnings-denied workspace/all-target Clippy, 368 native format roots, typecheck,
npm 260/0/3 original skips, Python 130/0/0 and Actionlint pass locally. These
execute permission separation, interruption/recovery, namespace, cancellation,
cross-maintenance and HTTP retirement fixtures; broader CLI/Session and actual
desktop/mobile qualification remain pending.

Ubuntu's exact-source hosted coverage, actual rust-analyzer and release build
also pass. Windows and macOS retain failing shards. A later dashboard mutex
contention correction and separate PowerShell LocalAppData trial pass source
checks, Python 136/0/0 and npm 260/0/3. The named dashboard tests now pass as
recorded above; complete platform acceptance remains pending.
Seven macOS composition cases now identify address-space limit installation
errno 22 before PDF input. No conforming replacement backend is established.
The isolated fresh release producer stops on the unchanged disk reserve;
its output is never adopted. No real user-data maintenance,
physical owner deletion/reactivation, all-host release proof or broader STATUS,
platform, bundled-runtime VSIX, main or release completion is claimed.
See [retirement](../native/docs/host-document-owner-retirement.md),
[purge](../native/docs/session-dashboard-history-purge.md) and the scoped
[execution evidence](validation/integration-2026-10-25/document-maintenance/README.md).

## Platform repair: five Windows native groups pass (2026-10-25)

At source `a9608c7f`, five complete native groups pass 760/0 with 7 unchanged
library ignores and no name filters: library 703, repository discovery 8,
GitHub App ingress 20, process 7 and agent REPL 22. Five fresh producers, five
test executions and warnings-denied workspace/all-target Clippy preserve 1044
identical selected inputs. Every phase proves root exit and an empty owned Job
before forced cleanup. Original profiles, deadlines, assertions and ignores stay.
Native formatting, typecheck, npm 260/0/3 original skips, Python 126/0/0 and
Actionlint 1.7.12 also pass locally.

The repair requires actual Unix child exit plus independent group absence,
propagates complete-output cleanup refusal, fixes protected-marker fixture
expectations and adds bounded timing/PowerShell failure observations. Actual
Unix behavior and the historical hosted PowerShell stall remain unqualified.
New hosted CI, the complete native suite, strict Darwin memory enforcement,
history/browser, bundled-runtime VSIX, full STATUS, main and release remain open.
This supersedes execution-pending notes only for these named local scopes.
[Evidence](validation/integration-2026-10-25/native-platform-repair/README.md).

## Scoped Windows native execution now passes (2026-10-24)

At source6b9a60dd, the complete default library group passes703/0 with7 unchanged
ignored tests; complete process and agent_repl groups pass7/0 and22/0. All three
groups run without a name filter. Four exact body-inlining regressions also
pass, each with24 other functions filtered; the full25-function body group is
not claimed. These cover the new namespace and standalone-assignment behavior.

Four fresh no-run producers and seven test executions all prove root exit and
their owned tree empty before forced cleanup, against1003 identical pinned
native/Cargo/workflow inputs. Profiles, assertions, ignores and test deadlines
are unchanged. The earlier d80 incomplete library producer remains incomplete;
a separately rebuilt library image now has a clean producer and actual runtime
result. The two prior PowerShell-related tests pass locally, but the hosted
cause and actual current CI/platform/live/history/VSIX acceptance remain open.
This supersedes local execution-pending notes only for these named scopes.
[Evidence](validation/integration-2026-10-24/native-windows-runtime/README.md).

## Native CI acceptance now waits only for its own platform (2026-10-24)

Each platform's acceptance job now depends on its own eight whole-target shards,
rather than all24 shards across Windows, Linux and macOS. All27 expanded check
names, original steps, assertions, timeouts, failure evidence, per-platform
coverage, actual rust-analyzer, release build and Windows history checks remain.
Plain YAML aliases share the unchanged step lists. Full acceptance still needs
all three platforms; no hosted pass or queue-speed improvement is claimed.
Actionlint1.7.12, parsed original-step equivalence,18 damaged variants and6144
dependency-readiness model cases pass locally. The full Python suite is113/0/0.
[Evidence](validation/integration-2026-10-24/platform-ci-dependencies/README.md).

## Local Windows native typing and Clippy: runtime acceptance open (2026-10-24)

At source d80c6c78, `cargo check --workspace --all-targets --locked` and full
workspace/all-target Clippy with warnings denied pass on Windows with Rust1.94.1
and the `x86_64-pc-windows-msvc` target.
All1003 frozen native/Cargo/workflow inputs remain exact. Each metadata run
reports370 fresh native artifact observations over368 distinct Cargo targets;
these are not test functions or executed groups. This supersedes earlier local
typing-pending notes for default-feature Windows code only. Unix typing,
doctests and native runtime behavior remain separate.

Byte-preserving per-file compression of1152 older own text logs/manifests
restores the existing1.2GB start admission. Every SHA-256, file identity and
mtime remains exact; no native image, cache or directory is compressed/deleted.
The independent500MB reserve remains. Reported compressed-file size reductions are not
exclusive physical-volume savings.

The library test-binary build reports Cargo0, but its owned group still has one
member before forced cleanup. The group is empty and root exit proved afterward;
the controlled build receipt remains incomplete. The candidate is not adopted
or executed, and no native tests run. Free space falls below1.2GB again after
linking. Current native/platform/history/browser/VSIX, full STATUS, main and
release acceptance remain open.
[Evidence](validation/integration-2026-10-24/native-windows-typing/README.md).

## Windows PowerShell phases: hosted cause remains unmeasured (2026-10-24)

Two existing native tests now retain bounded private phase markers across a
timeout, distinguishing observed Python-cell/script/folder progress without
depending on pipe EOF. Original20-second operation limits, assertions and shell
profiles remain. These changed native tests have not been compiled or run here.
Production process/runtime code is unchanged.

A separate failure-only diagnostic on Windows shards5/7 compares fixed owned
PowerShell commands under one/two Jobs and primary-thread/snapshot resumption.
It records actual resume/exit/script/EOF/cleanup observations, with no native
acceptance claim. Four local trials finish in about0.2s and leave empty
Jobs before forced cleanup. Ten new Python tests are included in the113/0/0
whole Python result; counts are nonadditive. Typecheck and npm260/0/3 existing
skips pass. These local observations do not explain the actual hosted20s failures.
Current native/platform/history/VSIX, full STATUS, main and release remain open.
[Evidence](validation/integration-2026-10-24/powershell-phases/README.md).

## TypeScript body assignment extension: native admission pending (2026-10-24)

Classic body expansion now handles a complete call on the right of a plain
identifier assignment that occupies one expression statement. Worker and Rust
independently derive the original `=` assignment, target and statement; original
target/operator/comment bytes remain copies. Parentheses and unbraced branches
retain ownership. No property/computed/destructuring target, compound/logical/
chained or consumed assignment is admitted by this extension.

The actual TypeScript5.8.3 worker passes44 direct cases:20 valid variants accept
and24 sequencing-sensitive variants refuse. All44 refuse at parent e125c304.
Accepted projections retain target bindings/types, exact bytes/hashes, argument/
write order, exceptions, finally completion and owned runtime observations.
Three native test functions and the original AST-field/capacity test are added/
extended but not run locally below the1.2GB native build admission. No current
Rust/application/platform acceptance is claimed. Full R1/STATUS, native CI,
history/VSIX live acceptance, main and release remain open.
[Evidence](validation/integration-2026-10-24/inline-assignment-heads/README.md).

## TypeScript body namespace correction: native admission pending (2026-10-24)

Classic body expansion now resolves free references with the pinned compiler's
actual type, value or namespace meaning. An unrelated same-name binding in the
other namespace no longer causes a false refusal; actual shadows still refuse,
including value references inside `typeof` and shorthand properties. Missing or
unknown classification fails closed. Source recipes, Rust admission and apply
permissions are unchanged.

The actual TypeScript5.8.3 worker diagnostic covers22 cases:12 valid variants
accept and10 adversarial variants refuse. At parent c1366e8d, the same22 cases
all refuse, including12 false refusals. Accepted projections retain copied
binding targets, result types, exact UTF-8/CRLF edits and owned runtime results.
Two new native test functions are added but have not run locally: available
space remains below the1.2GB native build admission. Typecheck and npm260/0/3
existing skips pass. Direct worker success is not independent Rust or platform
acceptance. R1, current native CI, live history/VSIX, main and release remain open.
[Evidence](validation/integration-2026-10-24/typescript-namespace-bindings/README.md).

## Readiness diagnostics and scoped hosted observations (2026-10-24)

Windows editor failures at c11511ee occur at the original3-second startup wait,
before request behavior. The fixture now records atomic startup stage, elapsed
and worker/channel state on failure. Existing macOS history/drip failures gain
only timing/count diagnostics. Original assertions, sleeps and budgets remain;
these modified native tests await execution. No production behavior changes.
Changed-file rustfmt, Python103/0/0, typecheck and npm260/0/3 existing skips pass.

The c11511ee Linux owned-C diagnostic now compiles and verifies256-MiB direct/
inherited limits with512-MiB allocation denial and group cleanup. This is not
native product or macOS qualification. Ten requested obsolete queued-run
cancellations are confirmed terminal, not counted as passing checks. Current
native/platform/live/VSIX and full STATUS remain open.
[Evidence](validation/integration-2026-10-24/readiness-phase-diagnostics/README.md).

## Publication SQLite deadline callback: native confirmation pending (2026-10-24)

The scoped source/publication policy now reinstalls the owned SQLite busy
callback before direct SQL and inherits the Store's earlier deadline. A caller's
built-in busy timeout had replaced that callback; changing its saved deadline
data alone could not reactivate it. Numeric timeout/length/deadline policy is
restored on success or refusal. An actual private SQLite C-API probe observes
zero callback calls and about0.948s without reinstallation versus22 calls and
about0.061s with it. This is Python's SQLite3.50.4 API evidence, not native Rust
or macOS acceptance. Three new native tests await execution; the original60-ms
contention test is unchanged. Full native/platform/live/VSIX/STATUS remains open.
368 format roots, Python103/0/0, typecheck and npm260/0/3 existing skips pass
separately; none executes the changed native code.
[Evidence](validation/integration-2026-10-24/sqlite-publication-deadline/README.md).

## Dashboard timeout result transport: native confirmation pending (2026-10-24)

Management now prepares two bounded refusal representations before queueing,
preserving the atomic distinction between never-started and unconfirmed work
when the waiting thread resumes after expiry. Normal results still require the
original deadline; prepared refusals use only the existing nonblocking408 write
attempt. Expiry during preparation queues nothing and returns a fixed null-ID
refusal. Two new native tests and extended existing queued/running tests await
execution. The original100-ms HTTP test, permissions and operation limits remain.
This closes a source-level result-loss path; actual macOS/native/HTTP/browser/
current VSIX/full STATUS confirmation remains open.368 format roots,
Python103/0/0, typecheck and npm260/0/3 existing skips pass with their separate scopes.
[Evidence](validation/integration-2026-10-24/dashboard-timeout-delivery/README.md).

## HTTP backpressure fixture: native confirmation pending (2026-10-24)

The library socket test now sends a4-MiB response after actual WouldBlock, so a
tiny write cannot fit into space released by in-flight bytes and invalidate its
pressure assumption. An actual local Python socket probe demonstrates a40-byte
write succeeding after WouldBlock while the4-MiB write reaches the deadline.
The250-ms response budget, existing assertions/buffer/fill/wait bounds and
production transport remain unchanged. Rustfmt, Python103/0/0, typecheck and
npm260/0/3 existing skips pass separately; the modified native test has not run.
Current hosted/native,
remaining timing/runtime/memory, live/VSIX and full STATUS gates remain open.
[Evidence](validation/integration-2026-10-24/editor-backpressure/README.md).

## History worker tree exit: stricter admission, native acceptance pending (2026-10-24)

An actual inert Windows worker counterexample shows the earlier runner accepting
exit0 while a child survives until forced Job cleanup. Success now independently
requires an observed empty assigned Job before that cleanup, with at most one
second of observation inside the original180-second deadline. Actual clean and
leaking Python trees prove positive admission and refusal; all owned Jobs end
empty.18 focused history tests and103 whole Python tests pass separately, with
overlapping counts. Typecheck and npm260/0/3 existing skips pass. Native
source/workflow remain unchanged;
archive/live/native/current VSIX/full STATUS acceptance remains open.
[Evidence](validation/integration-2026-10-24/history-tree-exit/README.md).

## Unix memory API diagnostic: observations pending (2026-10-24)

A fixed owned C fixture now measures Unix CI memory API behavior separately
from native acceptance. It retains the256-MiB hard/soft ceiling, attempts large
untouched reservations only after exact limit readback, and preserves API/shell
refusals. Child-only macOS allocator variants measure possible startup effects;
they change no product or host settings. JSON/logs are uploaded separately;
neither a header nor diagnostic completion certifies native enforcement.
101 Python tests pass, including14 diagnostic cases (nonadditive), with warnings
treated as errors. Typecheck and npm260/0/3 existing skips pass; native Rust
sources are unchanged. Actual
Unix fixture compilation/observations, current native matrix and complete
STATUS acceptance remain open.
[Evidence](validation/integration-2026-10-24/unix-memory-diagnostic/README.md).

## Status-hook binary stdin correction: native confirmation pending (2026-10-24)

At4ea583e1 the Windows shell/source-boundary test now passes; the status callback
instead refuses after2.064s with complete pipes and no timeout. Eight verified
Windows shard artifacts retain this evidence and separate REPL/known-folder
failures. The source4ea whole library passes694/0/7 ignored, not current native.

The status bootstrap now inherits binary stdin to avoid a managed writer's BOM.
Five actual inert PowerShell5 cases verify exact0/164/65536-byte transport,
Unicode arguments, stderr, reproduced old preamble insertion and clean owned-Job
exit. Current native tests add the generated command under that managed encoding
and explicit rejection of caller BOMs; native execution remains pending.87 Python
tests,368-root format, typecheck/npm260/0/3 existing skips pass. Shell policy,
permissions, deadlines, JSON rules and OS settings remain. Full acceptance is
open. [Evidence](validation/integration-2026-10-24/status-stdin-handles/README.md).

## Unix cleanup-test trait correction: hosted confirmation pending (2026-10-24)

Actual2154c13d Ubuntu Clippy rejects the new Unix fault wrapper with E0277:
process-wrap's generated trait requires Debug. The correction adds only that
test wrapper's derive; production, warning gates, assertions and budgets stay.
Changed-file rustfmt passes; actual Unix compilation/test confirmation remains
pending. Prior format/Python/TypeScript passes never certified native typing.
[Evidence](validation/integration-2026-10-24/unix-cleanup-test-trait/README.md).

## Volume-aware Unicode fixture assertions: hosted confirmation pending (2026-10-24)

Two native integration-test files now observe exclusive directory creation and
actual entry names before asserting distinct Unicode roots. Distinct volumes
retain the original positive symbol/provider assertions; aliasing volumes must
refuse the uncaptured spelling without provider execution or fixture overwrite.
Discovery expects only real entries, and uppercase Git metadata follows actual
volume lookup behavior. No production, workflow, profile or deadline changes.
An independent Windows filesystem probe and368-root format checks pass; the new
native branches have not been compiled or run locally. A successful alias case
does not certify support for two distinct roots on that volume. Native/platform/
live/VSIX and full STATUS acceptance remain open.
[Evidence](validation/integration-2026-10-24/volume-alias-fixtures/README.md).

## Bounded runtime cleanup and Unicode fixture correction: acceptance pending (2026-10-24)

The native runtime owner now consumes its process through the existing bounded
checked lifecycle instead of an unbounded raw wait. Actual cleanup errors and
missing receipts remain explicit, including after operation/deadline refusal.
Unix ESRCH still requires a real owned-root exit status; other signal failures
refuse. Five native regressions are added (three Windows/five Unix), not yet run.

The source-exclusion test now keeps NFC/NFD names in separate subdirectories so
APFS cannot overwrite its first fixture. A new create-only alias test asserts
actual on-disk names, immutable bytes and literal exclusion on either filesystem
behavior. Native compilation/execution remains pending.87 Python tests,368-root
format plus final changed-file checks, typecheck/npm260/0/3 existing skips pass.
Memory-limit, other filesystem/timing/platform/live/VSIX and full STATUS gates
remain open. [Evidence](validation/integration-2026-10-24/runtime-cleanup-unicode/README.md).

## Windows CI temporary spelling: partial correction, confirmation pending (2026-10-24)

At source84e11866 all eight Linux shards pass in push run40101366598;
Windows1/2/7 and seven macOS shards fail. Ten failed-shard log artifacts are
independently checked by API size/digest, inventory and log hashes. The Windows
fixture-path refusal motivates a CI-only canonical spelling of the same existing
temporary directory, with repeated identity observations and link/path refusal.
GetTempPath2W matches the Rust runtime; only subsequent job TEMP/TMP changes.
No shell policy, system setting, privilege or test deadline changes.

13 focused and87 whole Python tests pass separately; typecheck/npm260/0/3
existing skips and changed-file rustfmt pass. No current native build or run is
attempted locally. Windows shell/provider deadlines and subprocess refusal,
macOS memory/cleanup/filesystem/timing failures and full acceptance remain open.
[Evidence and limits](validation/integration-2026-10-24/windows-ci-temp/README.md).

## Whole-target native CI scheduling: acceptance pending (2026-10-24)

The earlier Windows job is terminal/cancelled after roughly45 minutes, still in
the suite. Its log preserves one shell-REPL deadline failure and two source-hook
failures. These remain open. CI now distributes all369 default Cargo groups
(365 test executables, three compile-only examples, one library doctest invocation)
over eight shards per platform. Original assertions/operation deadlines, profiles,
Windows1/Unix2 test threads, warning/permission gates and45-minute job limits stay.
Every same-source shard and whole-group/log receipt is required before the
original platform release/live steps. Missing evidence refuses completeness.

11 scheduler and16 separate history-admission tests pass locally without native
Yoke execution; source-hook changes add failure-only diagnostics. Format/typecheck/
npm260/3 existing skips pass. Hosted partitioned execution and complete STATUS
remain open; preceding504963a6 Node/Clippy successes keep their own source scope.
[Evidence and limits](validation/integration-2026-10-24/native-target-shards/README.md).

## History worker containment admission: hosted confirmation pending (2026-10-24)

The CI worker now verifies membership in its parent's specific fresh named
Windows Job before reading the native image. A non-inheritable query handle
closes before proceeding; missing/wrong Jobs and query/close failures refuse.
The normal suspended assignment, kill-on-close, deadlines, capacities and500MB
floor remain unchanged.16 admission/evidence tests pass on Windows, including
one actual empty-Job refusal and a simulated preceding-source regression.
No native Yoke image is read or executed. Typecheck/npm260/3 existing skips pass;
native Rust/workflow bytes remain exact. The hosted native gate and full STATUS
remain open. [Evidence](validation/integration-2026-10-24/history-worker-job/README.md).

## Unix CI test-helper correction: hosted confirmation pending (2026-10-24)

Rust CI for head `d90a4e35` stops on both Ubuntu and macOS because the document
test deadline helper is compiled without its callers' Windows condition. The
correction adds that condition to the helper only. No test, runtime behavior,
warning policy or workflow limit changes. Local format/typecheck and npm260/3
existing skips pass; native checks remain resource-gated. Hosted confirmation,
Windows history live acceptance and full STATUS remain open.
[Scoped evidence](validation/integration-2026-10-24/unix-document-test-helper/README.md).

## Host-history CI acceptance gate: configured, result pending (2026-10-24)

Product `9c32ef68afef7adaf21586ff586b11f0001748e0` adds an owned live acceptance
driver after the successful Windows release build and the existing release
artifact upload. All three changed Git blobs are independently verified. The
original Rust workflow prefix, tests, permissions, job timeout and release
upload remain exact. A separate artifact retains bounded test evidence; native
Session diagnostics stay in the private fixture outside that artifact.

The driver exercises native/Serena/persistent-Python stdio Sessions and real
private HTTP management. It checks both retired and current host authority while
archival is pending, exact two-package evidence/readback, stale-event refusal,
foreign occupants, a fresh save in the same destination host and isolated own
shutdown. A suspended worker is assigned to its kill-on-close Job before resume;
deadlines, protocol/output/source limits and the existing500MB stop remain.
Actual checkout revision, selected source hashes, executable hash, process
receipts and Job cleanup are recorded. This does not claim a complete build-input
inventory or native browser acceptance.

**11 pure Python admission tests pass locally**, without opening or running a
native executable. All nine preceding archive product inputs remain exact.
Native694 and TypeScript260/3 keep their preceding source scopes; neither is
rerun or reassigned here. The real CI driver has not yet been observed passing.
Archive integration/live/browser/release and complete STATUS acceptance remain
open. Local native work remains stopped below the disk reserve.
[CI gate evidence](validation/integration-2026-10-24/dashboard-history-ci/README.md).

## Global document history archive: implemented, acceptance pending (2026-10-24)

Development product `5ccfdffe70d17e3c1623aca86c1c605df8342be5` is pushed; all nine changed Git blobs are
independently verified. Explicit host history-plan/apply/inspect/read/recover
now select closed journals of a migrated global file. Both complete selections,
independent host authorities, current destination generation and private archive
root remain required. Immutable exact evidence precedes recorded removal intent
and identity/security/hash-checked original removal. Whole-journal packages use
an independent32–512-record budget and explicitly disclose the remainder. Before
removal, reviewed cancellation retains originals; after intent, recovery resumes
only. The pending marker blocks ordinary writes/recovery and new migrations.
Completion requires its retained marker; missing/foreign evidence stays uncertain.

Current local scope: **694 whole-library passed /0 failed /7 existing ignored /
0 filtered**, including16 new archive functions. All-target Clippy-Dwarnings,
368 format roots, typecheck and npm260/3 existing skips pass on1154 exact inputs.
The following integration build stops at the unchanged500MB disk supervisor.
Cargo emitted an own library and management-test executable, but that test was
not executed and no current CLI build/live acceptance is claimed. The overall
qualification is incomplete. Its transient281993216-byte free-space receipt and
later cleanup observations remain recorded: the polling guard is not an OS disk
reservation. No protected/failed output is adopted, and no guard/profile/test/
privilege setting or source byte is relaxed to continue below the limit.

The exact current client renders the archive blocker at1280 and390 pixels with
the draft retained/read only, writes disabled and no retirement action. These
are synthetic backend fixtures, not native browser/CLI acceptance or screenshots.
Initial Rust CI for this product is running on Windows/Linux/macOS; no successful
CI or release result is claimed. Local native integration/CLI/transport/browser,
current release/VSIX and complete archive acceptance remain open. The preceding
810-pass generation scope retains its own source; its total is not reassigned.
[Contract](../native/docs/session-dashboard-history-archive.md) and
[partial evidence](validation/integration-2026-10-24/dashboard-history-archive/README.md).

This supersedes only the earlier statement that archive code did not exist.
Physical owner deletion remains unimplemented. Committed-frame archive purge
now has the later source implementation described above; qualification is pending.
Broader STATUS, original353/73/77, ten OS1314 prerequisites, main and release
acceptance stay unchanged; the overall task remains active.

## Earlier global document proposal generations: scoped acceptance (2026-10-24)

Product `5c4dee9a4e6f23366be4c474fb9844e7be654d5d` is pushed and all three product blobs independently
verified on GitHub. An actual old-CLI counterexample showed that an ordinary
cached proposal could write after migration out and back, even with unchanged
configuration bytes and Session state. The earlier migration acceptance did not
cover that ordinary-proposal path; its broader old-review statement is superseded.

New global proposals bind optional `ownerGeneration`, checked during approval
and again under the original journal lock before publication. A return to the
same selection cannot revive either a generationless or generation-bearing
ordinary proposal. New generation-bearing recovery journals also bind that
epoch. Explicit event-reviewed recovery of existing generationless journals
remains supported; that compatibility path cannot approve an ordinary proposal.
Absent fields preserve original serialized bytes/IDs. Project plans stay unchanged.

Final scope: **810 native passed /0 failed /7 existing ignored /0 filtered**,
thirteen unique whole groups: library678, management22, dashboard13, notes15,
HTTP18, protocol5, steering12, selection8, settings8, project6, retention17,
capture-changes1, main7. One byte-identical same-source library continuation is
counted once after a downstream disk stop; no different-source group is reused.
All1153 inputs remain exact. All-target Clippy-Dwarnings,368 format roots,
typecheck and npm260/3 existing skips pass. Qualified current CLI SHA-256:
`457abc343041e3cc79c173a4e0bcaefcd62e67a1b63956abb41f9b3f66a86a51`.

The prior locally qualified debug CLI built from published source reproduces
the stale write with HTTP200; it is not a published release binary. The same
actual API worker with the corrected CLI runs native, Serena and persistent
Python Sessions: both stale proposal forms return HTTP422, preserving exact
file/owner/journal bytes; explicit new reviews save. Python45 stays live and
owned process exit precedes Job cleanup. Real browser UI handlers also retain
and lock the old draft, require explicit reload, save a newly reviewed proposal
with the current generation in its actual journal, and end the owning Session
before stdin EOF. The first browser run completes the file proofs but exceeds
its300-second deadline before shutdown; it is incomplete, preserved, and excluded
from the complete74.38-second repeat. All Jobs finish empty and the owned tab closes.

Two native snapshots time out. Inspected1280/390 images are static captured-
native-DOM rerenders, not native screenshots. Initial keyboard/click automation
sequencing failures remain preserved; this scope claims UI handler/state checks,
not another keyboard or accessibility certificate. Lossless resource recovery
keeps the500MB stop,1.2GB admission, profiles and original tests unchanged.
Protected hardlinked outputs are skipped; no deletion or system-setting change.
[Evidence](validation/integration-2026-10-24/dashboard-document-generations/README.md).

At that generation checkpoint, history archival had only a WIP contract and
bounded journal validators. The implementation above now supersedes that code
status, while complete archive acceptance remains pending. Standalone
owner deletion, remaining STATUS/Windows/Unix/upstream/Google/current VSIX,
main and release qualification remain open. Earlier353/73/77 and ten OS1314
prerequisites are unchanged. The overall task remains active.

## Dashboard keyboard access: scoped acceptance (2026-10-24)

Product `480fb00d3ba2866cb0b0a63fb8f3e7f2361d8737` is pushed and its exact HTML blob independently
verified on GitHub. A first-focusable skip link reaches the main content;
explicit page navigation focuses its heading, while automatic refresh preserves
input focus. Confirmation dialogs expose their explanation to assistive tools.
Real native Tab/Enter/Shift-Tab/Escape checks retain unsaved inert Unicode drafts,
restore focus on cancellation and confirm own shutdown before stdin EOF/Job
cleanup. The390×844 dialog fits its viewport. Synthetic and captured-native-DOM
pixel rerenders are inspected; native snapshot/click adapter timeouts and an
initial Tab attempt retaining BODY focus are preserved. No native screenshot or full
accessibility certificate is claimed.

Current HTML scope:13 dashboard tests/zero failures, all-target Clippy-Dwarnings,
368 format roots, typecheck and npm260/3 existing skips pass on1152 exact inputs.
CLI SHA-256 `03ef5e39ce72e21269bd447d9bf814ca2479f486221fe9bfc40674f267df7bcf`. The earlier805-pass
migration qualification retains its own source commit; it is not a new whole-
source total for this HTML change. Original Windows prerequisites, broader
dashboard/STATUS, main and release acceptance remain open. See
[keyboard evidence](validation/integration-2026-10-24/dashboard-keyboard/README.md).

## Earlier explicit global write-binding migration: scoped acceptance (2026-10-24)

Product `5bd0eadd88508b889520072ad53bc1b70ba58174` is pushed and all eight product blobs independently fetched
from GitHub. A host can now review and migrate the write binding of one exact
global document between two independently writable selections. Both complete
selection/root sets, current document and owner identity/security, exact bytes
and closed journal histories are checked. Metadata stays in the original held
owner file, so no absent/unbound interval admits another selection. A durable
pending marker blocks ordinary writes and recovery until explicit host recovery.
Fresh generations retire the preceding write selection. Ordinary cached
proposal revival after returning was not covered here and is corrected in the
current generation scope above. Never-migrated format1 bytes stay unchanged;
older builds refuse migrated format2. This does not edit configuration bytes,
reload profiles, migrate project files or grant rights from serialized data.

Final scope: **805 native passed, zero failures, seven existing ignored and zero
filtered**, thirteen unique whole groups. Library673, management22, dashboard13,
notes15, HTTP18, protocol5, steering12, selection8, settings8, project6,
retention17, capture-changes1, main7. One byte-identical same-source library group
continues after a downstream disk stop and is counted once; no different-source
group is reused. All 1152 inputs stay exact through qualification/live checks.
All-target Clippy with warnings denied, all368 format roots, typecheck and
npm260/3 existing skips pass. Qualified CLI SHA:
`5ffab66ed573302a674104a41c7478cf1cb3f00cb68e1395675847e0b968233c`.

Three actual stdio variants run CLI plan/apply/inspect/recover, reject a missing
host flag, preserve an occupied completion path and expose an explicit incomplete
browser blocker. Exact owned-fixture relocation permits reviewed recovery;
migration leaves configuration bytes exact, old writes refuse, and a fresh host
profile writes successfully under the new binding. An additional live case
retains the old Python value45 after the fresh host writes. Real HTTP actors both
observe the retired write authority without automatic profile reload; runtime99,
new Session123 and owning shutdown stay independent. Actual native browser checks
retain the draft through blocked and retired authority states, keep writes
disabled and prove old native/Python exit before stdin EOF/Job cleanup. All Jobs
finish empty; owned tab closes and the new synthetic credential is removed.
Inspected images are synthetic or captured-native-DOM rerenders; two native
snapshot timeouts are preserved, without a native screenshot claim.

Earlier CLI return-type and Clippy naming failures remain in the archive. The
unchanged500MB guard stopped a downstream build after the library passed.
Verified own evidence and positively qualified exact artifacts received
SHA-preserving lossless compression, without deletion or policy changes. A
resource-only historical main test initially used a missing scratch environment;
its corrected seven passes are excluded from805. Focused passes are also not
added. The continuation rechecks all1152 bytes and the passed library executable.
[Contract](../native/docs/session-dashboard-binding-migration.md) and
[scoped evidence](validation/integration-2026-10-24/dashboard-binding-migration/README.md).

This closes explicit per-target migration and retirement of its preceding write
selection. Standalone unbound-owner deletion, journal-history retirement/removal,
broader native recovery UI, tray/automatic opening and full dashboard/STATUS/
platform/upstream/release acceptance remain open. Earlier353/73/77 and ten
historical OS1314 prerequisites remain unchanged. No main merge or release.

## Earlier selected project-file journal recovery: scoped acceptance (2026-10-24)

Product `191c06b7c9f5baa3149f3f773a8278d082dfe22a` is pushed to the integration branch. All twelve product blobs
are independently verified on GitHub. The dashboard can now inspect finite pages
of native journals bound to its exact current writable project-file selection,
review immutable before/after bytes, and explicitly confirm conditional rollback.
The raw journal revision is rechecked under the original WriterLock. Current
Session/root/project/editing/read-only/query/tool/source-scope grants still apply;
metadata never grants permission. Unknown current bytes are preserved. Resume and
older unbound/native journals remain in the original host workflow. Empty pages
are never advertised as complete workspace recovery. Native host rollback and
emergency journal updates preserve optional provenance; default Receipt API and
unbound serialization remain unchanged. Provenance does not retain CAS evidence.

Final scope has **788 native passes, zero failures, seven existing ignores and
zero filtered functions** in thirteen whole groups: library657, management22,
dashboard13, notes15, HTTP18, protocol5, steering12, selection8, settings8,
project6, retention17, capture-changes1 and main6. No older-source group is reused.
All 1149 frozen inputs stay exact through qualification and live checks.
All-target Clippy with warnings denied, 368 format roots, typecheck and npm260/3
existing skips pass. Qualified CLI SHA:
`0ded3e33a52c750ce7b43ab54392ec234c72a1c7fe889f205ab970697f51f7f9`.

Three real stdio variants inspect original/native plan IDs and exact before/after
bytes; reject resume, stale reviews and foreign bytes; restore exact originals;
then refuse stale retries and expose terminal journals without recovery actions.
Real HTTP actors share durable journals, while proposals remain actor-local:
one actor's rollback invalidates the other's saved review. The native browser
reviews both byte versions, cancels without writing, confirms rollback, retains
the previous draft until explicit reload, and proves restored file bytes.
Owned native/Python exits, independent HTTP runtime state and empty Jobs retain
their original proofs. No existing credentials, system policy, model call or
analyzed project execution is introduced. Pixel evidence distinguishes synthetic
fixtures and captured-native-DOM rerenders from native screenshots.

The first paging assertion failure is retained: a valid two-entry page could
precede the authored corrupt row. The corrected test follows exact cursors and
counts that row once. Focused 7+11 passes are not added to the whole-group total.
The unavailable-format-tool attempt and initial synthetic-fixture JSON parsing
failure remain distinct from the passing final gates and inspected fixtures.
[Contract](../native/docs/session-dashboard-project-recovery.md) and [scoped evidence](validation/integration-2026-10-24/dashboard-project-recovery/README.md).

This supersedes dashboard recovery debt for these bound single-file journals.
Global write-binding migration/retirement, broader native recovery UI, tray/
automatic opening, accessibility/platform/upstream and full STATUS/release work
remain open. Original 353/73/77 boundaries and ten historical Windows OS1314
baseline failures are unchanged. No main merge, release or full certificate.

## Earlier selected project-file writes: scoped acceptance (2026-10-24)

Product `125a538cffc90c45807f30b232afd70752998a6d` is pushed to the integration
branch; all seven product blobs are independently verified on GitHub. Explicitly
writable selected project files now use native immutable workspace plans. Host
workspace writes, current editing/read-only/query policy and tool exclusions
remain checked. The browser prepares and inspects; it cannot approve a plan or
execute verification. Saving uses original exact approval, passed executed
verification for the result snapshot, current full base snapshot, WriterLock,
scope/expiry checks and workspace journals. Global-file authority is never used
for project writes. Effective line-ending after-bytes are published for review.

Final scope has **760 native passes, zero failures, seven existing ignores and
zero filtered functions** in eleven whole groups, including all 647 library and
22 management functions. All 1145 inputs stay exact through qualification and
live acceptance; no preceding-source group is reused. All-target Clippy with
warnings denied, all 368 format roots, typecheck and npm260/3 existing skips pass.
Qualified CLI SHA:
`d4fbfbcc3a6de7175e6d6528cf77c9ef26ad4726c158e76d5dc3579da53542fc`.

Three actual stdio variants refuse before host approval and executed verification,
then save exact reviewed bytes through native receipts. Real host-authored Python
checks verify inert configuration bytes in copied trees. HTTP proposals remain
actor-local while saved file bytes are shared. The actual native browser checks
disabled Save, real host proof status, Cancel, confirmation, exact persisted hash
and retention of a later unsaved project draft after owning shutdown. All owned
Jobs finish empty and the fresh synthetic credential is removed. Native exit and
owned runtime exit are proved before stdin EOF/Job cleanup. Inspected pixels are
synthetic or captured-native-DOM rerenders; native snapshot attempts timed out.

The initial 643-pass/4-fail/7-ignore library result and corrected fixtures remain
in the evidence. The focused eight-function pass (646 filtered) is not added to
760. A controller cwd issue was corrected before reaching its CLI stage; its
unused resume draft never ran. [Contract](../native/docs/session-dashboard-project-files.md)
and [scoped evidence](validation/integration-2026-10-24/dashboard-project-files/README.md).

This supersedes historical project-write debt below. Workspace recovery remains
the original native host workflow; the dashboard reports unavailable/incomplete
inspection rather than empty completion. Dashboard workspace recovery, global
write-binding migration/retirement, tray/automatic opening and broader STATUS/
accessibility/platform/upstream/release work remain open. Original 353/73/77
boundaries and ten historical Windows OS1314 baseline failures remain unchanged.
No main merge, release or full dashboard certificate is claimed.

## Earlier owning MCP Session shutdown: scoped acceptance (2026-10-24)

Product `3bcea306547f641f294f018b09077183d5f4e646` is pushed to the integration
branch; all eleven product blobs are independently verified on GitHub. A separate
host grant and trusted transport binding enable ending only the owning MCP
connection. Current Session authority still applies. The owner quiesces after
acceptance and waits for the reply-write attempt or abandoned delivery before
normal cleanup. An acknowledgement does not claim completed process exit.

Final scope has **752 native passes, zero failures, seven existing ignores and
zero filtered functions** in eleven whole groups, including all 639 library and
22 management functions. All 1143 inputs stay exact through qualification and
live acceptance; no preceding-source group is reused. All-target Clippy with
warnings denied, all 368 format roots, typecheck and npm260/3 existing skips pass.
Qualified CLI SHA:
`8d7f4166f8824a6bcd5025727eacbcbf856980a587581fef054ee1e90c3fcb6a`.

Three actual stdio variants exit before stdin EOF. HTTP closes only A; B retains
its runtime value 99 and the service accepts C. Held image-checked process handles
prove owned Python runtime exits before Job cleanup. Actual native browser
cancellation, rapid reopening, confirmation and retained copyable unsaved drafts
pass at 390-by-844 without horizontal overflow. Snapshot automation timed out;
inspected pixels are labeled synthetic fixtures or static captured-native-DOM
rerenders, never native screenshots. Original compile and visual failures remain
in the evidence. All owned Jobs finish empty and the fresh synthetic credential
is removed. No privilege change, existing credential or automatic execution of
analyzed project code is used.

[Contract](../native/docs/session-dashboard-shutdown.md) and
[scoped evidence](validation/integration-2026-10-24/dashboard-owned-shutdown/README.md).
This supersedes owning-shutdown debt in the historical checkpoints below.
Project writes through original approval/executed-verification gates, binding
migration/retirement, tray/automatic opening and broader accessibility/STATUS/
platform/upstream/release work remain open. The original 353/73/77 boundaries and
ten historical Windows OS1314 baseline failures remain unchanged. No main merge,
release or full dashboard certificate is claimed; the full action remains active.

## Private raw logs and activity labels: scoped acceptance (2026-10-24)

Product `c65c1512bfbefc7f5c408bf4a31b6a7cdd15a73e` is pushed to the integration
branch; all eleven product blobs are independently verified on GitHub. An
independent `management.rawLogs` selection now exposes explicit finite byte
windows from private host files outside every analyzed root. Physical ancestor
identities and pinned parents retain namespace boundaries. Separate log directory
read/write sharing permits rotation without weakening strict document guards.
Opened-generation Base64 bytes, decimal offsets and hashes remain authoritative;
no immutable snapshot or whole-file completeness is inferred. Document, language
and log activity names are fixed allowlisted labels with no arguments/results.

Final scoped acceptance has **747 native passes, zero failures, seven existing
ignores and zero filtered functions** across eleven whole groups, including all
634 library and 22 dashboard-management functions. All 1141 inputs stay exact
through qualification and live acceptance; no preceding-source group is reused.
All-target Clippy with warnings denied, all 368 formatting roots, typecheck and
npm: 260 passes and 3 existing skips pass. Qualified CLI SHA:
`8c65cefa4acf52b44dd0bbab7cc3cb5bd6297c69c3b88719b2d2a2db166ce59e`.

Actual CLI acceptance covers three stdio variants, Python state from 41 through 45 and
two independent HTTP Sessions with deliberately shared host-selected log bytes
and separate observation IDs. The actual native browser proves explicit reads,
no automatic reread after append, exact Base64, inert script text, disclosed
lossy UTF-8 and preserved stale observations after a competing project change.
The actual 390-by-844 viewport has no horizontal overflow. Inspected pixels are synthetic
fixtures or static captured-native-DOM rerenders, not native screenshots: snapshot
automation twice timed out. All owned Jobs finish empty; the fresh synthetic
service credential is removed after exact verification. No privilege change,
manual deletion, model call or automatic analyzed-project execution is added.

The original rotation failure (633 passes, 1 failure and 7 ignores), its finite Windows OS
comparison and the fixture-only 400-response parser failure remain in the evidence. The
corrected actual-source isolated 105-function harness is separately labeled and
is not added to the final 747 native passes. [Contract](../native/docs/session-dashboard-raw-logs.md)
and [scoped evidence](validation/integration-2026-10-24/dashboard-raw-logs/README.md).
Project configuration writes through original approval/executed-verification,
write-binding migration/retirement, owning-MCP shutdown, tray/automatic opening
and full dashboard/STATUS/platform/upstream/release acceptance remain open.
The original 353/73/77 boundaries and ten historical Windows OS1314 baseline failures
remain unchanged; no main merge or release is claimed.

## Selected global documents and language selection: scoped acceptance (2026-10-24)

Product `5e094c6dded5c0ab2965f5bfc74d42ff25ad886d` is pushed to the integration
branch; all twelve product blobs are independently verified on GitHub. Explicit
host selections now read global/active-project UTF-8 files and edit independently
writable global files through reviewed proposals, physical/security revisions,
atomic journals and conditional resume/rollback. Original write authority is
permanently bound beside each written global target; selection/journal changes
cannot silently bypass it. Project writes and automatic migration remain unavailable.
The selected-document backend is Windows only. Running configuration is not reloaded.

Final scoped acceptance on 1138 unchanged inputs has **733 native passes, zero
failures, seven existing ignores and zero filtered functions** in eleven whole
groups, including all 623 library and 19 dashboard-management functions. Nine
integration groups (104 passes) are reused exactly once after a controller cwd
mistake prevented the subsequent library command from starting. The corrected
qualifier rechecks their exact inputs/artifact hashes and runs library, CLI tests
and build. All-target Clippy with warnings denied, all 368 format roots, typecheck
and npm260/3 existing skips pass. Current CLI SHA is
`b18eff3d4e2f088656ae636190ff22309a657a99212514a8898ab4c49f662579`.

Actual CLI acceptance covers three stdio variants, Python state 41 through 44,
two independent HTTP Sessions, file CAS and actor-local proposals, deliberately
shared global file visibility, project read-only gates and provider selection/
disable/reset. The actual browser checks file review/confirmation/save with
BOM/CRLF, stale drafts and provider order/disable/reset. Actual mobile DOM bounds
are checked; inspected pixels are labeled synthetic fixtures or static rerenders
of captured native DOM, because snapshot automation timed out. They are not native
browser screenshots. All owned Jobs finish empty and the fresh synthetic
credential is removed. No system/privilege change or manual deletion is introduced.

[File contract](../native/docs/session-dashboard-documents.md),
[selection contract](../native/docs/session-dashboard-language-servers.md) and
[scoped evidence](validation/integration-2026-10-24/dashboard-documents/README.md).
The earlier language CLI disk stop below is historical and superseded for this
scope. The full dashboard/STATUS action remains active: project writes through
original approval/verification gates, binding migration/retirement, meaningful
file-operation activity labels, explicit private raw logs, owned MCP shutdown,
tray/automatic opening and broader accessibility/STATUS/R1-R9 remain open.
Original353/73/77-runtime, ten Windows symlink prerequisites and full platform/
upstream/release certificates stay unchanged. Main and release remain open.

## Historical dashboard language partial checkpoint (2026-10-24)

Product `4d353f0a9c0e31da3e0888b7990f84be8d06dce3` adds explicit host-granted
language-server selection to the private dashboard. All eight product blobs are
independently remote-verified. Ordered backend choices, a legacy-language subset
(including explicit disable), and reset apply to the original Session and active
project. Dormant registration alone does not grant selection. Host registry
bindings, current policy and session SHA remain authoritative; persisted project
overrides cannot leak a filtered primary registry into other projects.

The final implementation passes all 600 library functions, with seven existing
ignores, all-target Clippy with warnings denied, all 368 format roots, typecheck
and npm260/3 existing skips on 1134 unchanged inputs. The 13-function dashboard
group passed before the subsequent capability/activity/UI refinements; its five
new functions cover actual fixture-provider dispatch, order, disable, persistence,
separate grants, stale requests and registry mutation. It is not a final-source
13-function pass. Synthetic desktop/mobile reorder and stale-project render
fixtures were inspected separately from native transport acceptance.

The subsequent CLI build stopped at the unchanged 500MB disk guard. The expanded
whole matrix, new CLI, native browser and three stdio/two-session HTTP acceptance
are pending. Prepared live controllers have not run. No new executable is
qualified, and the old partial linker output is not substituted or optimized.
The full dashboard/STATUS action stays active; configuration documents with
durable recovery, private raw logs and owned shutdown remain unimplemented.

[Selection contract](../native/docs/session-dashboard-language-servers.md) and
[scoped evidence](validation/integration-2026-10-24/dashboard-languages/README.md).
Existing inventories, runtime unknowns and platform/release certificates remain
unchanged. The following checkpoint describes the preceding management scope.

## Native dashboard management checkpoint (2026-10-24)

Product commit `552cd8d8a1075d4f360eb1517e1f72a5683db40e` is pushed to the
integration branch; all twelve product blobs were independently remote-verified.
A second host opt-in now enables project/mode controls and current-project
named-memory CRUD with revision CAS on the same owning Session thread. Default
absent/null management retains preceding bytes and authority. The native CLI
supports Session/Serena stdio, including a persistent trusted runtime, and
Session-backed MCP-HTTP. A library host must install and poll its driver.

The final scoped matrix has 677 native passes, zero failures, seven existing
ignored workers and zero filtered functions in eight unique whole groups, on
1133 unchanged inputs. All-target Clippy with warnings denied, the complete
368-root format helper, native build, typecheck and npm260 passes/3 existing
skips pass. Two red/green regressions fix Windows split-request aborts and accepted
requests outliving a shorter capability. Actual CLI runs cover three stdio
variants and two independent HTTP sessions. The actual native browser covers
CRUD, a competing-client conflict with retained draft, project/mode changes and
390px layout bounds. Both source files remain exact. All owned jobs finish and
all newly created synthetic credentials are removed; no privilege setting changes.

The exact HTML was separately rendered and visually inspected on desktop/mobile
and conflict states. These synthetic pixel fixtures are distinguished from the
actual native browser workflow because collaborative screenshot capture timed out.
This is not a current whole-native/Windows/non-Windows/upstream certificate and
does not upgrade the ten original Windows symlink prerequisites or old inventories.

The full dashboard action remains active. Registered-language selection,
host-selected global/project configuration files with durable conditional
recovery, explicit private raw logs and owned MCP shutdown remain open, as do
tray/automatic opening and broader accessibility/STATUS/R1-R9 work. The older
read-only checkpoint below remains historical/default-mode evidence. See
[evidence](validation/integration-2026-10-24/dashboard-management/README.md) and
[contract](../native/docs/session-dashboard.md). Main and release stay open.

## Compiler move import attributes checkpoint

Immutable compiler moves now retain literal import attributes from captured
source. Source-only repairs restore compiler omissions and shorthand-only import
dependencies. Native syntax and bound-target checks reject changed, dropped,
invented or ambiguous attribute requests before plan publication. Original source
bytes, host gates and guarded application/recovery remain authoritative.

Qualified code 2240fadd passes 651 Rust functions in five whole groups,
with zero failures, seven existing ignores and no filtered tests. Seven added
functions are included. Clippy, 367-root formatting, locked build, typecheck and
npm 260/3 existing skips pass on 1129 frozen inputs. Three integration groups
reuse identical product/test inputs after a correction to one new cfg(test)
fixture; all 594 library and six CLI functions rerun. Actual old/new legacy move
effects, admission of all saved plan IDs and a current CLI reproduction with
independent copied-result tsc pass. Fresh plan IDs retain their existing clock
dependence; raw IDs are not claimed equal. The initial disk stop and corrected
new-fixture assertions remain archived. No original test, ignore, Cargo profile
or system setting changes. All owned Jobs end empty.
See [attribute evidence](validation/integration-2026-10-24/compiler-move-attributes/README.md)
and the [native contract](../native/docs/compiler-move.md#literal-import-attributes).
Broader attribute/consumer/loader semantics, R1-R9, the original 353 inventory,
77 backend runtime qualifications and remaining full acceptance stay open.

Explicit live prompts now support embed_memory(name) over a host-selected set
of existing authored application notes. Active-project/global namespace, mode,
tool/API and ignored-memory policies precede payload access. Existing-only
read-only stores, bounded SQLite reads, immutable revisions and before-delivery
state/head rechecks preserve authority and provenance. The separate protocol-4
function receives only resolved inert data. Missing/deleted selected notes have
explicit partial warnings; policy/integrity/selection failures refuse. Default
bindings and protocols 1/2/3 remain byte-identical. No source capture by the helper,
default model call, automatic memory lookup or analyzed-source execution is added.

Qualified code caeecb8e passes 969 Rust functions in 53 unique whole groups,
zero failures, 10 existing ignores and no filtered functions. All 592 library,
six binary and 13 added functions are included. All-target Clippy, 366-root format,
locked build, Python 47, typecheck and npm 260/3 existing skips pass on 1126 exact
frozen inputs. Only four integration groups and Clippy reuse identical green3
inputs, counted once. Sixteen actual old/new CLI parents preserve literal/eager
and preceding live-protocol-3 raw MCP bindings, original nonempty SQL and sources.
Eight actual HTTP requests and two persistent Python cells exercise real note
embedding, inert note text, mode/catalog changes and clean closure. One new
synthetic credential is removed; no existing credentials or system settings are
accessed/changed. All reviewed owned Jobs end empty. The pre-test disk stop and
two corrected new-fixture assertions remain archived; no original tests/ignores,
internal deadlines or Cargo profiles change. Only exact passed artifacts and
verified evidence copies receive file-only lossless compression; no deletion.
See [authored-memory evidence](validation/integration-2026-10-23/prompt-memories/README.md)
and [native contract](../native/docs/prompt-memories.md).
Default/upstream catalogs, automatic note selection, additional Python-Jinja
calling/conformance, clients/layouts/extensions/quality, original 353/R1-R9/77
runtime/current whole Windows/Unix/hosted/canonical VSIX/Google/main/release
acceptance remain open. Earlier entries retain their historical source scopes.

Explicit live session prompts now derive available_tools, available_markers and
tool_names from the same selected catalogs published by each actual transport.
Separate --allow-session-templates host authority enables bounded protocol-3
rendering for MCP initialization and initialInstructions, including mode/project
changes. Deferred YAML export/discovery retains original template scalars and
still requires independent startup authority. Literal/default/eager paths retain
their previous bytes. No upstream prompt/application code, default model call,
filesystem loader or analyzed-source execution is added.

Qualified code dd0d2cf7 passes 939 Rust functions in 50 whole groups, zero
failures, 10 existing ignores and no filtered functions. All 590 library and
15 added functions are already included. All-target Clippy, complete 365-root
format, build, Python 47, typecheck and npm 260/3 existing skips pass on 1122 exact
frozen inputs. Whole-group/Clippy reuse requires identical input hashes and is
counted once. Twelve actual compatibility CLI parents preserve raw old/default/
eager/MCP replies, exported profiles and nonempty SQL/source rows. Eight actual
HTTP requests verify mode/catalog hashes, guarded catalogs, 401 state preservation
and clean session closure; two persistent Python cells verify the live Serena
profile and a mode change. Temporary synthetic credentials are removed; existing
credentials and system/privilege settings are untouched. All owned Jobs end empty.
The initial link/scratch and HTTP-fixture failures remain in the evidence.
See [live prompt evidence](validation/integration-2026-10-23/runtime-prompts/README.md)
and [native contract](../native/docs/session-prompt-runtime.md).
Default/upstream prompts, embed_memory, full Python-Jinja/client/layout/extension
quality, original 353/R1-R9/77-runtime/current whole Windows/Unix/hosted/canonical
VSIX/Google/main/release acceptance remain open. Earlier sections record their
historical source-specific scope; this checkpoint does not close broad parity.

Explicit host-selected YAML prompt catalogs now support original-byte pins,
flat prompt_templates discovery, named rendering and create-only UTF-8 output.
Collision policy defaults to refusal; explicit last_wins retains every source and
shadowed declaration with replacement links. Raw file identities, combined source
budgets and before-delivery guards remain enforced. No default lookup, derived
runtime variables, model/network call or analyzed-source execution is added.

Qualified code cb901cec passes 742 Rust functions in 20 whole groups, zero failures,
seven existing ignores and no filtered functions. All 586 library functions and
20 added functions are already included. All-target Clippy, complete 364-root
formatting, build, Python 47, typecheck and npm 260/3 existing skips pass on 1118
exact frozen inputs. Previously passed whole groups and Clippy are reused only
with those exact inputs, without double counting. Twelve actual CLI parents prove
old literal and inline raw replies, catalog-exported profiles, raw MCP/session
replies and original nonempty SQL/source rows remain exact. One additional actual
named-render call binds a 300169-byte protocol-2 request over 300106 resolved data
bytes, beyond the retained old inline limits. All owned Jobs end empty. The initial
disk stop happened before tests; five already qualified CLI copies and completed
distinct-target PDBs were losslessly compressed with exact hashes and no deletion.
See [prompt-catalog evidence](validation/integration-2026-10-23/prompt-catalogs/README.md)
and [native contract](../native/docs/prompt-catalogs.md).
Derived runtime/tool variables, default/upstream catalogs, broader Jinja/client/
layout/extension quality, original 353/R1-R9/77-runtime/current whole Windows/Unix/
hosted/canonical VSIX/Google/main/release acceptance remain open.

Explicit host-selected prompt templates now render context/mode prompt and
project/local initial_prompt scalars through a separate native worker. The new
templateContext and --allow-template-evaluation grant preserve default literal
behavior. Original YAML policies, hashes and permissions stay separate; no
filesystem loader, environment lookup, provider/model or project-code execution
is added. Fixed fuel/output/recursion/deadline and checked OS memory limits bind
the worker, including a self-limit on direct calls.

Qualified code e0c9ff6e passes 722 Rust functions in 19 whole groups, zero failures,
seven existing ignores and no filtered functions. All 584 library functions and
16 added functions are included. All-target Clippy, complete 363-root format,
build, Python 47, typecheck and npm 260/3 existing skips pass on 1115 exact frozen
inputs. Ten actual CLI parents prove old/default raw plans, literal/rendered
exported profiles, raw session/MCP replies and original nonempty SQL/source rows
remain exact; initial_instructions returns the rendered guidance. An additional
bounded 1GiB indent allocation produces actual allocator failure without stdout;
all owned Jobs end empty. Default capture/cleanup logic remains unchanged,
with production Windows Job CRLF normalization disclosed.
See [prompt-template evidence](validation/integration-2026-10-23/prompt-templates/README.md)
and [native contract](../native/docs/prompt-templates.md).
Broader catalogs/runtime variables/Jinja conformance, layouts/extensions/clients/
quality, original 353/R1-R9/77-runtime/current whole Windows/hosted/canonical VSIX/
Google/main/release acceptance remain open.

Direct complete-call `if`, `switch` and `throw` heads now support explicit
classic function-body inlining. Independent Rust original-AST field admission
and full source reconstruction retain one evaluation, original branch/case/label
and finally behavior. Loop conditions, compound/short-circuit/nested expressions and
overlapping frames still refuse. Captures, permissions, static observations,
verification, approval and recovery stay mandatory; result complete=false.

Qualified code e01920ad passes 630 Rust functions in five whole groups, zero
failures, seven existing ignores and no filtered functions, including all 581
library functions. Five added functions are already included. All-target Clippy,
complete 362-root formatting, build, Python 47, typecheck and npm 260/3 existing
skips pass on 1112 exact frozen inputs. Three actual new-context CLI scenarios
pass 27 owned process calls: unverified/unapproved refusal, independent tsc,
approved apply, equal owned-literal runtime observations, foreign-preserving
recovery and exact restoration. Original body test bytes are retained intact.
All owned Jobs end empty. Failing fixture/sequencing phases remain disclosed.
See [statement-head evidence](validation/integration-2026-10-23/inline-statement-heads/README.md)
and [body contract](../native/docs/compiler-inline.md#classic-function-body-statements).
Broader expression/invocation/predicate forms, original 353/R1-R9/77-runtime,
current whole Windows/client/quality/hosted/canonical VSIX/Google/main/release
acceptance remain open.

Explicit host-only session-config discover now reads selected YAML files and
flat custom context/mode directories, returning a ready hash-pinned selection
and the existing policy projection. Names/order, byte hashes, guarded inputs,
complete candidate admission, before-output membership/source rechecks and
held-through-delivery handles remain explicit. Existing plan/export pins,
schemas, permissions and session identities stay unchanged. No default lookup,
Store, source rewrite, template/command/provider/model or network call is added.

Qualified code 09898097 passes 125 Rust functions in 17 whole groups, zero
failures/ignores/filtered functions, including two new namespace/source freshness
unit tests and twelve new end-to-end discovery tests. All-target Clippy,
complete 362-root formatting, build, Python 47, typecheck and npm 260 with three
existing skips pass on 1,112 exact source/control inputs. The core library is
unchanged and is not rerun or added to these scoped totals.

Ten actual CLI parents prove deterministic discovery, identical old/new pinned
plans and exported profiles, unchanged raw MCP/session replies and complete
original nonempty snapshot/graph/record rows. Original CRLF/BOM/Unicode source
bytes remain exact. The unchanged actual Code 1.143.1 host test passes all four
assertions on the new explicit native runtime; all owned Jobs end empty. The
new write-lock fixture first records Rust's Uncategorized classification, then
requires actual Windows error 32 and unchanged data/recheck without loosening
any source guard. See [discovery evidence](validation/integration-2026-10-23/config-discovery/README.md)
and [configuration contract](../native/docs/session-config.md#explicit-directory-discovery-and-first-pinning).
Broader layouts/Jinja/extensions/client/quality, original 353/R1-R9/77-runtime,
Windows/hosted/canonical VSIX, Google/main/release acceptance remain open.

Windows owned-Job list refusals now report bounded numeric query/count/error,
after-list accounting and retained-root observations. A real terminate-only
Windows Job alias reproduces the missing-diagnostic regression. Original typed
refusal, list equality/capacity, termination requests, member synchronization
and deadlines remain; observations are explicitly non-atomic and confer no
completion proof or new permissions.

Qualified code dfa068c7 passes 697 Rust functions in 11 whole groups, zero failures
and 8 existing ignores, including all 580 library functions. All-target Clippy,
format checks for all 361 roots, build, Python 47, typecheck and npm 260 with 3 existing skips
pass on 1,109 unchanged frozen inputs. Actual seven-parent CLI comparisons retain
ordinary absent/null replies, original SQL/source and selected/staged behavior;
the current explicit native Code 1.143.1 host passes all four assertions. All
owned Jobs end empty. Existing Job test Git bytes remain an exact prefix;
production/test checkout line-ending normalization is disclosed. Only exact
passed artifact files receive byte/hash-preserving lossless optimization.
See [diagnostic evidence](validation/integration-2026-10-23/owned-member-diagnostics/README.md)
and [cleanup contract](../native/docs/windows-process-cleanup.md).

The historical sporadic list-refusal cause remains open; no natural recurrence
is observed in this qualification. A follow-up review corrects the initial
Windows capture concern: the existing group_cleaned block already propagates
checked cleanup failures before any successful completed-stream exit. No
Windows capture fix is required. Drop is only a best-effort fallback; this
Windows proof does not certify Unix behavior. See the exact
[source correction](validation/integration-2026-10-23/owned-member-diagnostics/capture-claim-correction.json).
Broader STATUS/353/R1-R9/77-runtime/Windows/hosted/canonical VSIX/quality/Google/
main/release acceptance remains open.

The current native runtime passes both unchanged explicit rust-analyzer live
gates on actual Windows rust-analyzer 1.94.1: definition/references/rename and
MCP references bound to the immutable historical snapshot with exact byte/hash
proofs. These are two selected originally ignored gates, zero failures and two
other live gates filtered out, separate from default/scoped suite totals.

At source 41d66b04/native code 666656ba, all 1,109 monitored source/control
inputs, the selected provider and the existing native CLI stay exact; owned
Jobs end empty. Tests use private home/temp and the existing passive profile
without authorizing source execution. No original test, ignore, deadline,
profile or system setting changes. See [actual backend evidence](validation/integration-2026-10-23/rust-analyzer-live/README.md).
This finite navigation check does not certify resource/vendor settings, all
77 runtimes, transitive provider effects, current whole-Windows/hosted,
canonical VSIX, broader STATUS/R1-R9, main or release acceptance.

The complete Windows native formatting gate now passes all 361 Cargo target
roots without an oversized process command. The helper uses offline locked
metadata, per-target editions and recursive local path dependencies, with ordinary
rustfmt module/configuration behavior and explicit nonzero failures. The original
aggregate command still reproduces OS206; Unix CI keeps the canonical Cargo gate.

Qualified code ced3f2d2 passes Python 47, typecheck and npm 260 with 3 existing
skips on 1,106 unchanged source/control inputs. Canonical verbose roots,
independent metadata and the helper's full list agree exactly. Actual fixture
checks preserve source and never execute the build-script canary. All owned
Jobs end empty. Rust production, original tests/profiles and the current native
executable stay unchanged; no native suite is rerun for this tooling-only change.
See [format evidence](validation/integration-2026-10-23/native-formatting/README.md)
and [contributor command](../native/docs/native-formatting.md).
Canonical VSIX/release, current whole-native/Windows/hosted and broader
STATUS/353/R1-R9/77-runtime/client/quality/Google/main acceptance remain open.

## Current integration work

Explicit LSP resourceSettings now selects shallow workspace-setting overlays
for exact captured files, through trusted host registration, project/backend
settings and pinned YAML. Requests retain item order and section projection;
unavailable scopes explicitly return null. Original project/runtime/source/write
grants remain separate, with snapshot membership, effective capacity and passive
policy checked before provider startup. Both direct and rust-analyzer-namespaced
settings are checked, including ASCII-case variants of blocked compiler env keys.
Absent/null options retain original bindings and valid wire behavior.

Qualified code 666656ba passes 788 native functions in 32 scoped groups,
zero failures/13 existing ignores, including all 579 library and ten new resource
integration functions. Final gates track 1,104 source/control inputs; the library
result is reused under unchanged own inputs after one unrelated integration
assertion spelling change, independently reconstructed by SHA256. All-target
Clippy, 361 individual format roots, build, typecheck, Python 35 and npm
260 passed/3 existing skips pass; fmt-all retains Windows OS206.

Actual CLI comparisons preserve default/absent/null wire replies and complete
original SQL rows; selected file scopes and staged actions pass. A controlled
mixed-namespace counterexample first reaches the owned fixture and then refuses
before provider startup after the fix; no wrapper or analyzed source executes.
The current explicit native runtime also passes the unchanged actual host test
on VS Code 1.143.1, including all four source/permission assertions. All Jobs end
empty. Disk-limited compilation and failed audit/lint attempts remain explicit;
exact-file lossless optimization preserves bytes and deletes nothing.
See [resource evidence](validation/integration-2026-10-23/lsp-resource-settings/README.md)
and [contract](../native/docs/lsp-resource-settings.md).
Canonical bundled VSIX/release, current whole-native/Windows/hosted and broader
STATUS/353/R1-R9/77-runtime/client/format/quality/Google/main acceptance remain open.

Native VSIX packaging now admits the bounded ZIP directory and every entry's
metadata before reading manifest or executable payloads. Duplicate/ASCII-case
aliases, unsafe portable paths, links/special/encrypted entries, file-parent
conflicts and exceeded metadata/payload limits refuse before output creation.
The original 128,000,000-byte entry cap, output preservation and current release
checksum precondition remain. Qualified source 3c08fa3f passes Python 35,
typecheck and npm 260 with 3 existing skips on 1,100 unchanged frozen inputs.
All completed Jobs are empty; Rust and extension product sources are unchanged.

An actual packager CLI trial reopens all 299 entries of a combined archive and
compares exact current production source bytes. Its native payload is explicitly
synthetic and never executed or delivered; this is packaging-contract evidence,
not a canonical executable VSIX or release acceptance. A separate old published
packager trial fails the same final adversarial tests while retaining all three
original positive tests. See [archive admission evidence](validation/integration-2026-10-23/vsix-archive-admission/README.md)
and [packaging contract](../native/docs/vsix-packaging.md).
Canonical Windows VSIX/release, hosted checks and the broader STATUS/353/77-runtime
and other acceptance requirements remain open.

The CI helper now explicitly cleans ambient TLS overrides at the compiled
Cargo test executable boundary. Existing runner/target policy is refused;
production TLS refusal and original native tests/ignores/deadlines/profiles stay
unchanged. Qualified code c2f872d0 passes Python 24, typecheck and npm 260 with
3 existing skips on 1,100 frozen inputs. The original Google HTTP CLI passes
all four tests; the original library passes 578 with 7 existing ignores. These
scoped counts overlap the historical full baseline. Hosted acceptance is pending.

The unchanged actual extension-host test also passes on official VS Code 1.143.1
with the explicit native runtime at source 2095792b. All four source/permission
assertions pass. Canonical combined VSIX remains open: the debug executable
exceeds the unchanged package entry budget, and the normal release build is
deliberately stopped on critical disk headroom. No release artifact is accepted.
See [CI and current host evidence](validation/integration-2026-10-23/ci-tls-boundary/README.md).
No system setting, privilege, Developer Mode or artifact deletion is used.
Broader STATUS/353/R1-R9/77-runtime, Windows prerequisites, approved Google
export, client/format/quality, canonical VSIX, main and release remain open.

The current integrated native default suite at 89a277f8 now has a
complete attempted inventory: all 358 Cargo-confirmed library, CLI, integration
and doctest groups, with 3,376 passed, 10 failed and 53 existing
ignored cases. All three examples compile without execution. All 1,100 frozen
source/control inputs and the primary CLI stay exact. Original test/ignore/
internal deadline/profile values remain unchanged; no function filters are used.
The final consolidated failures match unchanged original Windows OS1314
symlink-setup fixtures, before their safety assertions. An initial attempt also
records 145 prerequisite failures from a missing private test-PATH entry for the
already installed PowerShell 7.6.7. After checking its Microsoft signature,
version and SHA256, only the child PATH is corrected. All four affected whole
harnesses pass their unfiltered retests; original failed attempts remain archived.

The original 26 groups are retained after correcting a Cargo workspace-selector
error before compiler startup. A disk-headroom pause retains the next 27 groups;
those completed groups remain in the inventory. The later environment correction
retains 144 other groups and repeats only the four prerequisite-failed whole
harnesses. Two protected old failed output paths
remain metadata-only and unchanged; their normal test harnesses compile/run at
separate new suffixes. Verified successful outputs and linked library artifacts
use lossless file-only LZX optimization where needed; every selected logical size/
SHA256 remains exact and nothing is deleted. A fixture audit separately proves
unchanged Git blobs and prior frozen working bytes, disclosing nine existing
CRLF/LF checkout differences without rewriting source.

All-target Clippy, 360 individual format roots, typecheck, npm 260 passed/3
existing skips and Python packaging/test-runner 19 passed
complete; fmt-all retains Windows OS206. Tools are measured during the trial and
rechecked after it, with no claim of a before-first-group tool hash snapshot.
See [complete current-source evidence](validation/integration-2026-10-23/native-baseline-current/README.md).
This is a complete default-suite attempt; Windows symlink prerequisites, broader
R1-R9/353/77-runtime/client/format/quality and main/release acceptance stay open.
No privilege, Developer Mode, system setting, original fixture or source code is
changed for this acceptance. No CI result, main merge or release is inferred.

Explicit project/backend LSP workspace settings now project through the trusted
host and pinned YAML settings selection. Each shallow overlay starts from the
original registered workspace object; absent/null preserves prior bindings and
wire data. Existing project/runtime grants, passive checks and current HTTP
revocation remain independent. Direct queries, staged actions and configuration
callbacks use the same selected provider; no default source execution is added.

Source 61a1313f passes 777 Rust functions in 31 scoped groups,
including all 578 library and seven new integration functions, with 13
existing ignores and zero failures. Clippy all-targets with warnings denied,
locked build, typecheck, npm 260 passed/3 existing skips and 360 format roots pass
on 1,100 frozen inputs; fmt-all retains Windows OS206. Actual push/pull, staged
and authenticated HTTP/revocation tests pass. Owned old/new CLI trials retain
ordinary absent/null wire/rows, original source and existing stored records;
selected JSON and pinned YAML retain one exact session binding. All Jobs end
empty. A new YAML fixture's missing host-grant assumption is corrected without
changing product bytes. See [evidence](validation/integration-2026-10-23/server-workspace-settings/README.md)
and [contract](../native/docs/server-workspace-settings.md). Broader vendor/
resource settings, all 77 runtime reviews and current whole Windows/main/release
acceptance remain open. Original inventory, OS1314 fixtures and system settings
stay unchanged; fixture protocol evidence is distinct from vendor certification.

Explicit Git project sets now support existing registered collections through
native/stdin/stdout and authenticated HTTP Session hosts. Native/view registration
uses the selected project's full captured scope and independent registry CAS/
permission gates. Immutable native archive reads do not run Git or open original/
unused receiver source. Foreign archive calls retain current per-principal import
grants and registered existing receiver policy, with local repository scope
removed from that receiver's option projection. Git flags grant no registry,
import, runtime or source-write authority; fixed single-driver refusal remains.

Source 6477c3a1 passes 809 Rust functions in 38 scoped groups,
including all 578 library and six new collection integration functions, with
8 existing ignores and one unchanged Windows OS1314 symlink setup failure. Clippy all-targets with warnings denied,
build, typecheck, npm 260 passed/3 existing skips and 359 format roots pass on
1,098 frozen inputs; fmt-all retains Windows OS206. Real signed-source TCP tests
cover current/absent/narrowed grants, invalidated sessions and no local receiver
capture. Owned CLI validation registers scoped source and reads it after Git/
original working source is unavailable, preserving archive and source/Git bytes.
All five stdio catalogs and ordinary payload/rows stay exact; private pinned-client
health and separate explicit Python callbacks pass, with all owned Jobs empty.
The initial new-test assertion used the wrong import storage table; only that
test changes. See [evidence](validation/integration-2026-10-23/git-repository-collections/README.md)
and [contract](../native/docs/git-repository-collections.md). Broader client/format/
quality and current whole Windows/main/release remain open. Original inventory
assessments, fixtures and system settings are unchanged; no default model/runtime
or analyzed-code execution is introduced.

Explicit `serve-session --git-projects` now selects a separate host Git driver
for every existing registration, with independent index-read/Git trust flags.
Complete names, disjoint roots and source/runtime permissions retain the shared
project-set contract. Primary metadata precedes cold state; global capture,
analyzer and explicit LSP settings plus full host source policy precede first
capture and MCP output. Startup may capture only secondary source. Named resume
preserves changing file membership and refuses changed policy/preference lifetime.
Ordinary and HTTP defaults remain compatible; ignored-source overrides still
refuse explicitly under the existing Git-scope restriction.

Source 2c8b2938 passes 891 Rust functions in 44 scoped groups,
including all 578 library and seven new stdio integration functions, with 8
existing ignores and zero failures. Clippy all-targets with warnings denied,
build, typecheck, npm 260 passed/3 existing skips and 358 format roots pass on
1,097 frozen inputs; fmt-all retains Windows OS206. Actual source calls qualify
all five stdio catalogs, ordinary registered-session payloads/records stay exact,
and pinned Claude 2.1.306 connects through private stdio configuration. A separate
explicit Python trial confirms startup project/source callbacks without enabling
a default runtime. Source/Git bytes stay exact and all owned Jobs end empty. See
[evidence](validation/integration-2026-10-23/git-repository-stdio/README.md) and
[contract](../native/docs/git-repository-stdio.md). Collections, broader client/
format/quality and whole Windows/main/release remain open. Original inventory
assessments, fixtures and system settings are unchanged; no model turn or
automatic analyzed-code execution occurs.

Explicit MCP HTTP now supports a separately authorized Git host for every
registered project through `gitRepositoryProjects`. Complete default/registered
names and disjoint canonical roots are required. Each root retains its own
program, limits/follow/preference binding, source exclusions and independent
write/read-only/query-only rights. Secondary metadata precedes its cold Store;
full per-project policy precedes first capture. Primary dynamic repository scope
is never copied into secondary engines. Inactive preference changes and stale
session state refuse further operations. Ordinary and fixed-root bindings stay
compatible; client JSON grants no new authority.

Source a1709786 passes 851 Rust functions in 40 scoped groups,
including all 578 library and eleven new project integration functions, with
8 existing ignores and zero failures. Clippy all-targets with warnings denied,
build, typecheck, npm 260 passed/3 existing skips and 357 format roots pass on
1,096 frozen inputs; fmt-all retains Windows OS206. Actual source calls qualify
all six HTTP profiles, unchanged catalogs and twelve pinned Claude 2.1.306 health
calls. Ordinary registered-session payloads/records match the preceding binary;
source/Git bytes stay exact and all fourteen live parent Jobs end empty.
Initial compile/type and envelope-assertion failures are retained; product bytes
are unchanged across those attempts. See [evidence](validation/integration-2026-10-23/git-repository-projects/README.md)
and [contract](../native/docs/git-repository-projects.md). Further stdio project-set
selection, collections, broader formats/quality and whole Windows/main/release
remain open. Original inventory assessments, fixtures and system settings are
unchanged; no model turn or automatic analyzed-code execution is introduced.

Explicit authenticated MCP HTTP now adopts per-principal fixed-root host Git
capture for all six existing profiles. Separate CLI flags authorize Git index
reads and trusted execution; host-file selection cannot grant them. Metadata
preflight preserves existing host/session validation without source publication.
Authenticated initialization applies the full source policy before capture;
initial/final metadata share the original byte budget. Current preferences,
fixed roots, independent permissions and existing transport controls remain bound.

Source 2674a2f5 passes 777 Rust functions across 30 scoped groups,
including all 578 library and seven new HTTP integration functions, with 8
existing ignores and zero failures. All-target Clippy with warnings denied, build,
typecheck, npm 260 passed/3 existing skips and 356 format roots pass on 1,094
frozen inputs; fmt-all retains Windows OS206. Actual TCP validates all six catalogs
against preceding captured names, source/exclusion/read-only-child proof and
12 pinned Claude 2.1.306 HTTP health calls. Source/Git bytes stay exact; all owned
Jobs end empty. Initial prototype/fixture failures are preserved and corrected.
See [evidence](validation/integration-2026-10-23/git-repository-http/README.md)
and [contract](../native/docs/git-repository-http.md). Multiple registered projects
within a Git target, broader client/format/quality and whole Windows/main/release
remain open. Original inventory assessments, fixtures and system settings are
unchanged. No model turn or automatic analyzed-code execution is introduced.

Explicit fixed-root Git MCP hosting now selects native/session/Serena/Trail/Graft/
Graphify profiles through `--profile`. Existing catalogs and independent source/
runtime permissions remain intact. Versioned host binding covers root, selected
Git program, options/limits and preference lifetime; session identity excludes
dynamic repository membership only with that driver. New files permit unchanged
session resume; changed policy/program or concurrent state/preferences refuse.
Registered projects/collections refuse before foreign-root resolution. Ordinary
sessions retain preceding fingerprints and registered-project behavior.

Source 037f082a passes 859 Rust functions across 42 scoped groups,
including all 578 library and five new profile integration functions, with
9 existing ignores and zero test failures. All-target Clippy with warnings denied,
build, typecheck, npm 260 passed/3 existing skips and 355 format roots pass on
1,092 frozen inputs; fmt-all retains Windows OS206. Actual live validation uses
25 owned parent calls: all five added profiles retain source baseline/catalogs,
old ordinary session data/records stay exact, source/Git bytes and original rows
are preserved, and pinned Claude 2.1.306 connects to Serena without a model turn.
The broad run retains a disk-exhausted linker failure; the unchanged remaining
targets finish with a separate output suffix and lossless successful-artifact
compression, without deleting files or repeating 802 passed tests.
Variable request IDs/metrics are independently checked; eight captured native
envelopes and SQL equality are separately audited without further client calls.
Initial alias-fixture and overstrict request-ID-comparison failures remain
preserved; corrections change no product input. All owned Jobs end empty. See
[profile evidence](validation/integration-2026-10-23/git-repository-profiles/README.md)
and [contract](../native/docs/git-repository-profiles.md). HTTP/multiproject,
broader client/format/quality and whole Windows/main/release remain open. Original
inventory assessments, fixtures and system settings are unchanged.

Explicit `git repository-scope --capture --serve` now starts a fixed-root native
MCP host after initial trusted capture. An invocation-owned, non-deserializable
driver retains root/program/limits/preference binding for subsequent normal and
bounded retrieval indexing. Native tool calls recheck current host preferences;
stale changes refuse until restart. Current exact selection/read-only child policy
also guards older writable snapshots. Client JSON grants no authority. Ordinary
index/serve/catalog defaults stay exact; no source-write grant follows.

Source c42f5067 passes 760 Rust functions across 26 scoped groups,
including all 578 library functions (one new) and five new integration functions,
with nine existing ignores and zero failures. All-target Clippy with warnings
denied, build, typecheck, npm 260 passed/3 existing skips and 354 format roots pass
on 1,091 frozen inputs; fmt-all retains Windows OS206. Actual native protocol
indexing and pinned Claude 2.1.306 health run through 14 owned parent process calls;
worker children inherit the assigned Job. Old/new warm default data/catalogs,
original snapshot/graph/control rows and all source/Git bytes stay exact. All owned
Jobs end empty. No model turn, automatic analyzed-code execution, original fixture
or system/privilege changes occur. See [host evidence](validation/integration-2026-10-23/git-repository-host/README.md)
and the [host contract](../native/docs/git-repository-host.md). Broader client/facade,
session/multiproject/HTTP adoption, formats/quality and whole Windows/main/release
acceptance remain open. Original inventory assessments are unchanged.

Explicit host Git traversal preferences now persist submodule/nested follow choices
for subsequent explicitly trusted repository captures. Root/workspace-bound CAS
updates require exact expected IDs and host-write permission; clear/recreate gets
a fresh identity. Invocation overrides retain saved state. Current bindings are
rechecked before source proof publication. Saved choices grant no Git trust or
source/write authority; ordinary index and metadata-only scope stay unchanged.

Source 7a31b97f passes 754 Rust functions across 25 scoped groups,
including all 577 library functions and eight new preference integration functions,
with nine existing ignores and zero failures. Clippy all-targets with warnings
denied, build, typecheck, npm 260 passed/3 existing skips and 353 format roots pass
on 1,089 frozen inputs; fmt-all retains Windows OS206. Actual CLI uses
29 owned process calls for save/show/update/stale refusal/clear/recreate,
invocation overrides and exact old/new default data and capture identities.
Original source/Git bytes and preceding snapshot/graph/control rows stay exact;
all owned Jobs are empty. Initial six-test qualification passes; one unused import
is removed before the final eight-test gate. Original fixtures and system settings
are unchanged. See [preference evidence](validation/integration-2026-10-23/git-repository-preferences/README.md)
and the [host contract](../native/docs/git-repository-preferences.md). Default/client
adoption, broader selectors/formats/quality and whole Windows/main/release acceptance
remain open. Original inventory assessments are unchanged.

Explicit host Git repository scopes now distinguish index-pinned gitlinks from
untracked nested-clone candidates with separate follow choices. An optional capture
binds original metadata, physical names, exact working files and read-only crossed
roots into immutable source evidence. Fresh index/physical rechecks precede
publication; fixed saved-scope source checks need no live Git and publish no new
proof. Native state is excluded from index enumeration, preserving repeat IDs.

Source 0749cd68 passes 746 Rust functions across 24 scoped groups,
including all 577 library functions (three new) and 16 new integration functions,
with nine existing ignores and zero failures. All-target Clippy with warnings
denied, build, typecheck, npm 260 passed/3 existing skips and 352 format roots
pass on 1,086 frozen inputs; fmt-all retains Windows OS206. Actual live CLI uses
27 owned process calls for real local submodule/nested scopes, exact legacy data,
old-reader refusal, current/captured child restrictions, explicit host verification,
approved parent workspace application and exact recovery with Git admin unavailable.
All owned Jobs are empty; source/Git bytes are restored. The earlier passing gate
missed saved-scope recapture: a genuine 0/1 counterexample and actual prototype
apply refusal remain preserved, followed by corrected source and successful reuse
of the same approved/verified plan. Compile-test and controller attempts remain
auditable. No system/privilege/original-fixture changes occur.
See [repository-scope evidence](validation/integration-2026-10-23/git-repository-scope/README.md)
and the [host contract](../native/docs/git-repository-scope.md). Automatic preference
reuse/default client adoption, wider repository formats/quality and whole-Windows/
main/release acceptance remain open. Original inventory assessments are unchanged.

Bounded host-only repository discovery now lists nested `.git` directory/file
markers and their nearest marked ancestor under one explicit absolute search
root. Exact candidate selection remains metadata. Marker redirects, source and
configuration are not read; no Git execution, Store initialization, registration
or activation follows. Protected trees, links/reparse points and depth omissions
remain explicit; exhausted finite budgets refuse before output.

Source 7cad9775 passes 620 Rust functions across seven scoped groups,
including all 574 library and ten new integration functions, with seven existing
ignores and zero failures. All-target Clippy with warnings denied, build, typecheck,
npm 260 passed/3 existing skips and 351 format roots pass on 1,082 frozen inputs;
fmt-all retains Windows OS206. Actual Git/worktree/nested-repository and junction
fixtures use 27 owned process calls; the final discovery/refusal checks preserve
all fixture bytes and directories. Default warm index data stays exact; original
variable byte/token metrics are independently checked. Initial Unicode-controller
and overstrict-metric comparison failures are retained and audited without hiding
them or repeating the completed compatibility calls. All owned Jobs are empty.
See [discovery evidence](validation/integration-2026-10-23/repository-discovery/README.md)
and the [host contract](../native/docs/repository-discovery.md). Persisted nested
capture boundaries/lifecycle, broader selectors and full Windows/main/release
acceptance remain open. Original fixtures and system settings are unchanged.

Explicit Windows ignored-directory capture now supports the versioned
`ignoredDirectoryIdentity: windows-physical-v1` selection. Existing regular
directories resolve to physical names before capture; recorded names replay
literally. Distinct NTFS `ß`/`SS` names remain separate, while actual `é`/`É` and
DOS8.3 aliases share the correct scope. Captured and current write checks preserve
readonly boundaries through aliases and case-only renames. Missing/protected/
reparse selections refuse; legacy absent/null wire and replay remain unchanged.

Source 55c1ab70 passes 703 Rust functions across 19 scoped groups, including all 574
library and 15 new integration functions, with nine existing ignores and zero
failures. Clippy-all-targets with warnings denied, build, typecheck, npm 260 passed/
3 existing skips and 349 format roots pass on 1,078 frozen inputs; fmt-all retains
Windows OS206. Twenty-three owned process calls verify unchanged old capture IDs/
rows, physical aliases, stale-plan refusal, explicit host byte verification,
approved workspace application, exact journal recovery and junction rejection.
All owned Jobs are empty. Initial published/WIP counterexamples and failed tests
remain preserved. No system/privilege/original-fixture changes occur. See the
[identity evidence](validation/integration-2026-10-23/ignored-directory-identity/README.md)
and [capture contract](../native/docs/ignored-source-capture.md#explicit-windows-physical-directory-identity).
Broader discovery/nested-repository lifecycle, full filesystem/platform coverage
and whole-Windows/main/release acceptance remain open.

Receipt-owned Claude source hooks now provide bounded prompt source context,
incoming dependency candidates after edits and optional selected PowerShell
status output over one existing immutable source-only View. The distinct
claude-source-hooks-v1 profile preserves the preceding SessionStart profile.
Callbacks recheck physical bindings, exact installed ownership and stale
preconditions; source freshness remains explicitly unchecked. No automatic
capture, model inference or analyzed-code execution follows.

Source 8966d8ca passes 648 scoped Rust functions, including all 573 library and
nine new integration functions, with seven existing ignores and no failures.
All-target Clippy with warnings denied, build, typecheck, npm 260 passed/3 existing
skips and all 348 format roots pass on 1,075 frozen inputs. Cargo fmt-all retains
Windows OS206. Actual isolated Claude 2.1.306 dispatches session/prompt/post-edit
hooks; a controlled protocol fixture independently performs only its granted
owned-file edit. A corrected offline audit of that same trace verifies three
received 30-byte source contexts and parallel historical observations without new
client/model calls. Original SQL and other protected bytes remain unchanged.
Native refusal/stalled-stdin checks and actual old/new SessionStart plan/output
comparison pass. All owned Jobs are empty. Interactive client status rendering,
real model quality, broader hook/team behavior and whole-Windows/full-parity
acceptance remain open. See [source-hook evidence](validation/integration-2026-10-23/source-hooks/README.md)
and the [host contract](../native/docs/claude-source-hooks.md).

Explicit local CPU ASR now has a native host CLI/library contract over registered immutable mono16kHz PCM16 WAV audio. A selected local faster-whisper1.2.1/tiny.en trial transcribes owned synthetic speech; original model timestamps remain distinct from qualified captured-sample envelopes. Host trust, pinned program/model files, bounded owned execution, immutable request/control journals, checked stored replay and typed retention are implemented. Default runtime/MCP calls acquire no ASR packages, models or inference privileges.

Source dd462925 passes 622 scoped native functions with one preserved original media OS1314 failure and 7 existing ignores. All 573 library functions and six new integration functions pass. Clippy, build, typecheck, npm and 347 formatting roots pass; fmt-all retains Windows OS206. Fourteen native CLI refusal cases and actual stored replay preserve original SQL; a corrected read-only audit reuses the completed model attempt. See [ASR evidence](validation/integration-2026-10-23/native-asr/README.md) and [host contract](../native/docs/media-asr.md). General speech quality, other formats/languages and whole-Windows/hosted/full-parity acceptance remain open.

A current contract review now maps all 73 historical unreconciled entries and
the two historically unavailable dashboard entries to explicit native behavior
and remaining limits. The original 353-entry inventory, ledger assessments and
summary counts are preserved. This is source/contract reconciliation, not full
pinned upstream or runtime certification; all 77 backend runtime IDs retain
separate acceptance requirements.

All 25 project keys have explicit handlers: 21 map to bounded native settings,
while encoding, language_backend, activation_command and activation_command_timeout
accept only restricted standard values. The nine mode and 16 context names match
the pinned sequences. Native guidance and finite exclusions/gates do not establish
identical upstream prompts, Jinja behavior or branded client installations.
Dashboard viewing, PostgreSQL/HTTP/arxiv/controlled Google acquisition, database
publication, App history and selected host integrations are implemented within
their dated contracts. Broader ASR quality/formats, the hosted cloud/team service, broader hooks/statusline,
UI management and authenticated/live/quality acceptance remain open.

A bounded Windows probe runs 300 owned benign Git starts and 2,406 successful
process-list snapshots. One snapshot reports assigned=2/listed=1 at capacity4096.
This supports a possible membership race behind the earlier conservative cleanup
refusal; the exact original-fixture cause remains unproven. All known root handles
are signaled and nested/outer accounting is empty. No production cleanup check,
process privilege, system setting or original OS1314 fixture changes.
See the [75-entry current review](validation/integration-2026-10-22/current-status-audit/README.md)
and its exact source/index pins. No product runtime test is newly claimed by this
read-only mapping; the following source-pinned qualification remains separate.

Compiler source contracts now include actual TypeScript constructor parameter
properties on captured named classes. Physical parameter-property flags, an in-body
constructor owner and exact class/constructor/property ownership proofs are
validated. Ordinary parameters remain distinct. Field-to-parameter,
parameter-to-parameter and parallel interface bindings retain original source proofs
without inventing a generic instance, invocation or runtime execution.

A genuine counterexample fails 0/1 against byte-identical published c0ad70fb
production, then passes after correction. Anonymous/class-expression owners remain
explicitly partial: a real old/new CLI regression was caught and fixed, Both
versions accept the valid input; the new report explicitly identifies missing
constructor-property owner coverage. The additional relation is omitted without
dropping existing property observations. Six forged
flag/owner/body/ownership-receipt mutations are rejected.

Source a1e6d407 passes 784 Rust functions across 17 scoped groups, including all 569
library tests and 34 compiler-affected tests, with 9 existing ignores and zero
failures in the final qualified groups. All-target Clippy with warnings denied,
native build, typecheck, npm (260 passed, 3 existing skips) and 346 format roots
pass on 1,065 frozen inputs. Focused R3 results are reused only under the identical
source manifest. The completed library is likewise reused. One earlier broader
attempt records an unchanged Windows member-list cleanup refusal (11 pass/1 fail);
its one unchanged-source group repeat passes 12/12. The initial failure is retained,
cleanup is not weakened, and its cause remains a separate open investigation. Cargo
fmt --all retains Windows OS206; every actual formatting root is checked.

Actual native Git/compiler-affected CLI checks use trusted TypeScript 5.8.3 over
owned Unicode/CRLF snapshots, field/constructor-property contracts, parallel
interface proofs and depth-zero behavior. The execution canary remains absent;
source/Git and whole fixture SQL remain exact. Old/new default configuration, MCP53
and literal LSP data remain exact apart from independently old-repeat-proven
requestId/duration. The original SQL reconstructs exactly after removing only new
dashboard session rows in read-only memory. All owned Jobs are empty.

The ten original symlink fixtures remain byte-exact. Their Windows OS1314
acceptance, broader anonymous/implicit/receiver/heritage effects, exhaustive
compiler quality, all 77 runtimes and whole-native/main/release acceptance remain
open. System settings and original 353 inventory assessments are unchanged. See
[constructor-property
evidence](validation/integration-2026-10-22/compiler-parameter-properties/README.md)
and [compiler-affected contract](../native/docs/git-compiler-affected.md).

### Remaining scope

Open work is summarized here. Later sections preserve results for their stated
source commits; the [residual register](parity/native-reconciliation-2026-10-15.json)
retains the detailed boundaries and original inventory assessments.

| Area | Work and acceptance still required |
| --- | --- |
| Refactoring/compiler | Broader declarations, languages/configurations, implicit effects, reference and provider-intent completeness. |
| Retrieval/source identity | Richer repository discovery/selectors, nested repository lifecycle, cross-history/workspace joins and quality qualification. |
| Configuration/clients/UI | Additional YAML layouts, Jinja, restricted-default project extensions, broader client hook/status behavior, dashboard management and broader UI/accessibility acceptance. |
| Backends/providers | Per-runtime review of all 77 backend IDs, remaining hosted dialects and licensed external backends. |
| External acquisition | Broader ASR quality/formats, authenticated Google export, remaining social/video/authenticated routes and broader database version/mixed-source qualification. No approved Google test document is available. |
| App/cloud/receiver | Live App installation/deployment, remaining claim/CAS/source-digest workflows and hosted team/client services. |
| Semantics/quality | Broader corroboration/outcome learning, independent correctness/cost campaigns and measured model/provider quality. |
| Windows/release | Ten original OS1314 prerequisites, sporadic owned-member cleanup refusal, current-head whole-native/hosted and extension-host acceptance, main/release/package remote verification. |

### Earlier source checkpoints

Actual compiler property/accessor and callable-slot source contracts now use
separate overridesProperty/implementsProperty relations. Changed unobserved
generic getters or callable fields can retain consumers bound to base/interface
slots without inventing a generic instance or runtime dispatch. Getter/setter
contracts stay separate, while structural property references and their actual
access modes remain intact. Physical roles, static sides, private slots, exact
source/owner/heritage anchors and shared proof/depth/budget limits are checked.

The causal getter test fails 0/1 on byte-identical published326a production
implementations after actual compiler/source-containment/no-instance admission.
The correction passes, together with parallel interface-clause and adversarial
role/owner/heritage/static/dispatch cases. Source contracts cost zero incoming
steps and retain their original relation IDs/proofs and separate basis; they are
not new invocation or observed-execution facts. Uncaptured/type-literal heritage and constructor-parameter property
contracts, broader receiver/heritage-only/implicit effects and exhaustive semantic
or runtime completeness remain open.

At source 0ac3a2dc, 780 Rust functions pass across 17 scoped groups,
including the full library569 and30 compiler-affected functions. 9 existing
ignores and zero failures occur in this scope. All-target Clippy -Dwarnings,
native build, typecheck, npm260 passed/3 existing skips and346 actual format roots
pass on1065 frozen inputs. Cargo fmt --all retains Windows OS206. R2 focused
results are reused only on the identical frozen manifest. All ten original
symlink fixture Git/prior working bytes remain exact; their Windows acceptance
is still open. No new skip, system/privilege or protected-linker change occurs.

The actual native Git-workflow/compiler-affected CLI uses trusted TypeScript 5.8.3
over owned captured Unicode/CRLF sources, records the new getter and parallel
callable-property bindings and confirms depth-zero behavior. Its source-execution
canary remains absent; original source/Git and whole fixture SQL stay exact.
Actual old/new default configuration/MCP53 and literal LSP data JSON remain exact;
only requestId/duration are excluded after an independent old-binary repeat.
Original fixture SQL is exact before a new optional dashboard binding and
reconstructs exactly after removing only its new session rows in read-only
memory. Existing dashboard host-exit cleanup remains checked. All owned Jobs
are empty. This is scoped evidence, not current whole Windows/all 77/full parity.
See [compiler property-contract evidence](validation/integration-2026-10-22/compiler-property-contracts/README.md)
and [compiler-affected contract](../native/docs/git-compiler-affected.md).

An explicit host dashboard setting now enables a private read-only native
session view through open_dashboard, cfg.open_dashboard() or openDashboard.
It shows session/project/modes, selected public tools, language/workspace
selections and bounded structured native-operation activity. No listener starts
by default; absent/null settings preserve previous identities and default bytes.
The client opens the returned private URL; native code does not launch a browser.

The IPv4-loopback listener checks Host/Origin and a fresh capability, bounds
HTTP framing, headers, response sizes, time, lifetime and request count, and
refuses mutation. Each data read checks existing-only read-only state against
the exact workspace/session binding. Activity contains no arguments, results,
source bytes, launch commands or error text. Host exit and MCP-HTTP revocation
close access. Source, configuration and memory editing, raw logs, tray handling,
automatic browser opening and MCP shutdown remain unavailable in this view.

At source 1191fedd, 736 Rust functions pass across 21 scoped groups,
including the complete library (569 passes) and 13 dashboard functions.
8 existing ignores and zero failures occur in the qualified scope.
All-target Clippy with warnings denied, native build, typecheck, npm260 passed/
3 existing skips and 346 actual format roots pass on 1065 frozen inputs.
Cargo fmt --all retains Windows OS206; bounded rustfmt checks every root.
The ten original symlink fixture Git and prior working bytes remain exact.
No system/privilege changes, new skips or fixture replacements occur.

Actual old/new default configuration/MCP53 and literal LSP data JSON remain exact.
Only requestId/duration are excluded after an independent old-binary repeat.
Original SQL is exact before the new binding and reconstructs exactly afterward
by removing only new session rows from a read-only in-memory copy. Actual native
MCP open/close and stale upgrade/downgrade refusal are checked. Private capability
URLs are redacted before transcript logging and excluded from the archive.

Actual desktop/mobile screenshots were inspected during live running/completed/
failed activity, tool filtering and mode changes. The first visual round found
long project names widening the 390px layout to718px; corrected CSS was rebuilt,
retested and visually checked at1440px and390px without page overflow. Owned
test/browser Jobs are empty. This is scoped acceptance, not full accessibility,
all-browser, all 77-runtime, original 353/R1-R9 or current whole Windows certification.
See [dashboard evidence](validation/integration-2026-10-22/dashboard/README.md)
and [dashboard contract](../native/docs/session-dashboard.md).

Broader configuration/UI, authenticated Google (no approved test document),
the ten original Windows prerequisites and other listed STATUS work remain open.

Additional actual Windows runtime qualification at source d792a53c uses
installed rust-analyzer 1.94.1 (e408947b 2026-03-25). Native MCP refactor.prepare/rename
changes a declaration and cross-folder use across distinct U+0130/i+U+0307
primary/additional roots in an owned single crate. Original source hashes and
UTF-8 name offsets, both expected Unicode-name/CRLF after-blobs, all four base
members and the derived manifest are exact. Original files stay unchanged;
exactly one plan and no approval, application receipt or journal are created.
Host read-only, Planning mode and stale binding refuse; the actual provider
rejects ../../escape as an invalid identifier (-32602). Source execution and
provider commands remain unauthorized; no dependencies are supplied. This adds
runtime/operation evidence without production changes or new Rust-suite counts.
It does not certify other crates, compiler/reference completeness or all 77
backends. See [actual Rust rename evidence](validation/integration-2026-10-22/rust-rename-live/README.md).

The database residual summary now distinguishes implemented connectors from
remaining qualification. Reviewed native Neo4j and FalkorDB publication already
exists, with historical Windows TLS, exact evidence-row, concurrent-CAS and
no-resend recovery cases in the indexed 2026-10-19 checkpoints. An integrity
audit at source 3d5f39e6 verifies all 1732 indexed artifacts and historical source
ancestry. It does not rerun database services or certify the current binary live.
Other server versions/JVMs, production/RBAC, non-Windows and full mixed-source/
upstream equivalence remain open. The controlled Google export implementations
are also separate from authenticated Google live acceptance, which still lacks
an approved test document. Original inventory assessments remain unchanged.
See [database status reconciliation](validation/integration-2026-10-22/status-db-reconciliation/README.md).

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
See [Unicode workspace evidence](validation/integration-2026-10-22/unicode-workspace/README.md)
and [configuration contract](../native/docs/session-config.md#fields-and-boundaries).

This does not certify all filesystem alias rules, all 77 runtimes or broader
semantic quality. External/foreign roots, other project-setting extensions,
original 353/R1-R9, authenticated Google, current whole Windows, main/release and
the other listed status work remain open.

Passive Rust now omits captured rust-project.json selectors from its private
provider copy at every directory depth, with case-insensitive filename matching.
Original bytes, capture, graph and source hashes remain unchanged; omission
receipts identify exact original paths/hashes. Ordinary similarly named files
remain available, and explicit host source-execution authority retains selectors.
New JSON-aware source-policy strings bind new provider evidence. Original policy
strings retain their original exact omission meaning for archived overlays;
old evidence is not rewritten or upgraded. Captures without JSON selectors retain
preceding policy strings and default results.

At source 2721b39d, 694 Rust functions pass across 18 scoped groups,
including the full library (569 passes). 10 existing ignores and zero failed
functions occur in this scope. One expanded provider-copy test and one new
versioned-overlay function cover actual file delivery, host authorization,
hash/order/set refusals and immutable prior results. All ten original symlink
fixture Git and frozen working bytes remain exact; their Windows prerequisite
acceptance remains open. No fixture skips, privileges or system settings change.
All-target Clippy -Dwarnings, native build, typecheck, npm 260 passed/3 existing
skips and 345 actual format roots pass on 1061 frozen inputs. Cargo fmt --all
retains OS206. This is not whole current Windows certification.

The causal test fails against unchanged published provider code because the actual
fixture sees both JSON selectors. Current code passes. Actual old/new CLI default
configuration/MCP53 and literal LSP data JSON remain exact in the same owned
fixture; whole SQL before/after is unchanged. Only Native requestId/duration are
excluded after independently repeating the old binary. The installed rust-analyzer
1.94.1 still resolves an exact cross-folder reference when an owned JSON selector
points outside captured source: that selector is omitted and hash-reported with no
source-execution grant or external dependencies. This is one controlled
runtime case, not a general runtime-effect or semantic-quality certificate.
See [passive JSON-selector evidence](validation/integration-2026-10-22/rust-json-selector/README.md)
and [provider authority](../native/docs/provider-authority.md).

Broader all-77 runtime/vendor settings/install/version acceptance, remaining
project-setting extensions, original 353/R1-R9, authenticated Google, current whole
Windows, main/release and other listed status work remain open.

Optional additionalWorkspaceFolders and pinned YAML ls_additional_workspace_folders
now add relative semantic roots from the same immutable captured source. Independent
global/project/local selection leaves the primary native symbol index and capture
policy unchanged. Null inherits; an explicit empty local/project list clears inherited
extras. Absent/null/all-empty policies preserve default serialized identities.
External, foreign-snapshot, protected, ignored-only and missing roots refuse before
provider startup. Effective host roots are preserved and deduplicated within budgets.
Common transport, staged/owned refactoring, refresh and folder-move admission use
the same effective roots and current session binding. YAML grants no runtime trust.

At source 6eece132, 833 Rust functions pass across 37 scoped test groups,
including the full library (569 passes), eight new integration functions and two
new YAML functions. Thirteen existing ignores and one unchanged original OS1314
symlink prerequisite remain. All ten original fixtures keep exact Git and frozen
working bytes. No test skips, system or privilege changes were introduced.
All-target Clippy -Dwarnings, native build, typecheck, npm 260 passed/3 existing
skips and 345 actual formatting roots pass on 1061 frozen inputs. Cargo fmt --all
retains Windows OS206; this is not whole current Windows certification.

Actual old/new CLI same-fixture checks preserve default configuration/MCP53 and
native LSP data RAW, primary symbols and original SQL. Only Native requestId and
metrics.durationMs vary; an original-binary repeat independently proves this and
all other envelope fields plus the literal data JSON bytes are compared. New
sessions can be removed from a read-only memory backup to reconstruct original
SQL exactly. YAML/JSON bindings, stale upgrades/downgrades and source bytes pass.
The installed rust-analyzer 1.94.1 also resolves an exact source-bound cross-folder
reference in an owned one-crate project without source-execution authority or
external dependencies. Manifestless and unusual outside-package attempts remain
recorded failures; they do not certify broader semantic quality or all 77 runtimes.
See [semantic workspace evidence](validation/integration-2026-10-22/semantic-workspace/README.md)
and [configuration contract](../native/docs/session-config.md#fields-and-boundaries).

The additional-folder setting remains partial for external/foreign sources.
Vendor settings/install/version/runtime work, other project-setting extensions,
broader configuration/client work, authenticated Google, original 353/R1-R9/current
Windows/main/release acceptance remain open. No full parity certificate is issued.

Standard project YAML defaults now confirm existing native UTF-8/LSP behavior,
no activation command, default integer timeout 180 and no additional workspace
folders. These values add no inert HostProfile policy or runtime authority.
Pinned-source receipts identify accepted defaults and explicitly disclose the
unavailable extensions. Non-UTF8, active commands, other timeouts, JetBrains and
nonempty additional folders refuse with named unavailable errors. Public
added_modes:null now retains inherited additions, equivalent to an empty list.

At source 7104f358, 153 Rust functions pass across 20 scoped test groups:
25 YAML/library-filter functions, five CLI functions and 18 integration targets.
Four new functions cover complete owned standard-default data, equivalent profile
bytes, type/scope/Unicode/path/budget refusals and actual CLI export. There are
1 existing ignores and no failed function in this scoped run. This is not
an unfiltered library or whole-native certificate. All ten original symlink test
Git blobs and prior frozen working bytes remain exact; their Windows prerequisite
acceptance remains open. No original symlink tests, system settings or privileges change.

All-target Clippy -Dwarnings, build, typecheck, npm 260 passed/3 existing skips and 344
actual format roots pass on 1058 frozen inputs. Cargo fmt --all retains OS206.
The causal Rust default-file test fails 0/1 against the published parser, then
passes with current source. Actual old reduced/new full CLI in the same fixture
preserves exported profile bytes, original-session configuration/MCP53/symbol RAW,
and whole SQL before and after. Old reduced plan RAW also remains exact. Source,
Git, host, old-image and YAML bytes remain exact; no registered server starts.
See [native default compatibility evidence](validation/integration-2026-10-22/project-defaults/README.md)
and [configuration contract](../native/docs/session-config.md#fields-and-boundaries).

Six project settings retain partial/unimplemented extension scope. Vendor-specific
server dialect/install/runtime work, broader configuration/client work, authenticated
Google, original 353/R1-R9/current Windows/main/release acceptance remain open.

Host-authorized languageServerSettings and pinned YAML ls_specific_settings
now apply five generic settings to actual selected catalog registrations:
initializationOptions, ls_path, ls_base_cmd, ls_args and ls_extra_args.
Backend entries replace local > project > global; null inherits and an empty
backend entry restores original host defaults. Initialization options merge only
top-level keys; existing mandatory passive options remain enforced. Every project
uses the original registry, preserving independent project defaults.

The host separately authorizes trustedServerSettingsProjects. YAML cannot grant
that authority, registration trust or source-execution rights. Changed launch
commands/arguments additionally require the registered allowSourceExecution;
TCP launch overrides refuse. Current passive checks run before process startup.
Absent/null/empty policy maps and dormant grants retain default RAW/bindings.
Changed policy/registry/grants bind sessions; stale clients refuse.

At source 78a14774, 827 Rust functions pass across 37 scoped Cargo targets,
including the complete library (564 passes), eight new integration functions and
two new YAML functions. There are 13 existing ignores and one unchanged
original OS1314 symlink-fixture failure. All ten original fixture Git blobs and
prior frozen working bytes stay exact; no skips or system/privilege changes.
All-target Clippy -Dwarnings, build, typecheck, npm 260 passed/3 existing skips and 344
actual formatting roots pass on 1057 frozen inputs. Cargo fmt --all retains OS206.

Actual old/new CLI same-fixture checks preserve default configuration/MCP53/symbol
RAW and whole SQL before the new settings binding. A read-only memory backup
reconstructs original SQL after removing only new session rows. Actual LSP options,
YAML/JSON binding equivalence, upgrade/downgrade and grant revocation pass. New
integration tests cover initialization, launch args, project changes, provider
refresh, staged/owned refactoring, HTTP revocation and schema/budget boundaries.
See [generic settings evidence](validation/integration-2026-10-22/server-settings/README.md)
and [configuration contract](../native/docs/session-config.md#fields-and-boundaries).

This qualifies the generic setting subset. Vendor-specific settings, core-
dependency/JAR interpretation, installation/version options and 77-runtime review
remain open. Six project settings retain partial/unimplemented scope, together
with broader configuration/client work and original 353/R1-R9/current Windows,
authenticated Google/main/release acceptance. No full parity certificate is issued.

Explicit gitIgnore and pinned YAML ignore_all_files_in_gitignore now select
real source capture, symbol indexing and pattern-search defaults independently
per registered project. False includes authorized Git-ignored sources; .ignore,
hard exclusions, secrets, links, source exclusions and budgets still apply.
Original UTF-8 bytes/offsets and immutable observed-ignore facts remain bound.
Included ignored sources stay read-only; current local ignore rules separately
reject prospective workspace writes, including old plans, before a journal.
Historical plan validation uses original captured policy, preserving saved bytes.
Ordinary approved/verified edits and guarded recovery still work.
Absent/null/empty policies retain preceding profiles/captures/bindings and configuration/symbol RAW. Public empty-glob pattern defaults are separately corrected to mean no restriction.

At source cf222d7c, 923 distinct Rust functions pass: 915 in
51 scoped Cargo targets plus eight unchanged original capture-option functions
compiled under an owned output name. The complete library contributes 562 passes.
There are 14 existing ignores and one unchanged original OS1314 failure
during symlink fixture creation. All ten original fixture Git blobs and prior
frozen working bytes remain exact. No skips, system or privilege changes occur.
Eleven new functions cover real inclusion/indexing, project layering/stale state,
source/authority/budget boundaries, original plan preservation/current write
admission, ordinary apply/recovery, LSP, archive/retention and YAML. The public
pattern-search default has a causal one-test RED followed by actual GREEN.

All-target Clippy -Dwarnings, native build, typecheck, npm260/3 existing skips and
343 format roots pass on 1054 frozen inputs; Cargo fmt --all retains OS206.
Actual published CLI RED/SAME-fixture GREEN preserves absent/null/empty config
and MCP53/symbol RAW, and whole original SQL before selected capture. New captures
append source data; original snapshots/graphs/cache/records and old FTS row
multiplicity remain exact. Source/Git/host/YAML/original-image bytes stay exact.
Actual selected symbol/workspace/pattern queries, true filtering, YAML/JSON
binding equivalence and upgrade/downgrade refusals pass. This does not claim
whole SQL equality after capture growth, or a current whole-native certificate.
See [indexed Git-ignore evidence](validation/integration-2026-10-22/git-ignore-policy/README.md)
and [the configuration contract](../native/docs/session-config.md#fields-and-boundaries).
Six project settings, broader backend/configuration/client, authenticated Google
and original 353/R1-R9/current Windows/main/release acceptance remain open.

Ordered languageServers now selects actual current host-registered catalog
backends per project. Local overrides project, then global; null inherits.
The first backend supporting the file extension wins, otherwise the first is
the explicit fallback. Provider errors do not silently try another backend.
CLI and HTTP host registrations remain separate from YAML/project policy,
require explicit runtime authority and start nothing merely by registration.
Direct/staged/owned LSP paths and lifecycle refresh share this selection;
mixed-backend directory moves refuse before provider or plan publication.
Changed registry/configuration/order binds a new session identity. Absent/null/
empty policies and dormant registrations retain preceding default behavior.

At source 60d9379b, 294 Rust functions pass across 39 scoped targets,
with 5 existing ignores and one unchanged original OS1314 failure during
symlink fixture creation. Ten new functions cover actual ordering/fallback,
independent projects, stale bindings, schema/authority/budget adversaries,
direct LSP, owned symbol move, lifecycle/cancellation, HTTP revocation and YAML.
All ten original symlink fixture Git blobs and earlier frozen working bytes
remain unchanged. No skips, system settings or privileges were changed.

All-target Clippy -Dwarnings, native build, typecheck, npm260/3 existing skips
and 342 actual format roots pass on 1051 frozen inputs. Cargo fmt --all retains
Windows OS206; bounded direct rustfmt checks every root. Actual published CLI
RED/SAME-fixture GREEN preserves absent/null/empty and dormant-registry config
RAW and MCP53/real-hover RAW. Original whole SQL remains exact before new
bindings; a read-only memory backup reconstructs it exactly after removing only
four new session rows. Original stored values, source/Git/host/provider/fixture
bytes stay exact. Actual selected CLI/MCP calls verify overlap order and fallback;
legacy-only registration and changed session bindings refuse.
The first RED harness failed on an incorrect response-envelope assertion;
corrected RED2 records genuine old-CLI positives and new-schema refusals.
See [indexed server-selection evidence](validation/integration-2026-10-22/server-selection/README.md)
and [the configuration contract](../native/docs/session-config.md#fields-and-boundaries).
Seven project settings, broader configuration/backend/client, authenticated
Google and original R1-R9/353/current Windows/main/release acceptance stay open.
Catalog extension matching does not certify any of the 77 runtime backends.

Workspace selections now reach the common LSP transport before passive-policy
validation, provider-profile hashing and process startup. Direct navigation,
refactoring queries, staged edits, provider restart and owned symbol-move intents
share the effective configuration. Directory-move preparation checks those same
roots and refuses relocating a selected workspace root or its ancestor.
The source/index distinction, full private captured tree and existing runtime,
write, verification, recovery and immutable-evidence boundaries remain intact.

The post-publication audit of 49e9b56d found four bypassed paths. Four new actual
tests fail against that source, then all four pass with centralized selection:
direct configured transport, inspection registry, owned symbol move and folder
root relocation denial. The preceding nine workspace and sixteen YAML functions
also pass again. At source 1b497994, 251 functions pass across 32 scoped
targets with 4 existing ignores and one unchanged original OS1314 failure.
That failure occurs while creating the symlink fixture in facade_file_delete;
it is neither skipped nor converted to a pass. All ten original fixture Git blobs
and their earlier frozen working bytes remain unchanged. Full Windows acceptance
remains open.

All-target Clippy -Dwarnings, native build, typecheck, npm260/3 existing skips and
341 actual formatting roots pass on 1048 frozen inputs. Cargo fmt --all retains
Windows OS206; bounded direct rustfmt checks all roots. Actual published CLI RED/
SAME-fixture GREEN preserves selected and unselected configuration RAW and
53-tool MCP with real selected-root hover RAW. Whole logical SQL stays exactly
unchanged before and after the comparison, together with source/Git/host/provider
configuration/fixture bytes. The source-specific 171-function earlier checkpoint
remains historical; these current results do not certify the whole native suite.
See [indexed route evidence](validation/integration-2026-10-22/symbol-workspace-routes/README.md)
and [the workspace contract](../native/docs/session-config.md#fields-and-boundaries).
Eight project settings and the broader configuration/backend/client, authenticated
Google, original R1-R9/353/current Windows/main/release acceptance remain open.
No system setting, original inventory assessment or protected process changes.

Host-selected symbolWorkspaceFolders and YAML ls_workspace_folders now define
a separate structural symbol-index projection and real LSP workspace roots.
Filtering precedes paging, root match thresholds, body search and descendants.
Global/project/local selections resolve independently for registered projects;
absent/null/empty policy preserves original wire bytes and session bindings.
The full authorized captured source tree, root configuration, original graphs,
source IDs, explicit graph views and guarded edit/recovery behavior remain intact.
This does not claim avoided capture/parsing work or complete compiler indexing.

At source 5c6812b6, 171 distinct Rust functions pass across 24 scoped
targets with 2 existing ignores on 1047 frozen inputs, including 11 new
functions. Genuine owned LSP initialization advertises workspace support,
receives private selected folders and answers the subsequent server callback
with the identical list. Root configuration remains available. Invalid callback
parameters, missing/ignored/file-valued roots, paths, schema and aggregate
budgets refuse. Explicitly captured ignored sources stay readable but cannot
become index roots or symbols merely through that read grant. Independent
projects, stale sessions and changed bindings retain their gates.

The first preflight catches a new test-helper character/string type error.
After correction, R1 passes 16 YAML library and six integration functions.
A protocol review then adds the workspace capability/callback and adversaries;
a separate ignore-proof review closes the explicit ignored-capture edge case.
Those source changes receive new frozen checks: final reuses only R2/preflight4,
never earlier successes on changed code. All-target Clippy -Dwarnings,
build/typecheck, npm260/3 existing skips and all 340 formatting roots pass.
Cargo fmt --all retains the Windows OS206 command-length limit.

Actual old CLI RED/SAME-fixture GREEN keeps absent/null/empty configuration,
53-tool MCP and full symbol-search RAW replies exact. A read-only in-memory
database copy, minus only the two new session records, reconstructs full original
logical SQL exactly; original stored values remain exact. Selected MCP finds only
indexed source; other authorized source reads and
original mixed-newline immutable plan preparation still work. Original source,
Git, host and YAML bytes remain exact; equivalent YAML/JSON resumes one binding,
and upgrades/downgrades refuse. The first GREEN harness stops because its
expected Unicode path was encoded as ASCII ?. Recorded successful outputs are
reviewed without rerunning that segment. Its first recovery harness accidentally
shadows the Windows Job API binding with a row key and fails before starting the
next command. The corrected recovery validates the same recorded outputs and
SQL reconstruction, then completes the remaining real CLI checks. Both failed
harnesses remain failed; product sources stay unchanged. See [the contract](../native/docs/session-config.md#fields-and-boundaries)
and [indexed evidence](validation/integration-2026-10-22/symbol-workspace-folders/README.md).
Eight project keys, broader configuration/backend/client, authenticated Google,
full R1-R9/353/current whole-native/Windows/main/release acceptance remain open.
Original inventory assessments, OS1314 fixtures and system settings are unchanged.

Optional symbolInfoBudgets and selected YAML symbol_info_budget now resolve
global/project/local soft limits for supplementary LSP hover information.
Positive finite seconds round upward to native milliseconds; zero is unlimited.
After elapsed budget, subsequent optional requests stop with explicit per-item
omissions and retrieved/omitted counters. Mandatory source search, anchors,
body semantics, JB calls and host timeout/cancellation retain their boundaries.
In-flight requests are not interrupted by this soft setting. Provider errors
and host cancellation never become successful partial replies. Absent/null/empty
selection preserves original serialized identities, replies and source IDs.

At source da23e006, 119 Rust functions pass across 16 scoped targets with
0 existing ignores on 1044 frozen inputs, including 11 new functions.
Actual delayed owned LSP requests exercise early stopping, zero/unlimited,
independent projects, exact Unicode/CRLF source bindings, provider errors,
stale sessions and cancellation after a real hover starts. A persistent Python
worker checks qualified declaration bridges. R1 has 16 library passes and 5/7
integration passes; two new tests violate existing JB argument/pattern contracts.
R2 corrects those inputs and reaches 6/7; its final expected tuple still contains
the old JB hover string. The remaining expectation is corrected; protections
stay intact. A module-order format finding is corrected before the current
freeze. R3 passes the complete focused suite on current bytes. All-target
Clippy -Dwarnings, build/typecheck, npm260/3 skips and all339 format roots pass;
the combined Cargo format command retains the Windows OS206 limit.

Published CLI RED/SAME-fixture GREEN keeps absent/null/empty configuration,
53-tool MCP and actual hover RAW replies exact, together with full expanded
legacy SQL before new bindings and original stored values afterward. Actual MCP
also qualifies bounded partial and zero/unlimited hover requests. Original
mixed-plan/source-read/preparation behavior, source/Git/host/YAML/fixture bytes
and equivalent JSON/YAML session bindings remain intact. The original hover
command succeeds, but its first harness reads the outer native envelope as the
payload and fails; separate readback qualifies that exact recorded success,
without rerunning or relabeling the failed harness. See [the contract](../native/docs/session-config.md#optional-symbol-information-budgets)
and [indexed evidence](validation/integration-2026-10-22/symbol-info-budget/README.md).
Nine project keys and broader configuration/backend/client, authenticated Google,
full R1-R9/353/current whole-native/Windows/main/release acceptance remain open.
Original inventory assessments, OS1314 fixtures and system settings are unchanged.

CI fixture follow-up at source d87dd7e2 introduces a process-local native
test launcher. It removes only the three ambient TLS override variables for the
controlled fixture baseline, retains every Cargo test/thread argument, preserves
the caller environment and leaves the entire native subtree unchanged.
Explicit override adversaries still refuse. On 1041 frozen inputs, 13 Google
HTTP unit and four CLI tests pass with synthetic ambient overrides, plus 19
Python tests, focused Clippy, typecheck, npm260/3 existing skips and 338 format
roots. The reported Ubuntu failure and complete local evidence are indexed in
[the CI follow-up](validation/integration-2026-10-22/native-test-tls-environment/README.md).
Remote CI requalification remains pending; full acceptance is still open.

Explicit sourceExclusions and selected YAML ignored_paths now add bounded
Gitignore-style negative rules per global/project/local host layer. Independent
groups combine by exclusion; a later layer cannot reopen an earlier restriction.
Concrete case semantics and ordered patterns bind immutable capture options.
Current preparation and workspace application also check the current host scope,
including plans based on older snapshots, before journals/source writes.
Historical reads, checked replay, isolated verification copies and recovery keep
their original evidence and bytes. Secondary projects derive from original host
options, independently of primary-project rules. Empty/absent/null policies keep
legacy identities. These are source selections, not OS access-control grants.

At source 6b957ebc, 206 Rust functions pass across 25 scoped targets with
4 existing ignores on 1039 frozen inputs; 12 functions are new. Initial
Clippy finds two test-only default-field assignments, corrected to equivalent
initializers. All-target Clippy -Dwarnings then passes. The first regression
attempt fails linking capture_options (LNK1000/0xc0000006, nested 0xc000007f);
a cleanup hash error initially masks that failure. A retry reports LNK1104 for
the same output. Failed artifacts and protected processes are preserved.
An explicit-package cargo rustc build under a fresh EXE filename runs the same
unchanged capture_options tests successfully. No environment cause is certified.
The first fresh-output command rejects the virtual manifest; its corrected
package selection passes. A temporary launcher typo starts no Cargo and is fixed.
No failed/partial result counts as a pass. Focused tests, Clippy and the genuine
fresh-output test are reused only with identical source hashes and empty Jobs.
Build/typecheck and npm260/3 skips pass. Cargo fmt --all hits Windows OS206;
all 338 actual Cargo format target roots pass bounded rustfmt checks.
Actual published-CLI RED/SAME-fixture GREEN preserves absent/null/empty CLI/MCP
RAW, whole original SQL before additions and original records afterward.
Old mixed-plan/historical bytes, source/Git/host/YAML and equivalent JSON/YAML
session bindings remain exact. Owned executed verification, approved application
and recovery are exercised in Rust, including current-scope refusal of an older
fully approved/verified plan. See [the contract](../native/docs/session-config.md#source-exclusion-groups)
and [indexed evidence](validation/integration-2026-10-22/source-exclusions/README.md).
The ten remaining project keys, broader configuration/backend/client and full
R1-R9/353/Windows/main/release acceptance remain open. Original inventory
assessments, OS1314 fixtures and system settings are unchanged.

Explicit sourceLineEndings now resolves global/project/local LF, CRLF or native
source-write settings. Selected YAML line_ending maps to the same host slots.
Original UTF-8 edits precede formatting; only edited after-files are converted.
Concrete plan metadata, versioned identity, exact hashes/diffs and checked replay
bind the result. Existing plans are never reformatted during application;
conflicting current workspace settings refuse before journal/source writes.
Isolated historical copies and recovery retain exact planned/original bytes.
Provider diagnostics retain their pre-formatting scope and formatted results
require verification. Notes/configuration and all source/runtime gates remain
independent. Absent/null retains original host serialization and bindings.

At source 159e5f53, 285 Rust functions pass across 28 scoped targets with
4 existing ignores on 1037 frozen inputs, including 14 new functions.
The first attempt finds a missing default in the read-only Store constructor;
the next has 8/10 new integration passes and two incorrect error-envelope
expectations. Corrected expectations pass all ten; no protection is weakened.
After the 600-second compiler-body attempt ends with 13/17 partial passes and
proven cleanup, Clippy finds one collapsible YAML condition. Its equivalent
let-chain is corrected. All 28 targets rerun on the current bytes; body uses the
previously established 1200-second outer budget. No failed/partial target is reused.
Static checks are reused only on identical current inputs. All-target
Clippy -Dwarnings, build/typecheck and npm260/3 skips pass. The combined Cargo
format command hits Windows OS206; all 337 actual Cargo format target roots
pass rustfmt checks in bounded argument batches. Real LSP/TypeScript, verified
apply/recovery, conflicting old approved plans, Unicode/paths/budgets and YAML
layer/null controls are qualified. Actual published CLI RED/SAME-fixture GREEN
keep absent/null CLI/MCP raw replies, full legacy SQL before new bindings and
all original records afterward. Original source/Git/host/YAML and mixed old-plan
bytes remain exact. See [the contract](../native/docs/session-config.md#source-line-endings)
and [indexed evidence](validation/integration-2026-10-22/source-line-endings/README.md).
Original inventory assessments, OS1314 fixtures and system settings are unchanged.
Broader configuration/backend/client and full R1-R9/353/Windows/main/release
acceptance remain open.

Host primaryReadOnly now binds an explicit primary-project source restriction.
Absent/false preserves prior profile bytes, fingerprints and replies. True blocks
native/MCP/REPL edit/refactor preparation and workspace application, including an
already approved/verified plan. Independent secondary projects retain the global
host write grant; writable overlapping aliases/ancestors/descendants refuse.
Selected primary YAML read_only only tightens that restriction. Semantic query
plan carriers share the editing gate before server execution. Read access and
independent note/host-runtime permissions remain separate.

At source 4367539f, 155 Rust functions pass across 23 scoped targets with two
existing ignores, on 1035 unchanged hashes. The initial new target has seven
passes and one positive test blocked by its missing executed verification. That
fixture now runs an explicit owned byte-check command; all eight pass, including
real secondary approved/verified application and primary denial. No product or
verification gate is weakened. All 23 targets are reused only on identical
current inputs; Clippy -Dwarnings, fmt/build/typecheck and npm260/3 skips pass.
Actual published-CLI RED/SAME-fixture GREEN preserve legacy absent/false CLI/MCP
RAW and whole SQL before new bindings; original record values remain exact after
new sessions. True controls deny edits, keep reads/secondary grants, survive mode/
project switches and resume the same true binding from YAML. Upgrade/downgrade
binding changes refuse. See [the contract](../native/docs/session-config.md) and
[indexed evidence](validation/integration-2026-10-22/session-primary-readonly/README.md).
Original primary source/Git/host/YAML bytes stay exact. Inventory assessments,
OS1314 fixtures and system settings are unchanged. Broader configuration/client/
backend, R1-R9/353/Windows/main/release acceptance remains open.

Explicit host-selected YAML files now project into a reviewable native session
profile. Original byte hashes bind global/project/project-local/custom-context/
mode inputs. Supported keys use existing layer semantics; named registered
projects can only gain read-only restrictions. YAML cannot supply registrations,
runtime trust, execution commands or permissions. Strict parsing refuses
aliases/tags/merges/duplicate keys, multiple documents, unsupported settings/
templates and exceeded budgets. Plan opens no Store; export creates a fresh file.

At source 01af6109, 136 Rust functions pass across 17 scoped targets. The 12
new functions pass first functional attempts. After two Clippy-only test-helper
borrow fixes, four CLI functions pass again on 1033 current frozen inputs;
132 functions from 16 targets are reused on 1032 identical common inputs,
excluding only that standalone helper, with compiled dependency attestations.
All-target Clippy -Dwarnings, fmt/build/typecheck and npm260/3 existing skips pass.
Actual published-CLI RED establishes JSON 53/37-tool positives before the missing
import. The SAME original fixture GREEN preserves full legacy/restricted CLI/MCP
RAW and logical SQL; the YAML profile resumes the original restricted binding
with exact replies. Source/Git/selected files stay exact. Existing output, pins,
grants and exact/one-byte-short reply budgets are qualified.
See [the contract](../native/docs/session-config.md) and
[indexed evidence](validation/integration-2026-10-22/session-config-yaml/README.md).
Automatic discovery, broader YAML/Jinja/project/client/backend and full R1-R9/
353/Windows/main/release remain open. Original inventory assessments, OS1314
fixtures and all system settings are unchanged.

Explicit native Google HTTP export now transports a selected shortcut resource
key in its private Drive header. A host-only token/private stdin pipe and all
six CLI grants precede one fixed-endpoint HTTPS GET. Native image, original
shortcut/file ID, MIME, literal address, archive/reply budget and fresh output
are admitted before credential input. Source/image pins are rechecked before
create-only publication. No DNS/proxy/redirect/retry, ambient auth, project
execution or model call is added. The gws 0.22.5 path remains unchanged and still
refuses resource keys rather than discarding them.

At source e546114a, 66 distinct Rust functions pass across 12 scoped targets
(0 existing ignores), including 17 new HTTP, Windows-pipe and CLI functions,
on 1028 unchanged input hashes. Clippy -Dwarnings, fmt/build/typecheck and npm
260/3 existing skips pass. Controlled real wire tests qualify private headers,
MIME/body limits, redirects/status, reflected secrets, deadlines and cancellation.
An owned HTTPS attempt rejects plaintext without sending private HTTP headers.
Published-CLI RED and the SAME original fixture GREEN preserve legacy received
archive replay bytes, gws refusal, source/Git and original configuration. Only
the host native-image pin is explicitly selected for the new preview.
See [the contract](../native/docs/google-workspace.md) and
[indexed evidence](validation/integration-2026-10-22/google-http-export/README.md).
No real token or Google document was accessed. Successful authenticated Google
TLS/export remains unvalidated because no approved test document is available.
Full R1-R9/353/Windows/main/release remains open. Original OS1314 fixtures and all
system settings remain unchanged.

Explicit host optionalDefaults:serena_optional_v1 selects a versioned native
projection of documented optional operations: 29 classical tools and 42 of the
50 declared native APIs. Without that selection the preceding 53/52/6 tool views,
50 APIs, serialized profiles, fingerprints and raw results stay exact. Inclusion
and fixed-base settings can enable optional entries. The six REPL controls keep
their separate tool rules while APIs follow the selected defaults and layers.
No interpreter, trust, source application or approval is granted by selection.

At source 17304136, 227 distinct Rust functions pass across 36 scoped targets
(8 new first-attempt passes, 3 existing ignores) on 1024 unchanged input
hashes. Ten focused targets supply 78 functions without rerun on
identical inputs; 26 other targets, Clippy -Dwarnings, fmt/build/typecheck and
npm 260/3 existing skips pass. Actual published-CLI RED and the SAME original
CLI/MCP fixture GREEN preserve full legacy 53-tool/50-API raw output and logical
SQL. Explicit defaults return exact original schema subsets; real Python sees
42 methods, 43 after a single inclusion, and 50 after restoring all optional
APIs. Fixed bases, included classic REPL, missing/untrusted runtime, invalid
versions and upgrade/downgrade refusal are qualified. Old records/source/Git
remain exact. See [the contract](../native/docs/session-steering.md) and
[indexed evidence](validation/integration-2026-10-22/session-optional-defaults/README.md).
The public flag snapshot is the 2026-10-22 documentation, not optional metadata
from the pinned 4f revision. Its 53 tool names match the original inventory; the
14 additional current IDE methods are outside this native 50-method projection.
Backend/client/YAML/Jinja and full R1-R9/353/Windows/main/release acceptance remain
open. Original OS1314 fixtures and all system settings remain unchanged.

Explicit includedOptionalTools and includedApis now resolve through all six host
steering layers. Within each layer inclusions follow exclusions; later layers
may restrict again. Tool inclusions extend an earlier fixed base and use the
original Trail/Graft operation identities. Individual API inclusions can make
exceptions inside an excluded facade group. Ordered inclusion provenance is
exposed with the effective restrictions. Fixed/incremental conflicts, invalid
selectors and oversized settings refuse before publication. Empty fields retain
the preceding bindings, raw results and 53/52/6 interface views.

At source e0211bd6, 219 distinct Rust functions pass across 35 scoped targets
(8 new functions, 3 existing ignores) on 1023 unchanged input hashes. Nine
focused targets supply 70 functions without rerun on identical inputs;
26 other targets, Clippy -Dwarnings, fmt/build/typecheck and npm 260/3 existing
skips pass. The eight new functions pass on their first attempt. Actual published
CLI RED and the SAME original source/profile/requests/session GREEN retain full
legacy and restricted MCP/CLI raw output, schemas and logical SQL. Tool-only,
API-only and combined inclusions work independently in real MCP/Python, while
old bindings, invalid selectors and conflicts refuse. Old records/source/Git
remain exact. See [the contract](../native/docs/session-steering.md) and
[indexed evidence](validation/integration-2026-10-22/session-inclusions/README.md).
Optional default subsets, full upstream configuration/client/R1-R9/353/Windows/
main/release acceptance remain open. Ten OS1314 fixtures and all system settings
remain unchanged; inclusion grants no runtime trust or source application right.

Optional host agentInterface now fixes tools or repl at session startup. Global,
CLI and startup project/project-local settings resolve with exact provenance;
later activation or mode changes retain that selection. The tools view filters
the original 52 non-REPL schemas normally. The REPL view exposes the original six
no-facade controls, independent of tool steering, while retaining fixed-startup
admission and separately enforced API, source and trusted-runtime permissions.
Without a selection, the preceding mixed 53-schema interface remains exact.

At source 0cbeb5d1, 211 distinct Rust functions pass across 34 scoped targets
(8 new functions, 3 existing ignores) on 1022 unchanged input hashes. Eight
focused targets supply 62 functions without rerun on identical inputs;
26 other targets, all-target Clippy -Dwarnings, fmt/build/typecheck and npm 260/3
existing skips pass. Actual published-CLI RED and the SAME original CLI/MCP
fixture GREEN retain legacy raw output/full SQL and qualify exact 52/6 subsets,
real explicitly trusted Python/source reads, API exclusion, missing runtime and
untrusted refusal, invalid inputs and old host-binding refusal. Old records and
source/Git bytes remain exact. See [the contract](../native/docs/session-steering.md)
and [indexed evidence](validation/integration-2026-10-22/session-agent-interface/README.md).
Optional inclusion/default equivalence, wider configuration/client/R1-R9/353/
Windows/main/release acceptance remain open. Ten OS1314 fixtures and all system
settings remain unchanged; this is a finite native interface checkpoint.

Optional host fixedTools now selects a nonempty MCP tool base through the six
steering layers, with exact fixedToolsSource provenance and cumulative exclusions.
Same-layer exclusion conflicts and unknown/duplicate selectors refuse. Original
Trail/Graft operation identities cover both prefixes and legacy aliases. Current
persisted state is checked before session-backed tool discovery/dispatch and
direct adapters; stale parallel sessions return an error rather than an empty
successful catalog. Empty settings preserve preceding bindings and raw results.
Tool selection grants no source write, approval, execution or Python API authority.

At source 10586eba, 203 distinct Rust functions pass across 33 scoped targets
(3 existing ignores), including 8 new functions and broader Trail/Graphify
adapter/CLI/review/body/query checks. Eight focused targets supply 62 functions
on identical 1021 input hashes; 25 other targets, Clippy -Dwarnings, fmt/build/
typecheck and npm 260/3 existing skips pass. Actual published-CLI RED and the
SAME original CLI/MCP fixture GREEN retain all 53 schemas and raw results without
the setting; the new base returns the exact 2-schema subset and enforces calls.
Original records/source/Git and complete legacy logical SQL remain exact. See
[the contract](../native/docs/session-steering.md) and
[indexed evidence](validation/integration-2026-10-22/session-fixed-tools/README.md).
Interface/optional inclusion and wider configuration/client/R1-R9/353/Windows/
main/release acceptance remain open. Ten OS1314 fixtures and system settings stay
unchanged; this is a finite native checkpoint.

Host-owned readOnlyMemoryPatterns and ignoredMemoryPatterns now apply bounded
regular expressions to named application notes through all six steering layers.
Canonical global/name matching stays distinct from project names; the original
notesPolicy glob fields retain their exact semantics and serialization. Listing,
read/write/edit/delete/rename, reference updates and real cached Python callbacks
share the effective rules. Invalid/oversized and combined mode policies refuse
before publication/CAS; external stale state closes the owned worker before its
cell runs. Restrictions grant no source, note-write or execution authority.

At source e3c65663, 166 distinct Rust functions pass across 26 scoped targets
(9 new functions, 3 existing ignores) on 1020 unchanged input hashes. Only a redundant return binding changed in the new test helper after the first
full Rust guard: its 9 functions pass again. The other 25 targets supply 157
functions on 1019 identical relevant inputs; the separate test file is excluded
with exact before/after hashes. Current 1020-input integrity,
all-target Clippy -Dwarnings, fmt/build/typecheck and npm 260/3 existing skips pass.
Actual published-CLI RED and the SAME original source/profile/notes/session GREEN
qualify filtering, read-only admission, real MCP, old note/revision/anchor/head
values and exact original raw output/full logical Store controls. See
[the contract](../native/docs/session-steering.md) and
[indexed evidence](validation/integration-2026-10-22/session-memory-patterns/README.md).
Rust regex syntax, explicit host settings and native named-note storage are the
bounded contract; Python regex/file-discovery equivalence, other settings and
full 353/R1-R9/Windows/main/release acceptance remain open. Original ten OS1314
fixtures and all system settings remain unchanged.

An explicit host startupProject now selects a registered project (or default)
before initial modes, instructions or MCP catalogs are resolved. In ide,
claude-code and grok contexts, activation is hidden/refused and removing the
fixed project is refused across session, compatibility and real Python callback
paths. Stored project forgery, stale CAS and changed host selection refuse.
Other contexts start selected but retain project switching and resume their
current project. No-setting host/session identities and actual old CLI output
remain exact. Selection adds no read, write, execution or project trust grant.

At source c47813b1, 142 distinct Rust functions pass across 24 scoped targets
(8 new startup functions,3 existing ignores), with 1019 unchanged input hashes.
The 34 focused functions are reused on identical inputs; 20 other targets,
all-target Clippy -Dwarnings, fmt/build/typecheck and npm 260/3 existing skips pass.
Actual published-CLI RED and the SAME original source/profile/requests/session
GREEN qualify startup admission, concrete-project MCP catalog/dispatch, primary
selection and restart. Old session rows/source/Git and registered Unicode target
bytes remain exact. See [the contract](../native/docs/session-steering.md)
and [indexed evidence](validation/integration-2026-10-21/session-startup-project/README.md).
This qualifies explicit native startup selection, not YAML/client equivalence,
all 25 settings, the 353 inventory or full R1-R9/Windows/main/release acceptance.
Ten original OS1314 prerequisites remain unchanged; no system settings changed.

Native host-owned context/mode steering now also selects REPL API groups or
individual methods through explicit excludedApis settings. Per-cell discovery
and every host callback enforce the current policy, including cached callables,
private declaration/context bridges and in-cell mode changes. Ordinary trusted
tool adapters retain their distinct origin. Empty API settings retain preceding
serialized identities, fingerprints and actual CLI results. Steering grants no
Python, shell, debugger, source-write or operating-system sandbox authority.

At source8d80a94f, 134 distinct Rust functions pass across23 scoped targets
(8 new functions,3 existing ignores), with all1018 input hashes unchanged.
The38 passed functions from the focused guard are reused on identical inputs;
20 other targets, all-target Clippy -Dwarnings, fmt/build/typecheck and npm260/
3existing skips pass. Actual published-CLI RED and the SAME original fixture
GREEN qualify API selection, host refusal, namespace survival and exact legacy
raw output/source/Git/Store bytes. See [the contract](../native/docs/session-steering.md)
and [indexed evidence](validation/integration-2026-10-21/repl-api-selection/README.md).

Windows worker startup now classifies an explicit executable with GetBinaryTypeW
before CreateProcess; non-executable .exe data refuses promptly. Pure host
validation remains non-executing, managed .cmd/.bat paths remain supported,
and all18 unchanged HTTP tests pass in one normal Cargo invocation. Historical
10060/timeouts and later successful pre-fix Cargo diagnostics are retained;
this qualifies the defensive invalid-image path, not a general OS root cause.
No Developer Mode, elevation, system setting or original symlink fixture changed.
The preceding [context/mode checkpoint](validation/integration-2026-10-21/session-steering/README.md)
keeps its original126-function/source51772d7a results and failed attempts.
All353 ledger identities/assessments, broader configuration/client/R1-R9 work,
ten original OS1314 prerequisites and full Windows/main/release acceptance stay open.

Exact override/implementation contract associations now extend the compiler
incoming walk without inventing generic instances or dispatch. Separate outgoing
contract proofs preserve original IDs/directions and parallel bindings; unchanged
supertypes are marked originalSeed:false. Correct zero/one-cost distance
relaxation retains shorter states and longer alternative proofs. Depth0/boundary
and combined proof/visit/output limits remain explicit.

An actual published-CLI RED misses the caller of an uninstantiated generic
override. GREEN on the SAME original report reaches its exact changed-method
row while preserving every original compiler hierarchy value, row/seed/source
binding, worker/profile and prior proof field.26 query functions plus51 project/
Git functions pass (77 distinct,0 ignores), with Clippy/fmt/build/typecheck and
npm260/3existing skips on IDENTICAL1014 inputs. Passed26-test evidence is reused,
not rerun. Exact LF/refusal/state/legacy/compiler/constructor controls pass;
Jobs end empty. See [the contract](../native/docs/git-compiler-affected.md) and
[indexed evidence](validation/integration-2026-10-21/git-compiler-contract-expansions/README.md).
These are conservative source-contract associations, not changed supertype
declarations or actual execution. Broader R5/R1-R9/inventory/Windows/upstream/
main/release acceptance and the historical ten OS1314 prerequisites stay open.

Compiler-bound property references now retain separate read/write/delete
modes, applicable getter/setter effects and direct function-initializer source
ownership. Resolved invocation signatures, original slots, candidate classes and
actual generic witnesses remain separate. Optional-chain reads and logical
conditional writes keep different metadata; unique symbols use compiler slot
identity. Returned/assigned function values and implicit effects are not guessed.

The current frozen guard passes73 Rust functions (22 query,51 project/Git),
all-target Clippy, format, build, typecheck and npm260/3 existing skips on1014
exact inputs. Actual CLI/provider comparison preserves every old node/fact/type
witness on the same checked capture and adds parallel property/effect roles.
Forged pair/mode/witness/initializer proofs refuse. Exact LF bounds/refusals,
old affected/compiler bytes, inherited-constructor facts and owned source/Git/
Store bytes pass; Jobs end empty. The initial18-pass/2-fail development phase
and unique-symbol API counterexample remain archived. See
[the contract](../native/docs/git-compiler-affected.md) and
[indexed evidence](validation/integration-2026-10-21/git-compiler-properties/README.md).
Wider implicit/member/configuration effects and full R5/R1-R9/Windows/upstream/
main/release acceptance remain open. Earlier counts retain their own source
checkpoints; the historical whole-native baseline keeps ten OS1314 prerequisites.

Generic method targets now use actual source-witnessed compiler instance types.
Per-context witnesses retain class identity, original UTF-8 anchors and actual
type arguments/flags; union branches and complete intersections stay distinct.
Alias/merged-interface/JSDoc/phantom behavior follows the checker. Explicit any
and symbolic arguments remain visible; unobserved families never receive guessed
wildcards. Native checks witness source/owner/indices/budgets and exact original
invocation pairing. Existing declared/constructor facts and graphs stay separate.

The current guard passes 68 Rust functions (17 query, 51 project/Git), all-target
Clippy, format, build, typecheck and npm 260/3 existing skips on 1013 exact listed
repository inputs. The actual CLI repeats across fresh workers, retains parallel
typed proofs and rejects incompatible arguments. The old 7ed9 provider runs on the
SAME checked captured sources with passed diagnostics and 0 generic targets;
the new query adds witnessed targets. Exact LF-envelope/refusals, old affected/
compiler bytes, original inherited-constructor facts, owned source/Git/Store
bytes and empty Jobs pass. See [the contract](../native/docs/git-compiler-affected.md)
and [indexed evidence](validation/integration-2026-10-21/git-compiler-generic-instances/README.md).
Earlier counts retain their own source checkpoints. Unobserved instantiations,
wider members/accessors/fields/implicit effects and full R5/R1-R9/Windows/upstream/
main/release acceptance remain open; the whole-native 98ffa78 baseline stays
historical with unchanged ten original symlink prerequisites.

Constructor candidate classes now have a separate `possibleConstructValue`
relation in addition to their real constructor-body bindings. A field-only
Derived change therefore reaches a dynamic constructor caller even when the body
is inherited from Base. Exact paired invocation proof, original owner/target
identity and parallel sites remain distinct; this is candidate impact and does
not establish initializer execution or general field-effects completeness.

The current guard passes 64 Rust functions (13 query, 51 project/Git), all-target
Clippy, format, native build, typecheck and npm 260/3 existing skips on 1013 exact
listed repository inputs. An actual old-CLI RED exposes the missing caller path.
GREEN reads the same original report/Store: all original rows, seeds, graphs,
bindings, context IDs, nodes and prior relation facts/multiplicity remain equal;
the new class-value proof connects the original class seed to the caller. Repeated
actual stdout and all owned source/Git/Store bytes remain exact, and process Jobs
end empty. Existing invocation/legacy compiler byte and envelope controls pass.
See [the contract](../native/docs/git-compiler-affected.md) and
[indexed evidence](validation/integration-2026-10-21/git-compiler-constructor-values/README.md).
Earlier counts below remain their own source checkpoints. Wider fields/accessors/
implicit effects, generic instances and full R5/R1-R9/Windows/upstream/main/release
acceptance remain open; the whole-native 98ffa78 baseline stays historical.

The explicit compiler/Git query now retains new/super/tagged invocation kinds,
constructor overload-to-body bindings and separate static construct-result class
roles. Inherited constructor declarations, implicit source-less signatures and
union result classes stay distinct. Finite constructor-value candidates and
instance/static method candidates use actual checker assignability; original
signature, candidate owner and exact invocation proof remain separate. Literal
class references keep direct bindings. Native admission pairs original source,
target, range, kind and conditional flags before incoming traversal.

The current guard passes 63 Rust functions (12 compiler-query, 51 project/Git),
all-target Clippy, format, native build, typecheck and npm 260/3 existing skips on
1013 unchanged inputs. An initial 11-pass/1-fail union counterexample exposed a
lost candidate role; the fix removes pruning and retains the original assertion.
A later overload oracle is strengthened to keep separate Base/Derived owners
sharing one body, with exact original sites per owner and no production change.
Actual isolated CLI repetition, constructor/static/tag roles, parallel proofs,
exact LF envelope/refusals, all owned source/Git/Store bytes and empty process
Jobs pass. Both old captured-affected and original compiler stdout/artifact stay
exact. See [the contract](../native/docs/git-compiler-affected.md) and
[indexed evidence](validation/integration-2026-10-21/git-compiler-invocations/README.md).
Earlier scoped counts below retain their own pinned source checkpoints. Generic
instance instantiations, field/accessor/implicit effects, wider configurations,
ownership/schema/upstream and full Windows/R1-R9 acceptance remain open. The
whole-native 98ffa78 baseline is historical before both Explorer/compiler changes.

The explicit `git compiler-affected` read query now uses a host-trusted immutable
TypeScript 5.8.3 program for separate override, contract, resolved-call, overload
body and type-based possible-instance-target observations. Exact original UTF-8
proofs, parallel call sites, side/project identities, source-containment seed
reasons and coverage limits remain visible. Original reports/graphs and default
compiler analysis bytes stay unchanged. No analyzed module/model/Git executes or
new artifact/record is published. See the
[contract](../native/docs/git-compiler-affected.md) and
[indexed evidence](validation/integration-2026-10-21/git-compiler-affected/README.md).

Nine new and 135 existing regression functions pass (144 distinct scoped Rust
functions). The first broad phase passes 143 then stops at one new Clippy warning;
the equivalent inequality is corrected without suppression. Final coverage-only
changes repeat nine new/seven project functions, all-target Clippy, format, native
build, typecheck and npm 260/3 existing skips on 1013 unchanged inputs. An actual
isolated CLI repeats exact bytes, retains parallel proofs, passes the exact
JSON-plus-LF envelope, refuses one byte less/no trust and preserves every owned
source/Git/Store byte; process Jobs end empty. Actual legacy compiler stdout and
artifact identity stay exact. Constructors/tagged/static-target enumeration and
generic candidate instantiations remain explicit limitations, with original
capture omissions retained. Full R5/R1-R9, Windows/upstream/hosted/main/release
remain open; the complete 98ffa78 baseline predates this query and both Explorers.

The separate `explorer` command now serves a private authenticated loopback
source view. `--allow-rebuild` independently grants manual capture; request data
cannot widen the fixed root/options or grant permission. Reload reads cached
bytes, and rebuild requires the exact expected snapshot/revision. Failed builds
retain the old view; immutable captures can persist before an export failure.
An opaque source iframe retains its offline CSP. Static exports stay byte-exact.
See the [live contract](../native/docs/explorer.md#authenticated-local-live-workflow)
and [indexed evidence](validation/integration-2026-10-21/explorer-live/README.md).

The scoped guard passes31 Rust cases, including six new live cases, all-target
Clippy, format, typecheck and TypeScript260/3 existing skips. A real browser
preconnection stall is reproduced by a failing regression, fixed with a separate
short admission deadline, then rechecked. The final two access-label changes
repeat all six live cases, native rebuild, syntax/format and eleven final actual
Chrome156 images, all visually inspected. Source edits, saved-view reloads,
new/unchanged capture, stale refusal, full page reload, mobile, denied host grant,
missing capability and sandbox JSON download pass with zero JS exceptions.
Private capability/process files are excluded; all included evidence is scanned
against ten owned ephemeral capabilities. Full STATUS/R1-R9, Windows, hosted/
upstream/main/release acceptance remain open. The complete98ffa78 suite is
historical before both Explorer additions, with ten original OS1314 prerequisites.

The explicit `html-source` export now embeds checked immutable captured files
with Code/Context/Outline tabs, paged source/outline, exact observation quotes,
keyboard tabs and relationship verb chips. It preserves original directed and
parallel observations, uncertainty, coverage and omissions; source stays inert.
Complete source and escaped output have independent hard byte bounds. The default
metadata-only and document exports retain their original template and actual
legacy HTML bytes. See the [contract](../native/docs/explorer.md#explicit-captured-source-export)
and [indexed evidence](validation/integration-2026-10-21/explorer-source/README.md).

Five new plus ten existing scoped Rust tests pass, with format/all-target Clippy,
typecheck and TypeScript260/3 existing skips. After the final CSS-only adjustment,
the rebuilt native CLI repeats the five new cases and passes twelve actual
Chrome156 captures, all visually inspected, with exact Unicode byte highlights,
quote/download/graph preservation, keyboard tabs and visible desktop/mobile
selection. Three real visual rounds retain their intermediate limitations.
Broader UI/upstream/platform/hosted and full STATUS/R1-R9 acceptance remain open;
the separately authorized live workflow is described above. The complete 98ffa78 suite below predates this addition;
its ten original OS1314 prerequisites and all ignores remain unchanged.

The complete native default run at `98ffa78` attempts all 320 registered
groups, including the new community, owner-change, Claude hook and predicate
code: **2,991 passed, 10 failed and 53 explicitly ignored**.
All three examples compile. All 1002 source/control inputs, selected tools
and the existing primary CLI remain unchanged. Every failure is an original
symlink fixture with actual OS1314; there are no compile failures or additional
unexplained failures. Both actual Junction security cases pass. No test timeout,
fixture, ignore or system setting is changed. See the
[complete current-source evidence](validation/integration-2026-10-21/native-baseline-current/README.md).
This is a complete attempted default suite. Ten file-symlink prerequisites and
full Windows/R1-R9/hosted/upstream/main/release acceptance remain open. The older
bcf442b baseline and individual scoped counts below remain separately labeled.
The historical supplemental audit confirms the legacy metadata-only explorer's
header-only panel. The separate source-bearing mode above supplies the new tabs
and chips; the separate live workflow above supplies manual rebuild/reload.

The optional TypeScript 5.8.3 classic body route now preserves explicit/inferred
predicate and assertion caller flow with an inert original signature and fixed
computed-result relay. Pure generated caller references require one captured
ordinary lexical binding; separate exports/aliases are declaration-bound.
Calls/getters are never replayed. Known dense indices retain element types under
noUncheckedIndexedAccess, including genuine undefined. Complete surviving
compiler observations and independent write gates remain.

The scoped guard passes146 Rust functions/2 existing ignored across ten new-target
integrations, nineteen body units and117 regression integrations. Format/all-target
Clippy,1002 byte-exact primary inputs, fresh primary library/CLI build, typecheck
and TypeScript260/3 existing skips pass. Each actual binary passes four owned CLI
controls with independent tsc, verification, approval, application, foreign-byte
refusal/conditional recovery and equal literal runtime observations. All process
jobs end empty; preparation executes no analyzed source. Failed causal phases
and diagnostic-only probes stay separate. See the
[predicate evidence](validation/integration-2026-10-21/function-body-predicates/README.md)
and [contract](../native/docs/compiler-inline-predicates.md).
JSDoc predicates, generic/this/wider forms, full R1-R9 and current-head whole-native/
hosted/platform/upstream/main/release acceptance stay open. The full bcf442b
baseline and its ten OS1314 failures remain historical and explicit.

The optional [Claude SessionStart settings](../native/docs/claude-session-hooks.md)
profile now owns one external JSON file through the guarded installer. It does
not adopt existing files or edit foreign hook arrays. Literal executable/args,
original physical bindings and current unchanged receipts govern the bounded
read-only callback. Hook input never grants authority or selects transcripts;
the advisory explicitly marks snapshot freshness as not checked. No source Store,
model, Git, permission decision or initial user message is created.

The scoped guard passes206 Rust cases/1 explicit child-worker ignore, including
six new integrations and three new actual process-interruption functions. Format,
all-target Clippy, fresh primary library/CLI build, typecheck and TypeScript260/3
existing skips pass; every validated input matches primary byte-exactly. Both
actual Claude2.1.306 init-only fixtures receive the native context and observe a
static refusal with no context after a policy change. Parent exit0 alone is not
accepted. Callback source/settings/journal bytes remain unchanged, process jobs
end empty and zero provider requests are observed; no login/conversation occurs.
See the [exact evidence](validation/integration-2026-10-21/claude-session-hooks/README.md).
Interactive/model context use, other versions/runtimes, statuslines and complete
R6/R1-R9/platform/upstream/hosted/main/release acceptance remain open. The full
bcf442b native baseline predates this addition and remains separately labeled.

The separate [captured owner-change query](../native/docs/git-owner-changes.md)
now reports authored candidate changes for unchanged files when only CODEOWNERS
changes. It compares the full saved path union, verifies captured bodies, and
retains original parent, rules, duplicate tokens and Unicode/CRLF byte evidence.
Missing/unsupported rules stay indeterminate; candidate changes, moved winning
evidence, added/removed paths and source hashes remain distinct. It grants no
authenticated ownership, permission or actual review authority. The old owners
query preserves exact raw result identities.

Six new integration functions and 65 existing Git regressions pass (71 Rust
cases), with format/all-target Clippy. All 993 validated inputs match primary
byte-exactly; its library/CLI freshly build and format/typecheck/TypeScript260/3
existing skips pass. Both actual binaries preserve old owner bytes, repeat the
same new result, enforce exact newline budgets and leave every owned source,
Store and Git file unchanged. No Git/network/model or analyzed-source execution
occurs in these query controls. See the [indexed evidence](validation/integration-2026-10-21/captured-owner-changes/README.md).
The new mode has no complete native-suite or full R5/R1-R9/upstream/hosted/
platform/main/release acceptance; the earlier complete baseline stays separate.

An optional [community projection policy](../native/docs/community-policy.md)
now adds unique-neighbor hub degree/voting, stranded-node holdout and bounded
oversized/sparse split passes. Canonical and selected-View producers pin this
explicit mode. Source-order ties avoid snapshot-ID churn; original directed and
parallel observations, resolution labels, source bytes and evidence stay intact.
The old default and source-category-only v1 reports preserve exact raw CLI and
artifact/Markdown identities. No successful split or hard final size is claimed.

Six numerical units and 97 integrations pass with format/all-target Clippy.
The integrated primary checkout repeats all twelve new cases (overlapping),
format/Clippy, typecheck and TypeScript260/3 existing skips. Both actual compiled
CLI controls preserve old report families, repeat new mode bytes with/without
source categories and pass saved-root retention plans. All scoped inputs stay
frozen during checks; documentation follows. No model/analyzed-source execution
occurs in these controls. See the [indexed evidence](validation/integration-2026-10-21/community-policy/README.md).
The complete bcf442b baseline below predates the new mode; a new-head whole-native
run, full R3/R1-R9/upstream/hosted/platform/main/release acceptance remain open.

The complete native default recheck at `bcf442b` attempts all 316 registered
groups: **2,951 passed, 10 failed and 53 explicitly ignored**. Examples compile,
and all 941 inputs, tools and the CLI remain unchanged. Every failure is an
original Windows symlink fixture with OS1314; no compile failures remain. The
digest-history group now passes normally without an alternate output flag.
Actual Junction import/export and directory-move security cases also pass. The
user requested progress without enabling Developer Mode; no system setting is
changed. File-symlink acceptance remains open and is not replaced by Junctions.
See the [complete baseline evidence](validation/integration-2026-10-21/native-baseline-floats/README.md).
This is not complete Windows/R1-R9, hosted, main or release acceptance.

Canonical source-category reports now participate in retention through exact
read-only reconstruction of their saved sources, options and historical analysis.
Rehashed changes to scores/audit/policy/future fields and signed zero refuse;
report/Markdown and both historical source roots remain protected. The original
legacy shape checks remain. The already pinned JSON parser now uses exact float
roundtrips, fixing one-bit changes that could invalidate legitimate persisted
reports. Dependency versions and the lockfile stay unchanged.

The scoped guard passes 101 Rust tests across fifteen targets, format, all-target
Clippy, typecheck and TypeScript 260/3 existing skips. An actual same-snapshot CLI
control preserves both legacy and source-category v1 raw stdout/report/Markdown
identities, and actual retention/purge plans recognize its saved dependencies.
No deletion, model or analyzed-source execution occurs in this control. Failed
development phases and the stronger signed-zero counterexample remain archived.
The complete native recheck records the shared parser feature; broader R3
community policy, R1-R9/platform/hosted/upstream acceptance remain open. See the
[exact replay evidence](validation/integration-2026-10-21/policy-retention-floats/README.md).

Explicit [JSON MCP host profiles](native-json-hosts.md) now manage receipt-owned
Cursor `mcp.json`, Claude Code external `.mcp.json` and Claude Desktop config
entries through the guarded Windows installer. Strict unique-key JSON patches
create missing parents/files and preserve foreign values, Unicode/BOM/CRLF and
human formatting. Exact profile/source/launch bindings, independent write gates,
no-adoption rules and v1/v2 recovery remain mandatory. Client interpolation paths
are refused; generic/Codex serialized identities stay unchanged.

The final scoped guard passes 175 Rust tests with one explicit child worker
ignored, format, all-target Clippy, typecheck and TypeScript 260/3 existing skips.
Three new process-interruption functions exercise each JSON profile: partial
exit/resume/uninstall, prepared forced kill/rollback of the original file object,
and replaced-source refusals without state changes. Actual Claude Code 2.1.306
loads the managed file in an isolated test directory and reports Connected;
private import of the exact generated entry also connects. No login or model turn
is requested. All process jobs in these connected live gates are verified empty; config and
source bytes roundtrip unchanged. Failed development phases remain archived.

Claude status subcommands ignore global `--mcp-config`; the passing file gate
loads `.mcp.json` from the owned client working directory with only its test
server approved in private settings. Interactive explicit-file loading, model
selection/tool dispatch, Cursor/Desktop runtimes, hooks/statuslines and broader
R1-R9/Windows/current-head hosted/upstream qualification remain open. See the
[exact evidence](validation/integration-2026-10-21/json-mcp-hosts/README.md).

An optional [source-category analysis policy](../native/docs/source-category-policy.md)
now supplies explicit resolution/category mappings, relation projection weights,
ambiguity-only scaling, source-aware hub filters and explained runtime-family/
source-kind surprise scores. Unmapped categories remain visible. Original graph
observations, directions, parallel evidence and source bindings stay unchanged.
The default policy serializes exactly as before; both the existing fixed golden
and an actual pre/post CLI control preserve report/Markdown artifacts and raw JSON.
Policy-specific canonical and selected-View producers bind the exact configuration;
replaying altered stored policy metadata refuses.

The scoped run passes 55 Rust tests across seven targets, including five new
policy tests, plus format, all-target Clippy, retained typecheck and TypeScript
260/3 existing skips. Failed development lint phases remain separate; the final
fixture-only lint correction has an exact input comparison and passing recheck.
Native Leiden/splitting/reattachment differences and wider upstream analysis
qualification remain open; no identical upstream partition/ranking is claimed.
See [source-category evidence](validation/integration-2026-10-21/source-category-policy/README.md).

The complete native baseline at `6951ff9` attempted all 310 registered groups:
2,904 passed, ten failed and 53 remained ignored. All ten runtime failures occur
while creating Windows symlink fixtures (OS1314). `digest_history_jobs` additionally
had a held linker output; its unchanged normal test-profile harness passes all
thirteen tests under a separate output filename. The original held file/processes
remain untouched. All 927 inputs, tools and the baseline CLI stayed unchanged;
example compilation passes. This is not complete Windows acceptance.

The separate explicit [Google Workspace route](../native/docs/google-workspace.md)
now binds original shortcuts, host-selected IDs and trusted gws execution to
create-only immutable export archives. Storeless byte replay and separate bounded
XLSX inspection retain received-body/XML coordinates. Host-only source, network,
execution, client-state and output grants precede effects; ambient model/log/token
settings and unrelated gcloud ADC fallback cannot select authority. Ten units,
three CLI cases, one multi-case owned-client test and nineteen existing source
acquisition regressions pass. Format/all-target Clippy pass; the final fixture
lint correction has a separate passing process/format/Clippy recheck. Typecheck
and TypeScript 260/3 existing skips pass. The actual pinned gws client also refuses
empty selected state with blocked ADC fallback and creates no export.

Resource-key transport and authenticated Google export remain open, as do other
R1-R9 work, Windows symlink acceptance and current-head hosted qualification.
These results do not establish full upstream equivalence or a release.
See the [baseline and Google evidence](validation/integration-2026-10-21/google-workspace/README.md).

Attached caller JSDoc now stays with its original lexical declaration after the
inserted statements. Native derives the widened edit extent from original syntax;
there is no provider range override. Every removed definition leaves an empty
statement to stop trailing comments from acquiring a new annotation owner.
Comments/trivia move once in the same scope,
retaining typedef identity, exports, UTF-8/CRLF bytes and surviving types. Copied
callee function tags form one contract comment so template, parameter and return
tags keep their common binder. The original caller @type{3}/generic-factory
counterexample and a nested typedef/template counterexample now pass.

One new integration function covers thirteen positive cases. Two new units cover
original attachment/budget rules and coherent forged comment extents/types/output;
two additional CLI cases pass independent tsc, approval, apply and conditional
foreign-preserving recovery (seventeen CLI scenarios total). The scoped regression
passes 117 integrations / 0 failures / 2 existing ignored across eight targets and
sixteen body units on 811 unchanged native inputs; fmt/build/all-target Clippy pass.
Python 17, typecheck and TypeScript 260 / 3 existing skips pass on 805 unchanged
retained inputs. Counts overlap. Preparation executes no analyzed source.

Broader generic/expected-result contexts, actual TypeScript generic frames,
predicate/parameter/annotation/diagnostic and wider invocation/expression/
declaration/language/consumer semantics, other R1-R9 work and complete Windows/
current-head hosted/upstream acceptance remain open. See the
[caller-documentation evidence](validation/integration-2026-10-19/function-body-caller-docs/README.md).
The following argument-context results belong to the preceding source checkpoint.

Context-sensitive arguments now keep original callable typing through a fixed
pure rest collector. The original argument list executes once before parameter
defaults, rest/pattern initialization and the original body. Fresh raw arrays are
read only at their own dense indices, preserving behavior when Array iterators
are replaced. Inert original/signature carriers provide source-derived types;
original JSDoc parameter/template pieces have bounded native byte checks and
compiler free-binding checks. Simple literal/identifier arguments retain direct
fresh bindings and existing generic scalar inference. Both finite recipes have
independent native reconstruction and unchanged caller/argument type comparisons.
Preparation executes no analyzed source. Actual parameter/body environments,
finally behavior and ordinary verification/approval/application/recovery remain.

One added integration function covers twenty owned positive cases: callback,
property, tuple, rest and pattern contexts; generic function-value literal types;
anonymous arrow/function/class names; never-typed argument values; default order;
hostile iterators and scalar/var compatibility. Three additional CLI cases pass
independent tsc, approval, application and foreign-preserving recovery. Two added
units and extended actual-host tests reject forged documentation/source ranges,
UTF-8 boundaries, names, recipes, raw destructuring and extra executable code.

The scoped regression passes **116 integrations / 0 failures / 2 existing ignored**
across eight targets and fourteen body units on 811 unchanged native inputs.
Format/build/all-target Clippy pass. Python 17, typecheck and TypeScript 260 / 3
existing skips pass on 805 unchanged retained inputs. Counts overlap. Failed
development evidence retains a stricter old var assertion over an inert clone,
exact owned native/runtime controls, and generic scalar widening (3 to number).
The corrected direct scalar recipe preserves the old var test without weakening
it. Synthetic worker/signature probes are distinct from native admission.

Caller JSDoc annotation ownership, mixed generic and expected-result contexts,
TypeScript generic frames, other
parameter/annotation and broader R1-R9 work, complete Windows and current-head
hosted/upstream acceptance remain open. See the
[argument evidence](validation/integration-2026-10-19/function-body-argument-contexts/README.md).
The following return-context results belong to the preceding source checkpoint.

The return-context extension preserves original TypeScript return annotations as
contextual types for generated result storage, with a fixed undefined alternative
for normal completion. Function-valued properties, methods, tuples and callbacks
retain their original parameter types. Source identifiers keep the same binding
checks; consumer type shadows refuse. Source-derived void-comma barriers prevent anonymous
returned arrows, function expressions and classes from acquiring synthetic result
variable names. Original named expressions, evaluation order and finally behavior
remain intact. Native reconstruction checks the exact annotation and barrier.

Thirty-one new owned positive scenarios and one type-shadow refusal pass in two added
integration functions, plus nineteen source-wrapper classification cases in one new unit, with
Unicode/CRLF, retained exports and before-definition
calls. Three additional CLI cases pass independent tsc, verification, approval,
application and foreign-preserving recovery. Coherently forged annotations and
removed name barriers refuse despite matching forged result hashes.

The scoped regression passes **115 integrations / 0 failures / 2 existing ignored**
across eight targets and twelve body units on 809 unchanged native inputs.
Format/build/all-target Clippy pass. Python 17, typecheck and TypeScript 260 / 3
existing skips pass on 803 unchanged retained inputs. Counts overlap. The original
owned CLI rejection and the sixteen-case worker red/green comparison are retained:
eight original projected compiler errors and seven runtime name changes become
sixteen exact type/runtime matches. Synthetic probes claim no native admission;
runtime comparisons execute only owned literal fixtures.

Further predicate, argument/parameter/annotation and broader R1-R9 work, full
Windows and current-head hosted/upstream acceptance remain open. See the
[return-context evidence](validation/integration-2026-10-19/function-body-return-contexts/README.md).
The initial 114-pass/one-failure guard caught TS7027 from an overly broad comma
wrapper. The final source classifies anonymous values through erased wrappers and
keeps ordinary never-call flow; four never cases and the 31-case worker probe pass.
The following completion/type results belong to the preceding source checkpoint.

Classic function-body expansion under the optional `inlineCompiler` profile now
retains enabled unreachable-code and unused-label diagnostics. The original body
block runs under the fixed runtime-true guard `void 0 === void 0`; only actual outer
returns need an exit label. Normal completion clears a return cancelled by finally
control flow outside the original body. Throwing and never-typed return paths keep
their runtime behavior, including a declared-never call that actually returns.

Nonprimitive inferred result types use fixed `typeof` syntax over a fresh result
carrier. A separate original-parameter/body function carrier, with attached leading
JSDoc, is called only with the already evaluated argument temporaries. Literal const/contextual values use a fixed readonly wrapper and indexed typeof
query; other values retain ordinary inference. Original module type declarations
stay at their original site, with only bounded source function-tag pieces copied
when needed, preserving typedef identity. Both
initializers and the read remain under `void 0 !== void 0`; neither clone nor call
executes. This preserves JSDoc template/literal instantiation without reevaluating
arguments. Fixed empty statements protect neighboring declarations from orphaned
documentation while retaining original comment/typedef bytes. The compiler checks
free documentation type/value bindings against the consumer scope. Original/fixed annotations use a fixed intermediate
unknown assertion
for generated undefined paths. Native reconstruction checks source bytes, comment
ownership, fresh names, copied bindings and surviving caller types. Original var
storage carriers, argument/default environments, labelled returns, finally
cancellation and eval refusal retain their existing guarded contract. Preparation
executes no analyzed source or arbitrary compiler type expressions.

The final scoped run passed **113 integrations / 0 failures / 2 existing ignored**
across eight targets, plus eleven body units, on 809 unchanged native inputs.
Format/build/all-target Clippy with warnings denied pass. Python 17, typecheck and
TypeScript 260 / 3 existing skips pass on 803 fixed retained inputs. These frozen
scopes overlap; totals are not added. Only documented metadata changes follow,
with exact runtime and tracker preservation checks.

Two added integration functions cover 26 strict completion/type scenarios plus
thirteen generic/documentation positives and one explicit shadowing refusal. Two new
unit functions cover attached/trailing comments, BOM/Unicode, original bytes,
deadlines and bounds; extended admission tests reject forged guards, names and
type queries, carrier argument maps and documentation neutralization. Nine body
CLI cases include four new strict TS/JS cases with independent tsc, approval, application and foreign-preserving recovery. Runtime
comparisons execute only owned literal fixtures. Failed development phases retain
the original invalid test fixtures and the BOM regression caught by the new unit.

Further return/predicate/parameter/annotation contexts, diagnostic combinations,
wider expression/invocation/declaration/language/consumer forms and full STATUS/
R1-R9/Windows/current-head hosted/upstream acceptance remain open. Existing
OS1314/macOS failures and production App deployment remain separate. See
[the body contract](../native/docs/compiler-inline.md#classic-function-body-statements)
and [completion/type evidence](validation/integration-2026-10-19/function-body-completion/README.md).

Captured missing compiler-move imports now reconstruct original named/default/
namespace bindings and source-owned top-level free dependencies. Native checks
bind original/current definitions and references, exact relative module targets
and declaration-boundary import/export insertions before complete-result
syntax/binding/API and ordinary guarded-plan gates. Original target declarations
retain their names. Shorthand `__proto__` uses a computed own-property key;
prototype-setter substitutions have a distinct native syntax signature.

Six additional integration cases pass, with twelve imported-binding scenarios,
twelve private/public/default/aliased source-dependency scenarios, combined repairs,
BOM/CRLF/directive/path coverage and three explicit owned JavaScript property
oracles. Two existing integrations and two unit cases now cover omitted imports,
three actual CLI/independent tsc/approval/apply/conditional recovery scenarios and
coherent forged origins, module paths, source exports, references and budgets.
The final scoped native run is **659 passed / 0 failed /
9 existing ignored**, across 13 groups on 799 fixed native inputs.
Format/build/all-target Clippy pass. Development phases are not added to totals.
Python17, TypeScript typecheck and TypeScript260/3skipped pass on 793 fixed
retained inputs. The same owned no-repair Array/Promise CLI fixture preserves
exact files/diff/result snapshot/intent/bindings/context/effect digest without
adding repair metadata. Fresh timestamped plan IDs differ under the unchanged
identity formula. The earlier arXiv state-I/O failure and its same-binary passing
recheck remain archived; its underlying cause was not localized.

Remaining body-inline forms, unsupported attributes and sibling bundles, wider shapes,
runtime/upstream qualification and full Windows/current-head hosted acceptance
remain open. Existing OS1314 and macOS PDF/runtime/timing failures remain
unresolved. See [missing-import evidence](validation/integration-2026-10-19/compiler-move-missing/README.md).

Explicit [compiler move repairs](../native/docs/compiler-move.md#source-only-compiler-repairs)
now split direct named reexports and give generated imports fresh local aliases
without renaming original target/dependency declarations. Native original/current
source-anchor checks, captured free-binding checks and original-byte piece
composition precede the existing complete-result syntax/binding/API gates.
Shorthand property keys retain their independent definition anchors.

All **eight additional Rust cases pass** (six integration / two unit); two
existing integration cases now also exercise repaired outputs. Coverage includes
four import forms, three reexport styles, 128 UTF-8 composition sequences with
eight edits each, coherent forged effects, a shared operation budget and actual
CLI plus independent tsc/approval/apply/conditional recovery for both original
and repaired plans. The final scoped native run is **653 passed /
0 failed / 9 existing ignored**, across 13 groups on 798 fixed
native inputs. Format/build/all-target Clippy pass. Counts include the new cases;
development phases are not added to these totals.
Python17, TypeScript typecheck and TypeScript260/3skipped pass on 792 fixed
retained inputs. The same owned no-repair Array/Promise fixture retains its
exact files/diff/result snapshot/intent/context/effect digest; fresh timestamped
plan IDs differ under the unchanged identity formula.

At the earlier repair checkpoint, missing shorthand-only imports still refused.
The missing-import checkpoint above supersedes that limitation for captured
imports and supported source declarations. Classic body inline,
wider shapes/runtime/upstream acceptance and full Windows/current-head hosted
acceptance remain open. Existing OS1314 and macOS PDF/runtime/timing failures
remain unresolved. See [repair evidence](validation/integration-2026-10-19/compiler-move-repairs/README.md).

Explicit [immutable compiler moves](../native/docs/compiler-move.md) add actual
host-selected TypeScript 5.8.3 declaration moves into existing captured ES modules.
Native syntax/binding/import-export/result checks, explicit graph-absent AST
selection and original BOM normalization precede ordinary guarded plans.
All **16 new cases pass** (13 integration / three unit), including fourteen
analyzer/declaration scenarios, four encoding/language scenarios, two JSX
component scenarios, inert repository plugins and actual CLI plus independent
`tsc`, approval/application and conditional recovery. An additional CLI
Array/Promise standard-library move and independent tsc gate pass separately from these Rust test counts. At that earlier checkpoint, invalid compiler reexports
and import-alias collisions refused; the repair checkpoint above supersedes
that limitation for the covered shapes.

The final scoped native invocation is **645 passed / 0 failed /
9 ignored**, across 13 groups, including the library and selected
integrations. Format, build and all-target Clippy pass on 795 fixed native
inputs. Existing ignored entries include separately qualified large-capacity,
Windows path and live/parent-worker fixtures; they are not counted as passed.
Python17, typecheck and TypeScript260/3skipped pass on 789 fixed retained inputs.
This is scoped Windows development evidence, not a whole-native or other-platform
rerun.
See [compiler move evidence](validation/integration-2026-10-19/compiler-move/README.md).
Classic body inline, broader move/consumer/runtime contracts, full Windows and
current-head hosted acceptance remain open. The existing Windows symlink-privilege
gate and separately identified macOS PDF/runtime/timing failures remain unresolved.

Explicit [immutable compiler inlining](../native/docs/compiler-inline.md) adds
host-selected TypeScript 5.8.3 function-binding inline plans for TS/TSX/JS/JSX,
including sibling declarations, JSX/component bindings, object shorthand and
retained definitions. Compiler source/binding/diagnostic checks and independent
native initializer/removal/result checks precede ordinary guarded plans. Existing
other-language LSP routes and absent-profile session identities remain intact.

All **18 new cases pass** (16 integration / two unit), including twelve actual
language/sibling scenarios, component-shadowing checks, inert repository plugins,
actual CLI plus independent `tsc` verification, approval/application and conditional
recovery. The owned non-TS LSP preservation fixture is separately labeled.
The final scoped run is **629 passed / 0 failed / 9 ignored**, across 12 groups;
format/build/all-target Clippy, Python17, typecheck and
TypeScript260/3skipped pass on **788 fixed native / 783 fixed
retained inputs**. Earlier failed development/link runs remain archived, including
the disk-exhausted link that ran no tests and verified build-cache cleanup.
Classic function declarations, broader moves, other contexts/languages, runtime
equivalence and full native/current-head hosted acceptance remain open. The earlier
Windows symlink-privilege gate remains unresolved; its original test and source
reader were not weakened. See [compiler inline evidence](validation/integration-2026-10-19/compiler-inline/README.md).

Explicit [native semantic identity](../native/docs/meaning-identity.md) adds
source-replayed MinHash/Jaro candidates, one host-authorized model adjudication
job and a lossless positive-clique quotient. Original concepts, directed parallel
observations, role slots and original receipts remain unchanged. Provider-free
node/degree/trace queries consume the frozen checked result, including after Store
loss. Bound bills and unknown reservations retain their original meaning through
failure/recovery and later ledger observations. Model interpretations establish
no semantic truth or execution. Broader source/history/workspace identity,
upstream normalization/survivor rules and live semantic quality remain open.

All **22 new identity cases pass** (18 integration / four unit), with independent
MinHash vectors, actual CLI/lost-stdout recovery, unchanged original unknown costs
after both identity/source-ledger reconciliation, bounded SQL materialization,
source/receipt tampering, non-clique refusal and frozen topology queries. The
broader ten-target native run **failed: 574 passed / 1 failed / 7 ignored**. The
unchanged host-alias security fixture still fails during symlink setup, OS1314;
neither it nor the original host-file reader was weakened or skipped. Format,
build, all-target Clippy, Python17, typecheck and TypeScript260/3skipped pass on
**784 fixed native / 780 fixed retained inputs**. Existing alias normalization/Jaro
bodies remain exact, and all thirteen legacy join CLI cases pass. These are scoped
processing/recovery checks, not live semantic quality, whole-native/current hosted
acceptance or release certification. See
[identity evidence](validation/integration-2026-10-19/semantic-identity/README.md).

Explicit [semantic transcript candidates](../native/docs/semantic-transcript-candidates.md)
add one host-authorized provider attempt over selected original imported UTF-8
bytes. Durable source/payload/response/ledger/result replay supports provider-free
status, pagination and checked review. Bound bills survive domain rejection,
unknown sends keep reservations, and no job grants an automatic retry. Duplicate
candidates and ordered exact quotes remain distinct. They create no outcome events
and establish no semantic truth, independent corroboration or observed execution.
Owned protocol fixtures qualify processing and recovery; live model-quality and
broader object/platform/upstream acceptance remain open.

All **15 new transcript cases pass**, including actual CLI permission ordering,
lost-stdout recovery after committed publication and replay after provider/config
removal. The final broader nine-target run is **93 passed / 1 failed / 0 ignored**;
the unchanged Windows host-alias fixture fails at symlink setup with OS1314.
Neither its original test nor the original host-file reader was weakened or
skipped. Format/build/all-target Clippy, Python17, typecheck and
TypeScript260/3skipped pass on **775 fixed native / 772 fixed retained inputs**.
This remains a failed broader native run and pending full/hosted acceptance. See
[transcript evidence](validation/integration-2026-10-19/semantic-transcript/README.md).

The explicit [App history driver](../native/docs/app-history-integration.md) can
complete/replay a permitted worker and acquire bounded persistent Git history.
An authenticated binding protects the original cumulative plan; current grants,
selected queue revisions and exact native revisions remain mandatory. Separate
status opens an existing read-only Store. Production installation and broader
receiver reasoning remain open; this adds no default background task.

Separate [Meaning claim outcomes](../native/docs/meaning-outcome-feedback.md)
extend R9 with packet-qualified concepts, directed relations and hyperedges.
Original/evaluated View, source, overlay and Meaning receipt closures replay
before source-bound immutable events or historical ranking are admitted. Original
quotes must fit actual claim support; later evaluated quotes explicitly establish
same-support-path bytes only. All original packets, parallel directions, roles,
unknown receipt costs and per-event evaluated pins remain visible. Subject-wide
actor/session sequences prevent duplicate votes across evaluated Views. Whole
output precedes SQL publication, and retention validates every event's original
source closure. Labels/scores establish no semantic truth, current applicability,
independent corroboration or observed execution. Remaining object/platform/upstream
qualification stays open.

All **13 new Meaning-outcome cases pass**. The final broader five-target run is
**74 passed / 1 failed / 0 ignored**: the unchanged Windows host-alias security
fixture fails at `symlink_file` setup with OS1314 before an application read.
The host token lacks `SeCreateSymbolicLinkPrivilege` and Developer Mode is not
configured. Neither that test nor the existing host-file reader was weakened;
their exact unchanged bodies are verified. This is a failed broader native run,
not a whole-platform pass. Format/build/all-target Clippy, Python17, typecheck
and TypeScript260/3skipped pass. All 770 native and 768 retained inputs stay
unchanged during their final checks and still match source. See
[Meaning outcome evidence](validation/integration-2026-10-19/meaning-feedback/README.md).

The separate [receiver source interpretation](../native/docs/received-source-interpretation.md)
read binds existing explicitly built native Meaning graphs to fresh received
declaration verification. Exact local sources, canonical graph, capture/analyzer
lineage, provider response/cache origins and accounting replay independently.
Every complete submitted unit, including context and `no_claims` units, must fit
verified declaration bounds and local file hashes. All containing declaration
ordinals and the entire semantic layer remain: parallel/reverse/self relations,
ordered roles, exact quotes and original receipt/cost provenance. Extra host
permission admits quotations through an existing-only read-only CLI; no provider
is resolved, model started or derived record written. Interpretations remain
unverified inferences; broader reasoning/model quality, other packet claims,
production deployment and whole-source/remote equivalence remain open.

The final interpretation scope passes **114 Rust tests / 0 failed / 1 ignored**
across five groups, including thirteen new cases. Format/build/all-target Clippy,
Python17, typecheck and TypeScript260/3skipped pass. All 764 native and 763 retained
inputs stay unchanged during their final checks and still match the committed
source. Owned protocol fixtures qualify replay/processing, not semantic quality
or remote billing. The initial rejected fixture and test-only Clippy correction
remain in the [interpretation evidence](validation/integration-2026-10-19/received-source-interpretation/README.md).

The separate [received-declaration verification](../native/docs/received-source-verification.md)
contract independently replays an accepted receipt against a host-selected local
capture under current receiver/source grants. The read-only `digest-inbox verify`
CLI checks all advertised Rust declaration fingerprints, original ordinals,
headers, UTF-8 byte bounds and projection counters; supplied whole-file hashes
and claimed verified Git identities are also compared. It retains foreign and
local identities separately and refuses empty claims, stale pins, corruption or
capacity overflow. Matching fingerprints do not attest remote ownership, sender
snapshot identity or whole-source equivalence. Sender source-digest hashes,
discussions, history claims and semantic reasoning remain unverified. No model,
network request, project execution or persistent verification record is added.

The final verification scope passes **142 Rust tests / 0 failed / 2 ignored**
across five groups, including twelve new default cases with actual owned receiver,
Git and CLI processes. Format/build/all-target Clippy, Python17, typecheck and
TypeScript260/3skipped pass. Both native and retained checks keep 761 inputs
unchanged during execution. Two existing contract documents were reconciled
afterward without runtime/test changes; raw manifests and materialization proof
preserve that distinction. This is scoped local evidence, not a whole-native or
hosted Unix pass. See
[verification evidence](validation/integration-2026-10-19/received-source-verification/README.md).

The App-history scope passes **300 Rust tests / six default ignored cases**
across 14 target groups: 13 normal Cargo targets (287 passed) and the unchanged
13-test history-job target linked with an explicit test profile and a separate
output suffix after Windows retained a crashed linker's file lock. The additional
actual Windows CLI/Credential Manager gate passes with verified temporary-key
removal. Format/build/all-target Clippy, Python17, typecheck and TypeScript260/3skipped
pass. All 758 native inputs remain fixed across the final checks; this is scoped
evidence, not a whole-native or hosted-platform pass. See
[App-history evidence](validation/integration-2026-10-19/app-history-integration/README.md).

The hosted Ubuntu test job at `d819d71` executed **2,381 passed, 36 failed,
36 ignored** across 302 test-result groups. Every failure came from a
host-install fixture reaching its explicitly Windows-only guarded backend.
Successful installation/recovery fixtures are now scoped to Windows; Unix
tests require explicit refusal without target/state mutation or private output.
Permission-precedence checks remain cross-platform. The failed run is retained;
this fixture correction awaits fresh hosted Unix verification.

The correction passes 159 local Windows Rust tests with one existing
parent-spawned worker ignored, all-target Clippy with warnings denied,
format/build, 17 Python contracts, TypeScript typecheck and 260 TypeScript tests
with three skipped. All 41 original installation test bodies remain unchanged.
See [platform-test evidence](validation/integration-2026-10-19/host-install-platform/README.md)
for exact frozen inputs and the pending Unix execution boundary.

The [remaining-work map](REMAINING_WORK.md) reconciles current R1â€“R9 behavior,
concrete implementation gaps and separate acceptance gates. It accounts for
the completed mixed graph consumers/composition and existing discussion/digest
contracts while retaining every historical failure and inventory boundary.

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
export equivalence and the wider R1â€“R9 work remain open.

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
platform acceptance and the remaining STATUS/R1â€“R9 requirements stay open.

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
STATUS/R1â€“R9 gaps, hosted acceptance, packaging and release remain open.

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
The selected tree admits 1â€“1,000 captured text files, 2,000 directories and
16 MB counted per physical file. Multi-provider/binary trees, host-scope migration,
cross-filesystem fallback, exhaustive reference/IDE guarantees, default-safe file
usage checks, propagated deletion and the wider STATUS/R1â€“R9 work remain open.
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

Other file/declaration/language shapes, propagated deletion, broader R1â€“R9,
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
rerun is claimed. Other-language attachments, general propagation, wider R1â€“R9,
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
R1â€“R9, hosted/platform acceptance, main merge and release remain open.

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
General propagation beyond this finite policy, other R1â€“R9 work, hosted/platform
acceptance, main merge and release remain open.

[Portable Meaning projections](../native/docs/meaning-projection.md) now read
native group/join exports without a Store or provider and support inspection,
node lookup, degree and bounded directed traces. Policy/node identities,
graph/packet membership, original directed endpoints, evidence references and
every ordered hyperedge slot are structurally checked. Parallel observations,
selfloops, original quotes and repeated roles remain distinct. Imported replay,
attestation and completeness claims stay nested untrusted metadata; results
report `bindingValidation=not_replayed`, `sourceAuthority=false` and
`complete=false`. Import cannot create a checked `JoinedGraph` capability or
detect coherent omitted records/forgeries from the export alone.

All **nine new contracts** pass, including actual exports after Store/provider
removal, grouped and two-parent records, asymmetric traversal, exact independent
byte/count/output boundaries, malformed/Unicode/protocol/authority/path cases,
deadline and CLI no-initialization checks. The frozen selected run records
**480 native tests / 1 failed / 7 existing ignored gates** across six
groups. The failure is an unchanged archive security fixture that cannot create
a real Windows symlink (OS 1314); it is neither skipped nor weakened. Final
regressions use `--no-fail-fast`. The final test-only strengthening explicitly
closes the engine and asserts removal of the original source/Store/provider
directory. Runtime/library inputs remain identical to the preceding 429-test
library run; it is not claimed as a second run. These are local owned protocol
fixtures, not live-model or semantic
quality validation. Clippy, format, **17 Python tests**, mandatory typecheck and
**260 TypeScript tests / 3 existing skips** pass with
**677 integrated inputs unchanged**. The initial API RED and an
intermediate packet-selection error in the traversal test are retained with
raw logs and source hashes. [Evidence](validation/integration-2026-10-19/portable-projections/index.json)
records the finite scope. Semantic alias identity/de-dup, mixed-document spaces,
cross-history/workspace joins, outcome learning, other R1-R9 work, whole-native
acceptance, main merge and release remain open.

[Captured owner candidates](../native/docs/git-owners.md) now add `git owners`
over an unchanged saved impact/area parent. Base/head stay separate, each rule
and owner token retains its exact byte quote, all supported matches stay visible
and explicit empty-owner winners are preserved. The original owner parser and
report/replay identities remain unchanged. An internal callback keeps the same
checked read transaction through additional captured rule reads. Unsupported
active syntax makes winners indeterminate; uncaptured higher-precedence files
remain visible. The finite independent pattern policy and authored labels do
not establish actual ownership, identity, GitHub permission/review requirements
or full upstream parser/schema equivalence. No Git/provider/network/model,
current-source fallback, capture or Store/source publication occurs.

All **10 new saved-Git/CLI contracts** and **three included library parser cases**
pass: exact empty/rule/token quotes, same-label distinct positions, base/head,
live-source isolation, unsupported/uncaptured cases, independent exact quotas,
output, tampered CAS, SQL/caller transaction/cancel/deadline and offline CLI
admission/no initialization. The frozen selected run records **467 passed /
0 failed / 7 existing ignored gates** across four native groups. Clippy,
format, **17 Python tests**, mandatory typecheck and **260 TypeScript tests /
3 existing skips** pass with **681 integrated inputs unchanged**.
[Evidence](validation/integration-2026-10-19/captured-owners/index.json) retains
the initial API RED, implementation attempts, source hashes and unchanged
legacy-parser proof. Hard maxima are not claimed as fully populated qualification.
These scoped results do not replace earlier whole-native failures, including
Windows OS 1314 symlink prerequisites. Wider R5/R1-R9 work, whole-native/platform
acceptance, main merge and release remain open.

[Native received PostgreSQL catalogs](../native/docs/postgres-catalog.md) now add
an explicitly host-authorized `postgres-catalog capture/read` path. Rust owns
fixed serializable read-only metadata SQL, bounded client supervision, typed
immutable archives and offline queries. A trusted SHA-pinned `psql`18 installation,
network access and create-only output each require permission. TLS verify-full,
SCRAM/channel binding and endpoint-scoped OS credentials are separate from
explicit literal-loopback trust. Original received UTF-8 record ranges, routine
overloads, directed parallel FK constraints and ordered composite column metadata
remain intact. Definitions are stored bodies/opaque server trees or explicit
hidden/oversize states; no placeholder SQL, column nodes or stored routine calls
are synthesized. Imports remain unverified received data with `complete:false`.

All **14 new contracts** pass. The final frozen selected run records **479 passed /
1 failed / 11 existing ignored gates** across nine native groups; the sole failure
is the unchanged real Windows archive-CLI symlink fixture setup (OS1314). Clippy
for all targets, format, **17 Python tests**, mandatory typecheck and **260
TypeScript tests / 3 existing skips** pass with **689 integrated inputs unchanged**.
Actual Windows PostgreSQL18.6/psql18.6 validates Unicode database/role/password,
TLS/SCRAM/channel binding, hostname/auth downgrade and quota refusals, no ambient
profile/config authority, lost-stdout publication identity, deadline/peer cleanup,
visible/hidden definitions and offline traversal after server shutdown. All three
owned final-test OS credentials are removed. [Frozen evidence](validation/integration-2026-10-19/postgres-catalog/index.json)
retains the initial API RED, actual Unicode/CRL/prompt/schema defects and corrected
runs. These attempts are nonadditive. No libpq/server memory bound, maximum-size,
runtime-library attestation, other-server/non-Windows/upstream certification or
whole-native pass is claimed. Remaining R7 connectors, wider R1â€“R9 work, main
merge and release stay open.

[Native received HTTP sources](../native/docs/source-acquisition.md) now add
`source-acquisition capture/read` for explicitly selected public text, raw HTML
and PDF representations. One host-authorized GET uses a literal socket address
and the URL's TLS/HTTP name, with static public trust anchors, separate network/
output/loopback permissions and no proxy, redirect, retry, credential lookup or
script/resource fetching. Archives and ranges preserve exact body bytes,
Unicode/CRLF and byte offsets. Headers and imported capture claims remain
unverified data; no canonical source snapshot or semantic/server authority follows.
Explicit frozen PDF text uses the installed native bounded worker without Store
or original-path reacquisition. HTML article/DOM interpretation remains unavailable.

All **14 new contracts** pass. The final frozen selected run records **516 passed /
1 failed / 8 existing ignored gates** across twelve native groups. The sole failure
is the unchanged media source-policy test's real Windows symlink setup (OS1314).
All-target Clippy, format, **17 Python tests**, mandatory typecheck and **260
TypeScript tests / 3 existing skips** pass with **695 integrated inputs unchanged**.
Actual Windows native HTTPS captures example.com HTML and the W3 PDF fixture,
replays matching hashes, extracts native PDF text and preserves publication
identity after lost stdout. Owned protocol tests retain TLS name/root checks,
literal-address resolution, chunked bytes, malformed/truncated/duplicate/encoded
responses, exact quotas, independent permissions and cancel/deadline peer closure.
[Evidence](validation/integration-2026-10-19/source-acquisition/index.json) retains
the initial unavailable API RED, TLS-fixture Winsock10035 diagnostic and explicit
blocking-socket correction. Package versions remain unchanged; the existing
rustls-native-certs0.8.4 is now a direct dependency. Public payloads remain local
with published hashes/receipts and owned replay fixtures. Attempts are nonadditive.

At the preceding HTTP checkpoint, the direct arxiv API attempt timed out and a
public abstract-page investigation succeeded. The dedicated native route now has
its own separately frozen acceptance below; generic HTTP still refuses arxiv.

[Native received arXiv sources](../native/docs/arxiv-source.md) now add a
dedicated host-authorized abstract/PDF route with literal addresses, verified TLS
names and shared-file rate coordination. The exclusive journal spans each owned
request; completion starts a three-second cooldown. Pending/corrupt state refuses,
and explicit exact-hash pending recovery sends no request. Operators must share
the same coherent-lock file and UTC clocks across participating clients.

Frozen head metadata preserves each citation occurrence, repeated author labels
and exact original whole-tag byte evidence. Decoding does not establish a browser
DOM, paper version, author identity, server truth or a canonical code snapshot.
No default model call, project execution or query-time acquisition is added.

All **16 new contracts** pass. Final selected native evidence records
**532 passed / 1 failed / 8 existing ignored gates** in
15 groups. Failures remain in the evidence: `media_cannot_bypass_source_scope_links_ignore_or_protected_paths`.
All-target Clippy, format, **17 Python tests**, mandatory typecheck and
**260 TypeScript tests / 3 existing skips** pass with
**700 integrated inputs unchanged**. Actual Windows native
frontend metadata and PDF captures share completion sequences 2/4, matching
body/archive hashes and six distinct author occurrences. Arxiv PDF text extraction
is not claimed; public payloads and full abstract output remain ignored/local.

[Evidence](validation/integration-2026-10-19/arxiv-source/index.json) preserves the
initial API-unavailable RED, genuine comment-boundary panic and correction, two
test-fixture corrections, the first Clippy warning and an earlier unchanged
Git-process termination test failure. Attempts are nonadditive; later passes do
not erase earlier failures or establish their cause. Dependencies are unchanged.
Remaining Google/social/video/database and semantic connectors, wider R1â€“R9,
full native/platform acceptance, main and release stay open.

[Windows owned-member exit proof](validation/integration-2026-10-19/windows-process-exit/index.json)
now strengthens explicit process cleanup. It retains up to 4,096 observed nested
job members' synchronization handles before termination and requires their final
signals, the held root's exit and empty job accounting within the same existing
cleanup budget. Incomplete or unavailable member admission still requests owned
termination and reports missing proof. No PID-based termination is introduced.

The diagnostic RED reproduces a nonsignaled owned process at normal deadline
return with `cleanupUnproven=false`. The new guard passes **17 targeted cases**,
including two new OS permission/ownership contracts. Final selected native
results are **534 passed / 1 failed / 8 existing ignored gates**;
the sole unchanged media symlink setup failure remains OS1314. All-target Clippy,
format, mandatory typecheck, **260 TypeScript tests / 3 skips**, and **17 Python
tests** pass with **700 integrated inputs unchanged**. Actual Windows
PostgreSQL TLS/SCRAM/Unicode capture, cleanup of all three fixture credentials,
lost-stdout identity and frozen native W3PDF worker replay pass on the new binary.

The earlier eight same-binary and four temporary-trace passes are nonadditive.
Temporary tracing is absent from final code. This observed-member/OS proof does
not certify a kernel scheduling cause, every historical transient process,
arbitrary kernel API preemption, full Windows/non-Windows/upstream acceptance,
unattended App deployment, main or release. Wider STATUS/R1â€“R9 remains open.

[Reviewed native Neo4j publication](validation/integration-2026-10-19/neo4j-publish/index.json)
now adds an explicit Windows host workflow for saved native base graphs. Offline
plans grant no capabilities. Separate approval, network, credential, schema,
write and journal gates protect fixed parameterized Query API v2/Cypher 5.
Immutable scoped revisions retain original entity/evidence/observation JSON,
byte ranges, directions, parallel observations and unresolved targets. Atomic
expected-revision/count guards and durable pre-send intents prevent silent
overwrite or automatic replay of unknown writes. Recovery uses only fixed reads.
See the [host workflow](native-neo4j-publish.md).

The owned Neo4j Community **2026.09.0** fixture passes actual native HTTPS,
static-root/hostname refusals, Unicode private-pipe credentials, exact row
roundtrips, two forced concurrent updates with exactly one commit, lost stdout,
response loss after actual commit, historical recovery and count/schema conflict
refusals. All five fixture credentials and the Java server are removed. JBR25.0.3
is functional test evidence, not JVM-vendor qualification. The exact final
production binary is unchanged across a test-only cfg-import compiler fix.

There are **21 new cases**. Final 17 selected native groups record **555
passed / 1 failed / 8 existing ignored gates**; the unchanged media
symlink setup failure remains OS1314. All-target Clippy, format, mandatory
typecheck, **260 TypeScript tests / 3 skips** and **17 Python tests** pass with
**714 integrated inputs unchanged**. Native/live input proofs cover
708 files. Earlier incremental runs are nonadditive and all recorded
failures remain in the evidence. This is not a whole-native pass.

Live publication/credentials remain Windows-only. Hash-chain metadata is not
authentication or global journal coordination; client cancellation cannot
guarantee server cancellation. Other Neo4j versions/JVM vendors, Enterprise RBAC,
non-Windows and full upstream/mixed-source equivalence remain unqualified.
FalkorDB, Google/authenticated/social/video/semantic connectors, widerR1-R9,
unattended production, main and release remain open.

[Reviewed native FalkorDB publication](validation/integration-2026-10-19/falkor-publish/index.json)
adds a separate explicit Windows host workflow for saved native base graphs.
Closed offline plans grant no authority. The native bounded RESP2/TCP/TLS client
uses pinned sockets, verified hostnames/static roots and private-pipe OS
credentials. Fresh-graph WATCH/EXEC initialization, five indexed properties and
three operational unique constraints precede reviewed publication. Immutable
revisions preserve all original row JSON, byte evidence, directions, parallel
observations and unresolved targets; atomic expected-revision/count guards,
durable pre-send intents and positive fixed-read recovery prevent silent
overwrite or automatic replay of unknown results. See the
[host workflow](native-falkor-publish.md).

The owned **FalkorDB6.0.2/module60002** VM passes actual native TLS, wrong-root
and hostname refusals, Unicode credentials, exact row roundtrips, two forced
concurrent updates with one commit, lost stdout/commit response, historical
recovery, count/schema conflicts, foreign graph preservation and actual native
WATCH/EXEC interference. All five fixture credentials and the owned VM are
removed. The same final binary revalidates the full owned Neo4j connector live
fixture; its five credentials and Java server are also removed.

There are **25 new cases**. The final **20 selected native groups** record
**580 passed / 1 unchanged media OS1314 failure / 8 existing ignored gates**.
All-target Clippy with warnings denied and format pass; **724 native inputs**
and each live binary remain unchanged during their recorded runs. Mandatory
integrated checks are retained in the indexed evidence. Earlier incremental
results are nonadditive and recorded failures remain visible. This is not a
whole-native or other-platform pass.

Only module60002 is admitted. Production/RBAC, other database/JVM/server
versions, non-Windows and full mixed-source/upstream equivalence remain
unqualified. Google/authenticated/social/video/article/semantic connectors,
widerR1-R9, unattended production, main and release remain open.

[Managed native Codex MCP installation](validation/integration-2026-10-19/codex-host/index.json)
adds a separate explicit Windows `codex-mcp-v1` profile. Closed host selections
bind source/configuration/state directories and launch objects without home
discovery. The existing lossless TOML planner and guarded installation journals
preserve human bytes, stale conditions and ownership. Generic v1 serialized
identities stay unchanged. Initialization and both recovery schemas replay the
selected profile's canonical edit. See the [workflow](native-codex-host.md).

Actual **Codex CLI0.163.0** parses the installed command/argv/cwd/tool list,
starts its configured native server and dispatches an exact Unicode/CRLF source
read through the real MCP client. No model turn is started and the owned
loopback model endpoint receives no request. All owned processes exit with an
empty checked job after the bounded cleanup grace. Uninstall preserves human
tables; process-exit/resume, kill/rollback and stale-source recovery cases pass.

There are **12 new cases**. The final **13 selected native groups** record
**558 passed / 0 failed / 8 existing ignored gates**.
All-target Clippy with warnings denied, build and format pass. Native/live
input proofs cover **727 files** and the exact final binary. Mandatory
integrated checks are retained in the indexed evidence. Earlier test compiler,
canonical-recovery and public-status assertion failures remain visible and
incremental runs are nonadditive. The earlier media OS1314 prerequisite failure
is outside this selection; no whole-native or platform-wide pass follows.

Other branded clients/hooks, other Codex versions, interactive IDE/model tool
selection, project-local nested config roots, non-Windows and full upstream
qualification remain open. Multi-file visibility/power-loss guarantees are not
established. WiderR1-R9, unattended production, main and release remain open.

[Saved-plan outcome feedback](validation/integration-2026-10-19/plan-feedback/index.json)
adds explicit `feedback plan-record` and `plan-rank` over existing native Stores.
Canonical saved-plan replay, full base/derived View proofs and exact changed-file
side quotes bind authored historical outcomes to planned source bytes. A separate
event schema preserves released knowledge-event identities. Ordered immutable
history, visible integer decay/session selection, output-before-insert SQL
publication and validated retention roots retain their explicit budgets.
See the [contract](../native/docs/outcome-feedback.md#authored-outcomes-for-saved-change-plans).

All **14 new cases pass**. The broad **14-group** run records **585 passed /
3 failed / 7 ignored**: two unchanged Windows symlink fixture setup failures
(OS1314), plus a new test that assumed primary source-scope ordering. The test
now selects the explicit role and snapshot ID. The corrected **4-group** run
records **63 passed / 1 failed / 0 ignored**, with the existing feedback symlink
prerequisite still failing. Format, build and all-target Clippy with warnings
denied pass. The library's **465 passed / 0 failed / 7 ignored** remains bound
to identical production sources; only the test changes across the recorded
bridge, and the exact production binary is unchanged. The scoped runs are
nonadditive; mandatory integrated checks are indexed separately.

Planned results never establish actual application, execution, independent
corroboration or current applicability. Mutable verification/approval records
are not consumed as proof. No model, provider, capture or project execution
occurs in these operations. Additional targets, semantic transcript learning,
broaderR1-R9, whole-native/platform acceptance, main and release remain open.

[Explicit native Python empty-suite deletion](validation/integration-2026-10-19/python-empty-suite/index.json)
adds closed `deleteSymbol.emptyPythonBody: reject|pass` to native preparation.
The absent/reject default remains unchanged. Explicit `pass` fills only a
required parent block with no other non-comment statement, after exact original
AST/name/kind/range/decorator binding and before whole-result syntax replay.
Original coordinates, BOM, Unicode, CRLF, indentation and neighboring comments
remain intact. Mixed edits and other languages refuse the option. See the
[finite contract](../native/docs/semantic-refactor-intents.md#explicit-native-python-empty-body-deletion).

There are **10 new cases**: one unit admission, eight default integration and
one explicitly selected actual-Python gate. The final **10 selected native
groups** record **559 passed / 0 failed / 8 existing ignored gates** with
**732 inputs** unchanged. Actual **Python3.14.7** parses the copied source using
stdlib AST only, then separate permission/approval/application/recovery checks
preserve exact source bytes. The module and unknown decorators never execute.
Native MCP prepares the exact plan without granting application or execution.
Format/build/all-target Clippy with warnings denied pass; mandatory integrated
format/Python/typecheck/TypeScript results are indexed separately.

The retained first new test assumed Rust Result errors instead of the API's
error envelope; the corrected test checks denial and unchanged state. An
earlier broad run records **558/1/8**: an unchanged HTTP fixture required idle
cleanup to exhaust its20ms budget. Its test now checks the configured socket
timeout against that original ceiling and retains the250ms wall-clock upper
bound. HTTP runtime source and the exact production binary remain unchanged
across this test-only correction. Incremental runs are nonadditive.

Semantic-provider/facade defaults, saved-plan shapes/identities and independent
usage/approval/verification/stale/recovery gates remain. This finite policy does
not establish consumer safety or general refactoring semantics. Earlier OS1314
prerequisite failures lie outside this selection; no whole-native/platform
acceptance follows. BroaderR1-R9, hosted acceptance, main and release remain open.

[Explicit TS/JS sibling-declarator deletion](validation/integration-2026-10-19/sibling-declarators/index.json)
adds closed native `deleteSymbol.siblingDeclarators: reject|preserve`. The
absent/reject default remains strict. Explicit preservation binds one simple
function-valued TS/TSX/JS/JSX variable in a statement `const`, `let` or `var`,
then removes that original AST node and one direct parent comma as two separate
byte edits. Other declarators, export/ambient wrappers, semicolon, whitespace
and original entity/evidence coordinates stay intact. Nested commas are not
delimiters. Ambiguous direct comments, unsupported bindings/containers, mixed
edits and combined policies refuse. Full-result syntax replay retains the
source/AST/deadline bounds. See the [contract](../native/docs/symbol-deletion.md).

There are **10 new cases**: one unit admission, eight default integration and
one explicitly selected actual-compiler gate. Final **11 selected native groups**
record **568 passed / 0 failed / 9 existing ignored gates**, including
**467 / 0 / 7 library tests**, with **734 inputs** unchanged. Actual
**TypeScript 5.8.3 / Node24.21.0** compile the copied TS, JS, TSX and JSX results,
then separate permission/approval/application/recovery checks preserve exact
BOM/Unicode/CRLF source. No analyzed module executes. The previously implemented
Python AST-only application/recovery gate is revalidated on the identical final
source. Format/build/all-target Clippy with warnings denied pass; mandatory
integrated format/Python/typecheck/TypeScript results are indexed separately.

Earlier 15/0/1 and 26/0/1 selected runs and the initial actual compiler gate stay
separate and nonadditive. Native MCP prepares the exact effect without source
write or execution authority. Saved plan shapes/identities, semantic-provider/
facade defaults and independent application gates remain. Other source shapes,
comment attachments, consumer safety and general semantic/IDE equivalence stay
open. Earlier OS1314 prerequisites lie outside this finite selection; no
whole-native/platform acceptance follows. BroaderR1-R9, main and release remain open.

[Concrete class-method decorator ownership](validation/integration-2026-10-19/member-decorators/index.json)
adds a checked deletion range for plain-named concrete TS/TSX/JS/JSX methods.
After exact original AST/name/kind/range binding, up to64 contiguous direct
sibling decorators and their whitespace/comment gaps belong to the selected
method. Parent and neighbor decorations, ordinary earlier comments and original
entity/evidence coordinates remain intact. Unsupported gap tokens, constructor,
abstract/signature and computed/quoted/private decorated names refuse. The
entire resulting source is parsed under the original bounds. This adds a
previously refused native and provider-finalization case; no input/authority
schema changes. See the [contract](../native/docs/symbol-deletion.md).

There are **11 new cases**: one unit admission, eight default integration and
two explicitly selected actual gates. Final **12 selected native groups** record
**577 passed / 0 failed / 11 ignored**, including **468 / 0 / 7 library tests**,
with **736 inputs** unchanged. Actual **TypeScript 5.8.3** compiler/AST checks all
four extensions and proves parent/neighbor decorator association; actual
**language-server5.1.3** safeDelete produces the same checked source plan.
The fixtures use `experimentalDecorators: true`. Independent permission,
approval, application and exact-recovery checks pass without executing analyzed
modules or decorators. Existing sibling/Python gates are revalidated on the
identical final source. Format/build/all-target Clippy-Dwarnings pass;
mandatory integrated checks are indexed separately.

The initial baseline overlapped a still-running formatter and had changed
source hashes; it is retained as invalid frozen evidence and contributes no
accepted counts. A clean repeated baseline proves the original refusal. An
early negative test assumed every unsupported form had an indexed method;
the corrected test treats a missing entity as an unknown selection. All logs
and source manifests remain. Scoped runs and actual gates are nonadditive.

Saved-plan identity, insert/replace ranges and independent usage, write,
verification, stale-source and recovery contracts remain. Other member/name/
language forms, decorator modes/versions, compiler/consumer/dispatch/runtime
proof and universal IDE parity stay open. Earlier OS1314 prerequisites are
outside this finite selection; no whole-native/platform/main/release acceptance
follows. BroaderR1-R9 work remains active.

[Explicit wrapped-call propagation](validation/integration-2026-10-19/wrapped-propagation/index.json)
adds Rust API `safeDelete.propagationPolicy: captured_es_function_propagation_v2`.
It admits complete standalone plain calls within at most32 parentheses, `await`
or `void` wrappers, directly in a program or block. Defaultv1 and published
facade schemas stay unchanged. Original provider/source bindings and dependency
closure remain; unknown consumers remain visible. Full-result grammar and
bounded surviving-node kind/nesting/field/order/mapped-byte comparisons refuse unintended
statement merging at automatic-semicolon boundaries. Other expression consumers,
optional/type/assertion/unclassified forms and lone control-flow slots refuse.
See the [contract](../native/docs/propagated-delete.md).

There are **eleven new cases**: one unit, nine default integration and one actual gate.
Final **13 selected native groups** record **587 passed / 0 failed / 13 ignored**,
including **469 / 0 / 7 library tests**, with **737 inputs** unchanged.
Actual **TypeScript 5.8.3** and **language-server5.1.3** validate copied TS/JS
results for both v1 and v2, with separately exercised execution permission,
approval, write permission, exact application and recovery. No analyzed module
runs. Format/build/all-target Clippy-Dwarnings pass; mandatory integrated checks
are indexed separately. Counts overlap and are not added to other runs.

The first selected run records26/1/0 because a new test compared independently
created plan IDs/timestamps. Its correction compares bound snapshots and exact
effects while retaining the released identity formula. An intermediate passing
run predates the additional surviving-syntax guard. A fresh regression proves
the statement-merging risk before the guard; every log/source variant remains.
Other propagation/declaration/file/language/provider intents and exhaustive
compiler, consumer, runtime or IDE guarantees remain open. Earlier OS1314 fixture
prerequisites lie outside this selection; no whole-native/platform/main/release
acceptance follows. BroaderR1-R9 work remains active.

[Native session refactoring](validation/integration-2026-10-19/session-refactor/index.json)
makes existing `safeDelete`, `rename`, `inline` and `move` plans available through
`session --requests` and native `ci_session` with `refactor.prepare`.
The closed envelope accepts native arguments up to64,000 serialized UTF-8 bytes.
Mode, host-project and exact stored-session admission checks precede provider
work. Another client's steering requires resuming the session. The original
deadline, source/provider/snapshot bindings and native results remain; preparation
does not steer the session or write original source. Approval, executed
verification, workspace permission, stale-source checks and recovery remain
separate. See the [contract](../native/docs/agent-sessions.md#native-semantic-planning).

Ten new cases include nine default integration tests and one explicit actual
gate. Final **15 selected native groups** record **577 passed /
0 failed / 11 ignored**, with **738 inputs** unchanged.
Actual native CLI bytes are hashed before/after use and match the final binary.
Actual **TypeScript 5.8.3** / **language-server5.1.3** validate TS/JS safeDeletev2,
copied compiler-only results and independent approval/verification/write/stale/
recovery gates. No analyzed module runs. Format/build/all-target Clippy with
warnings denied pass; mandatory integrated checks are indexed separately.
The initial unsupported-operation regression, all source variants and scoped
results remain; counts overlap and are not additive.

The normal native eight-tool and published facade argument schemas remain;
the native session tool description advertises the added operation. Current
session validation is an admission check, with no lease over future state or
source changes. Native plan identity/publication behavior remains unchanged.
Other provider intents, client installations, exhaustive compiler/consumer/
runtime guarantees and broaderR1-R9 remain open. Prior OS1314 prerequisites lie
outside this selection; no whole-native/platform/upstream/main/release acceptance
follows.

[Function binding propagation](validation/integration-2026-10-19/function-bindings/index.json)
adds explicit `captured_es_function_propagation_v3` for single plain const/let/var
bindings initialized directly by arrows or anonymous function expressions.
Regular named functions remain supported. Existing function entities bind exact
variable-declarator/name ranges; the owning statement/export supplies the wider
removal range. Original entity/evidence/analyzer identities remain unchanged.
Defaultv1 and explicitv2 keep their previous module boundary. Multiple/pattern/
ambient/using/nonfunction/parenthesized/named-inner-expression forms and direct
declaration comments refuse. Surviving assignments or value consumers cannot be
forced away. See the [contract](../native/docs/propagated-delete.md#explicit-function-binding-policy).

Eleven new cases include ten default integration tests and one actual gate.
Final **15 selected native groups** record **592 passed /
0 failed / 13 ignored**, with **738 inputs** unchanged.
Actual native CLI, **TypeScript 5.8.3** and **language-server5.1.3** qualify TS/JS
const/let/var cases, async arrows and Greek names. Copied compilation, independent
permission/approval/application and exact recovery pass without executing source
modules. The existingv2 actual route also passes on the final freeze. Format,
build and all-target Clippy with warnings denied pass; mandatory integrated checks
are indexed separately. Counts overlap and are not added across phases.

The original unsupported-policy regression and all intermediate source/logs remain.
The owned protocol fixture keeps named-function lookup precedence and adds a
variable fallback; it is not a compiler. The now-supportedv3 negative selector
case moves to unsupportedv4. Private cycles, retained/exported dependencies,
argument closure without unused sweeping,16/17declarations,128/129usages,
missing/tampered proofs and automatic-semicolon boundaries are checked.
Source/provider quotas, original deadline and guarded canonical plans remain.
Other declaration/file/language forms, alias/rebinding/general consumer analysis
and full upstream/IDE/whole-native/platform/main/release acceptance remain open.
BroaderR1-R9 remains active.

Windows ZIP and VSIX development packages were built at `9f48353`: 313 dependency
licenses and 551 complete notice files were checked. The extracted CLI passes
index/find/context/exact Unicode-source reads with only Windows system folders
on PATH. The VSIX contains the identical binary; no new extension-host run is
claimed. These packages predate the registry follow-up and are not a release.
No live App installation, public comment, deployment or main merge is claimed;
full functionality and universal parity remain open.

The checked mixed-source input preparation adds a closed `--meaning-inputs`
request and opaque library capability over existing frozen native, Office
decoded/XML and explicitly permitted PDF page reads. Exact excerpts, original
View/Meaning receipts and coordinate-space precision remain intact. Ordered
context occurrences count repeated bytes separately; host labels establish no
semantic identity. Preparation starts no provider/model or acquisition and
persists no input, source or graph. Mixed semantic execution and general
cross-source graph relationships remain open.

Source `8950d290512b775e7ef63bfd6c0814e4d133618d`, runtime merge `9587dac869fb8a6abcfc843654bb663c50e51c3f`.
Seven new default cases include the actual native PDF worker/CLI permission
case; they are included in, not added to, the final 13 selected groups:
**588 passed / 0 failed / 7 ignored**. Format, build and all-target Clippy with warnings denied pass;
739 native inputs remain unchanged. Integrated format, Python17,
typecheck and TypeScript260/3skipped pass with 745
inputs unchanged. The earlier compile-only lifetime failure is retained and
does not count as executed tests. This is scoped Windows evidence, not complete
R4, whole-native, platform, main or release acceptance. Broader R1â€“R9 remains
active. See [the contract](../native/docs/mixed-views.md) and
[indexed evidence](validation/integration-2026-10-19/mixed-meaning-inputs/index.json).

The checked mixed interpretation claim path adds `--meaning-claims HOST_FILE`
beside explicit prepared inputs. Host-submitted concepts, directed parallel/self
relations and ordered role-bearing hyperedges retain their original keys and
order. Every support descriptor stays inside its submitted typed unit and is
reread from the opaque checked mixed view, including exact Unicode quotation,
XML origins and original PDF/container pins. The original prepared input and
native receipt/source-scope metadata remain nested. Interpretation attestation
stays `host_submitted_unverified_interpretation` / `not_established`; source
quotations do not establish semantic truth or canonical source authority.
This read-only check starts no provider and persists no graph or receipt.

Source `a8982d4b8d7e5434e08df81f952a6378f5abf2ed`, runtime merge `0fb15f82f66c3ebc0659ab2790158ca919238012`.
Five new default cases cover all four spaces, directions/parallel observations,
role slots, invalid support/coverage, exact UTF-8/output/128000-byte quote bounds,
deadlines, explicit no-claim coverage and strict CLI preinitialization refusal.
The actual native worker is exercised by the fixture; one new case invokes the
actual CLI. First four groups43/0/0 and final14groups **593 passed / 0 failed / 7 ignored** pass on the
same 742 unchanged inputs, with format/build/all-target Clippy.
Integrated format/Python17/typecheck/TypeScript260+3skipped pass with
747 unchanged inputs. Counts overlap and are nonadditive.
Semantic provider extraction, durable mixed graph replay/general consumers and
broader R1â€“R9 remain active. Whole-native/platform/main/release acceptance does
not follow. See [the contract](../native/docs/mixed-meaning-claims.md) and
[indexed evidence](validation/integration-2026-10-19/mixed-meaning-claims/index.json).

The separate `mixed-meaning build` now executes one fully admitted, explicitly
trusted host request over typed native/Office/PDF units and fixed context groups.
It registers a single-use ledger/job before sending, accounts for a completely
bound reply before semantic/error rejection, retains unknown reservations and
publishes immutable original response and graph artifacts with exact row
preconditions. Status/jobs start no provider; load replays original source,
receipts, exact quotations and ledger response events. PDF replay retains its
own explicit worker permission. Retention recomputes native/document/media
dependencies and stored quote consistency without launching a PDF process.
Original aliases, claim order, directed parallel/self observations and ordered
hyperedge roles remain. Interpretations/usage/bills stay unverified; later ledger
reconciliation does not upgrade original unknown receipts. No default model
call, analyzed-source capture, automatic project-code execution or native-source
promotion is added.

Source `7ce36b5f96842442874dec43d2ae7b99f83eab63`, runtime merge `efec81bd0d387031a4c270a1a447379a1ca070d0`.
Eleven new default cases include actual owned Node/PDF protocol processes and
one end-to-end CLI workflow, Unicode/four-space evidence, invalid usage/claims/
error/overrun accounting, malformed/unbound replies, unknown-cost reconciliation,
caller transaction/SQLite limits, metadata4096/4097 admission, private malformed
host config and corrupted quote retention. These are protocol/host-process
qualifications; no semantic model, real service billing or model quality was
evaluated. Final18groups **614 passed / 0 failed / 7 ignored** and format/build/all-target Clippy pass
with 746 unchanged inputs. Integrated format/Python17/typecheck/
TypeScript260+3skipped pass with 750 inputs unchanged.
Earlier failed/diagnostic variants remain separate and nonadditive. General
mixed graph consumers, cross-workspace composition, full R4/R1â€“R9, whole-native/
platform/main/release acceptance remain open. See [the contract](../native/docs/mixed-meaning-provider.md)
and [indexed evidence](validation/integration-2026-10-19/mixed-meaning-provider/index.json).

The checked mixed interpretation graph now supports graph-qualified concept
lookup, case-sensitive literal label/alias search, directed multiedge degree
counts and bounded frontier traces. Only actual source/response replay can
construct the opaque capability, which owns its frozen source proof and exact
verified graph. Queries survive later metadata corruption or complete Store/CAS
loss without another provider/PDF process. Original graph IDs/load bytes, typed
quotations, receipts, parallel array positions, self-loops and ordered repeated
hyperedge roles remain intact. Roles do not create traversal direction, aliases
do not merge concepts, and complete/canonical-source authority remain false.
The explicit CLI query retains independent PDF permission at load; malformed
query shape refuses before missing Store initialization.

Source `0ddc05e21d408417ebb40b1563fac61f2f8ed37b`, runtime merge `3d316808c622857f4380a2ecaea9cb794ff98717`.
Eight new default cases include actual owned Node/PDF setup and end-to-end CLI
query, Unicode/ambiguous labels, foreign graph references, all directions/depth0,
self/parallel/role counts, Store loss, closed JSON/privacy, output/deadline
refusals and unchanged records/provider-call markers. Initial17/1 and the first final run's disk-full linker refusal are retained:
the latter executes no complete native acceptance and does not change source.
The initial17/1 run's sole failure expected the wrong static refusal code; no production fix was
needed for that case. Final19groups **622 passed / 0 failed /
7 ignored**, format/build/all-target Clippy pass with 749 unchanged
inputs. Integrated format/Python17/typecheck/TypeScript260+3skipped pass with
752 inputs unchanged. Counts are scoped and nonadditive; full R4,
cross-workspace composition, external semantic qualification and whole-native/
platform/main/release acceptance remain open. See [query contract](../native/docs/mixed-meaning-queries.md)
and [indexed evidence](validation/integration-2026-10-19/mixed-meaning-consumers/index.json).

Explicit checked mixed graph composition now combines 2..4 owned source/response-
replayed parents with an ordered host-authored link review. Parent-qualified
concept/observation identities, original parallel/reverse/self relations, ordered
hyperedge slots and complete nested typed source/receipt/accounting remain.
Host links are separate unverified assertions with independent indices and
degree counts; traces follow them only when explicitly requested. Labels and
aliases create no merge or semantic identity, and roles imply no flow. Library
parents may come from different checked workspaces/history. The CLI selects
explicit jobs from one root Store, with per-parent PDF/Office worker permission.
No provider, persistence, source write or default MCP schema change is added.

Source `789d50bb0d9a72a3ab97a1cc9f7e8eab91493203`, runtime merge `6702727d303cf6088a8b03702cfc76fddc434ec3`.
Eight new default cases include owned Node/PDF setup, cross-workspace qualified
keys, parallel host/provider rows, split degree counts, Unicode ambiguity,
direction/depth0/capacity, independent parent/endpoint validation, private closed
CLI admission, both Store losses and an actual PDF+Office same-Store CLI query
without installed provider files. Initial feature compilation stops at a test-
only Vec.repeat/Copy constraint before executing any test group; the fixture
uses explicit cloning afterward. Three focused groups pass **27/0/0**. Final20groups
**630 passed / 0 failed / 7 ignored** and format/build/all-target Clippy pass with 752
unchanged inputs. Integrated format/Python17/typecheck/TypeScript260+3skipped pass
with 754 unchanged inputs. Counts remain scoped and nonadditive.
Broader analytics, semantic qualification, full R4/R1-R9 and whole-native/
platform/main/release acceptance remain open. See [composition contract](../native/docs/mixed-meaning-composition.md)
and [indexed evidence](validation/integration-2026-10-19/mixed-meaning-composition/index.json).

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
