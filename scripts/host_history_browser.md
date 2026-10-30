# Owned Windows host-history browser harness

At `abdbdb03`, [diagnostic run 41013033443](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41013033443/job/123922975632)
passes the native, Serena and HTTP browser invocations and the original
three-variant history harness. All six PNGs for each successful browser profile
were individually reviewed, with independent worker/tree/browser exits. HTTP
also proves its actual credential/authentication, Session revocation, peer
survival, natural service lifetime and owned-credential cleanup gates.
Persistent Python fails at the mobile pending click and has three images only.
It remains unaccepted. [Source-bound evidence and focus correction](../docs/validation/integration-2026-10-30/browser-abdb-checkpoint/README.md).

The native owning stdio browser flow passed at `ed92e9f8` in
[Windows diagnostic run 40995799274](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40995799274/job/123868515540).
All six actual desktop/mobile before, pending and after PNGs were visually
inspected. Exact source/producer, archive readback, guarded save and independent
browser/driver/native/Job exit evidence is retained in the
[browser success checkpoint](../docs/validation/integration-2026-10-30/browser-success-checkpoint/README.md).
This qualifies that native stdio flow. It supplies no Serena, persistent-Python,
HTTP, visible-shell, human or release acceptance.

`host_history_browser.py` is an additional Windows CI gate after the existing
release build and successful `host_history_live.py` step. The default `native`
invocation and its evidence directory remain. Separate explicit `--variant
serena` and `--variant persistent-python` invocations now exercise both old and
current owning Sessions through the selected Serena protocol. Serena has a
successful six-image run at `abdbdb03`; persistent Python still requires its
complete successful browser run and all six images. HTTP has its own successful
six-image run at the same source. The original three-variant protocol/history gate, launcher,
budgets, tests and claims remain unchanged. Its old Serena/Python actors did
not make its destination dashboard a Serena/Python browser flow.

Each invocation retains the original independent 180-second worker and
four-minute CI step limits. There are four distinct fixed fresh output roots:
`validation/ci-host-document-history-browser`, the `-serena` suffix, and the
`-persistent-python` and `-http` suffixes. Each requires all six original image/DOM pairs and
the original independent lifecycle proofs; one variant cannot borrow another's
receipt or output directory. Added variant steps can collect evidence after a
sibling browser failure, but still require successful original history and
release-build steps. They cannot make a failed native gate pass.

The shared acceptance-step YAML anchor includes Windows conditions; Linux and
macOS do not invoke this Windows harness. The new step has a four-minute Actions
timeout and its own worker budget of 180 seconds. Its named Job launcher is an
independent source copy of the original launcher, including suspended assignment,
500,000,000-byte free-disk stop, 32,000,000-byte caps for each private/evidence
tree, cardinality limits, independent root exit and zero tree members before
forced cleanup. The old launcher and its call sites are unchanged. A pure static
regression test compares the two launcher ASTs.

## Admission and explicit host tools

Windows-only marker steps bracket the existing, unchanged Cargo release command.
They record exact selected source hashes and CI run/attempt/revision, then native
file size/mtime metadata after a successful build. They never read EXE/PDB bytes.
Marker failures are deferred to the separate fail-closed browser gate so they
cannot skip the original release build or three-variant history step.
The wrapper requires those same fresh producer/source facts, exact
`target/release/yoke.exe`, canonical unlinked paths and clean tracked source.
Cargo may legitimately reuse an unchanged cached image; these are host-owned CI
build receipts and metadata checks, not independent compiler attestation.

The internal worker requires the CI identity, closed bounded spec, private
random nonce and selected variant, exact fixture layout and membership in the parent's specific named
Job before its first native payload hash/read. It rechecks producer metadata and
source hashes first. All native, Node and browser children inherit that Job; no
breakaway flag or process discovery is used. Native diagnostics, fixture specs,
profiles, URL capabilities, CLI raw output and browser state stay private outside
the artifact directory. Failed admission never falls back to local/mock mode.

The workflow explicitly selects the setup-node host executable. The worker
selects Microsoft Edge only from the two fixed system installation paths in
source. There is no executable discovery in analyzed data, package installation,
browser download or Playwright dependency. Installed Edge absence and unsupported
CDP fail the gate. The driver uses built-in Node APIs and inherited CDP pipes
(`--remote-debugging-pipe`), with no DevTools TCP endpoint.

The browser uses a fresh owned profile, normal browser protections and a private
loopback dashboard. Background features and caches are bounded. The proxy points
at that same owned listener so ordinary background traffic cannot reach an
external destination. CDP request admission continues only the exact dashboard
origin and fixed paths unchanged; all other page requests are denied and fail
acceptance. It never fulfills a request, replaces a response, mocks fetch, changes
application HTML/JS, injects handlers or fabricates server receipts. This is not
an OS network sandbox. Normal CSP/Host/Origin/capability checks remain active.

## Actual workflow specified by the source

An inert owned Unicode/CRLF source fixture and BOM/CRLF global settings file are
created in private CI temporary storage. Two original saves seed closed history.
The browser worker has one fresh ordinary Windows profile: `HOME` and
`USERPROFILE` select the same private home, with `APPDATA`, `LOCALAPPDATA` and
temporary storage beneath its `AppData` hierarchy. LocalLow, Desktop, Documents
and Downloads are ordinary private directories. The legacy Internet cache
compatibility name `AppData/Local/Microsoft/Windows/INetCache/Content.IE5` is also
created as an ordinary directory to occupy the role before host cache use.
Any later replacement with a junction still refuses. An occupied or linked profile
refuses before reuse. This replaces the browser fixture's separate flat user
directories; the original three-variant history fixture remains unchanged.
All links still refuse, including profile links created later by a host tool.
Closed link diagnostics include known area/entry/route categories, a depth capped
at 16 and up to 16 domain-separated component SHA-256 fingerprints. Unknown
spellings map to `other`; fingerprints allow comparison with known component
names without publishing private paths. They are diagnostic observations, not
path authority or proof of a target's safety. Targets remain unread and all links
still refuse. Actual browser execution and all six visual inspections remain
required before acceptance.
The original host CLI migrates the binding without changing settings bytes. One
selected owning destination stdio Session supplies the actual dashboard; a second
old owning Session verifies the retired authority. The default variant selects
native; both other variants select Serena for **both** actors. The real bounded
`tools/list` response must expose Serena's dashboard/configuration tools and
exclude the native `ci_session` tool. Persistent Python additionally requires
`serena_repl`. These are private owning protocol observations, not substituted
browser responses. These three stdio variants do not use `serve-http`.

The persistent variant alone grants the already running CI worker's canonical,
unlinked host `python.exe` via the existing explicit `--python`/`--trust-python`
arguments. No analyzed source chooses an executable. Both owning Sessions start
a real persistent runtime, set a marker to 45, report their PID and retain an
independent identity-checked process handle. Both must return 45 after the full
browser/archive/save flow. Ending the destination must leave the old Session and
its value intact. Each runtime must independently signal exit after its owning
native process exits and before Job cleanup. Missing values, unavailable handles,
wrong images and live runtimes refuse acceptance. All owned teardown attempts
run even if an earlier cleanup refuses; no cleanup can create success.

The real desktop (1440×1000) and mobile (390×844, mobile metrics) pages load the
selected settings, enter an exact BOM/Unicode/CRLF draft with real browser input,
and review it through the UI. Browser textarea LF values and the application's
CRLF proposal bytes are checked separately. The last UI-generated server plan is
retained for pending refusal; the Session intentionally has one latest proposal.

An owned foreign completion occupant makes the original CLI history apply leave
a real committed, incomplete archive marker. Each page observes a real status
refresh and clicks Inspect recovery. Real network response data must report
`complete:false`, `blockedByHistoryArchive:true`, empty journals and no changes.
The selected visible blocker, exact draft, read-only editor, disabled Review/Save,
absence of unsupported recovery buttons, keyboard focus and horizontal bounds
are checked. Both native authorities separately refuse cached save and unsupported
recovery; complete owned target/owner/journal/archive hashes must stay unchanged.

The original explicitly authorized CLI resumes the history operation with its
actual reviewed event. All original records are checked against independent
archive framing and original host CLI readbacks. Each viewport then explicitly
inspects recovery, reloads (confirms discarding its old draft), enters and reviews
a fresh draft, and confirms a real guarded save. Independent filesystem checks
after **each** save verify exact target bytes, actual new journal generation/hash,
unchanged owner and immutable frame. Source bytes stay unchanged.

Real served bootstrap bytes must hash to the selected source dashboard HTML.
Runtime script exceptions or console errors fail. Response/DOM/console/CDP frame,
JSON, base64, PNG, command/event counts and every wait have finite bounds. There
are six bounded actual full-content PNGs and six selected-node DOM observations,
each hashed: desktop/mobile before, pending and after. PNGs are capped at 2 MB
each/12 MB total, 4096 CSS pixels high and four million pixels each. These caps
fail rather than silently crop or skip a meaningful state.
After worker exit the parent independently requires all six distinct expected
image/DOM pairs, bounded PNG headers/dimensions and exact hashes; an empty or
protocol-only proof cannot complete the browser gate.

`Browser.close`, browser raw exit 0 and CDP/stdout/stderr EOF, Node raw exit 0 and stdout EOF,
both native owning exits, worker raw exit 0 and zero Job members **before** force
are success requirements. Forced cleanup only closes failure leftovers; it cannot
make acceptance succeed. Private fixtures are retained for the ephemeral runner;
there is no recursive deletion or foreign-process kill.
Already admitted asynchronous CDP handlers drain before `Browser.close`. Only
the transport's typed clean EOF may precede its acknowledgement; protocol
refusal, timeout, partial frames and write errors still fail. Raw browser and
driver exit/EOF receipts remain independent requirements.

## Explicit HTTP owning-Session variant

`--variant http` alone starts the existing public `serve-http --trust-service`
CLI, twice, with separate fixed loopback listeners, private host configurations
and Session-profile principals. It grants no Python runtime or source writes.
The two actor APIs select the same original global-document host authority as the
stdio fixtures. The real Session tool catalogs must expose `ci_session` and omit
Serena aliases. All original browser requests, HTML hashes, draft/pending/save
assertions, archive readbacks and six PNG/DOM checks are retained.

The admitted worker creates two fresh **logon-scoped** Windows generic
credentials. Names use a fixed owned prefix and 192 random bits; token bytes
contain 256 random bits. Creation shares the native credential store's named
mutex and refuses occupied names. Readback checks exact name, type, scope,
capacity and bytes. Only reference names enter private host configuration;
credentials never enter arguments, fixture files, source snapshots or artifacts.
Deletion rechecks exact bytes under the same mutex and verifies absence.
Mutable copies and native read buffers are cleared; CPython and OS temporary
copies are not claimed to have independently verified secure erasure.

These credential APIs run only inside the already admitted Windows CI worker
Job, after producer/source/native-image admission. No user credential is selected
or enumerated. Handled failure cleanup attempts every owned service, reader,
pipe and credential even if an earlier operation fails. A failed deletion or
identity/absence check cannot yield successful automation. A forcibly killed
worker may leave a logon-scoped entry until the ephemeral runner logs off;
that run remains failed. The harness does not claim physical/atomic erasure or
containment against another hostile process under the same OS account.

The actual HTTP client uses fixed IPv4 loopback `/mcp`, mandatory bearer/SID and
negotiated-version headers, bounded JSON or finite notification SSE, exact
Content-Length plus EOF, 16 KiB/48 header limits, and one ten-second absolute
request deadline. New bytes cannot renew it. No redirect, remote bind, TLS
fallback, response substitution or caller-supplied endpoint is available.
Missing/wrong bearer and foreign Origin must refuse; each valid principal must
also refuse the other service's transport Session ID.

HTTP dashboard shutdown acknowledges only the **owning MCP Session**. The client
requires its SID to return 404 within eight seconds while its service remains
alive. The old Session and its actual tool catalog must still work after the
destination closes. Both Sessions close before either service exit is admitted.
Each service has the fixed public CLI lifetime of 120,000 ms. The harness then
requires actual raw exit zero, unclosed input, stdout EOF and the final stopped
receipt before Job cleanup. Parent receipt admission distinguishes these two
Session closures from two natural service exits and checked credential removal.
The original eight-second stdio exit waits, 180-second worker and four-minute
step are unchanged. Only this selected HTTP fixture waits for its independently
bounded service lifetimes. Early service exit is not Session isolation proof.

The HTTP helper and 21 adversarial unit tests use only Python standard-library
APIs. Focused suites pass 79 tests with mocked HTTP/native/credential peers and
inert Job fixtures. Actual Credential Manager, public CLI/MCP/dashboard behavior,
natural service lifetime and all six HTTP screenshots subsequently pass at
`abdbdb03`, with the separate source-bound visual review linked above.
[Original candidate review scope](../docs/validation/integration-2026-10-30/browser-http-candidate/README.md).

## Variant-bound driver startup correction

At `d6b9da55`, native automation passes again, but both new Serena/Python runs
fail at `browser-start` / driver `phase-entry` before screenshots. The driver
still admitted only the native output basename. A controlled actual-guard
comparison reproduces that rejection without launching a browser. The corrected
closed start payload includes its selected variant, bound to the private worker
environment and the exact repository evidence path. Unknown, cross-profile,
relative, foreign and traversal outputs refuse before filesystem observation
or Edge launch. Existing Windows/CI/nonce, URL, system-browser and fresh-profile
checks remain. The startup receipt echoes the variant; constructor and parent
both require that binding. Startup refusal now closes the constructor's owned
Node/process/channel handles using the original bounds.

All original behavior after driver configuration remains, except that additive
startup receipt field and the pure test helper export. The full browser
interactions, requests, pending/save assertions, image bounds, waits and shutdown
proof remain. The correction passes 80 focused Python and 11 in-memory Node
tests plus causal guard RED/GREEN, typecheck and npm gates (266 passed, three
skipped). Fresh corrected live runs and all six visual inspections per selected
profile remain open. [Retained source-bound evidence](../docs/validation/integration-2026-10-30/browser-driver-variant-checkpoint/README.md).

## Evidence and remaining gates

At `42b94aca`, diagnostic run `40983256567` builds the release CLI and passes
all three original owning-Session history variants. The real-browser worker
then fails at the closed `browser-pending` exercise stage after capturing both
before-review states. Those two new desktop/mobile PNGs were inspected and are
readable without horizontal overflow; four later states remain absent.
[Exact pending-stage checkpoint and original history receipts](../docs/validation/integration-2026-10-30/browser-pending-checkpoint/README.md)
retain those earlier-source outcomes and their selected-source comparisons.

An owned Edge-only diagnostic demonstrates that the original click can observe
the recovery button focused while its real request has disabled it, followed by
focus moving to the body when the request completes. The candidate waits for
the control to be enabled after the second real pending receipt and returns to
it with at most twelve actual Tab key pairs. It never assigns DOM focus or
changes the application, disabled controls, response data or pending assertions.
All original write/draft/recovery, request-count, screenshot and shutdown gates
remain. Fixed internal pending checkpoints and viewport labels are emitted on
failure only; malformed labels and unavailable diagnostic output cannot admit
success or disclose private data. The precise hosted failure assertion remains
unproven until the new real-browser run supplies its result.

At `76333e2e`, diagnostic run `40974744304` reaches both actual desktop/mobile
before-review captures, without reporting the earlier junction refusal. Both
images were visually inspected and remain readable without horizontal overflow.
The worker then exits 1; four states and complete automation remain absent.
[Exact partial artifacts and inspection limits](../docs/validation/integration-2026-10-30/browser-before-checkpoint/README.md)
retain the failed result. Failure-only closed exercise-stage labels now identify
the next failing stage without exposing raw exceptions, CLI/protocol bodies,
paths, capabilities or endpoints. Observer-output failure preserves the original
exception. No original operation, assertion, deadline or acceptance gate changes.

For focused CI diagnosis, a pushed commit whose message contains
`[browser-evidence]` also starts the separate Windows history-browser diagnostic
job. It uses the same release producer markers, original three owning-Session
history variants, guarded browser script, and five/four-minute step ceilings.
The job has its own 45-minute ceiling and uploads distinct `diagnostic-*`
artifacts. It does not depend on native shard success, so an unrelated test
failure cannot hide browser startup evidence. It supplies no shard coverage
receipt and does not replace the full native acceptance jobs or their merge
and release requirements. No diagnostic success establishes visual review.

Only the four fixed browser evidence directories are uploaded, as separate artifacts. Source maps,
validator/driver/native hashes, producer metadata, installed browser/engine/Node
versions, selected DOM states, real screenshot hashes, CLI outcome summaries and
raw lifecycle proofs are retained. Capabilities, URLs, endpoints, headers, storage,
raw CDP, raw browser logs and full arbitrary DOM/error bodies are excluded.

`automationComplete` means the specified real-engine assertions and lifecycle
proofs passed. `realInstalledHeadlessBrowser` and screenshot capture are distinct
from visible desktop-shell/manual interaction. `visualReviewComplete`, human
acceptance, visible-shell interaction, all-transport and release claims remain
false. The parent must actually run the guarded CI step, inspect **all six PNGs**,
fix visible issues and regenerate/inspect again before recording visual acceptance
in a separate reviewed handoff. The harness cannot self-certify human review.

The focused browser and original-history Python suites pass locally, including
in-memory Node CDP/focus regressions, complete Python fixture orchestration for
all selected profiles, retained-value and runtime-exit refusal, wrong-profile
and wrong-output admission, and teardown failures. These use mocked native and
browser peers plus inert Job fixtures; they establish no new browser or Yoke
runtime acceptance. Fresh hosted runs and visual review remain required for each
new profile. The original native acceptance, HTTP browser transport, whole-project
CI, strict macOS worker memory and release gates remain separate.

Protocol design references: [CDP domains](https://github.com/ChromeDevTools/devtools-protocol/tree/master/pdl/domains)
and [upstream pipe stdio launch contract](https://github.com/puppeteer/puppeteer/blob/main/packages/browsers/src/launch.ts).
The harness uses neither upstream implementation as a runtime dependency.
