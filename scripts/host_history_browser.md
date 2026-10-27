# Owned Windows host-history browser harness

This is a **source-only implementation, with runtime and visual acceptance unrun**.
`host_history_browser.py` is an additional Windows CI gate after the existing
release build and successful `host_history_live.py` step. It neither replaces nor
extends the original three-variant gate, its budgets, tests, or claims. No product
UI or automatic browser behavior changes. The parent owns global status/release
claims.

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
random nonce, exact fixture layout and membership in the parent's specific named
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
The original host CLI migrates the binding without changing settings bytes. One
native owning destination stdio Session supplies the actual dashboard; a second
old owning Session verifies the retired authority. This is not `serve-http`.

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

## Evidence and remaining gates

Only `validation/ci-host-document-history-browser` is uploaded. Source maps,
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

Pure mocked/static adversaries are authored in `tests/test_host_history_browser.py`.
They cover malformed/oversized/duplicate specs, producer freshness and exact
identity, admission order before image access, capability redaction, paths and
cleanup-to-success forgery. **They have not been run in this source-only task.**
No native build/test, project Python/npm test, browser, Node driver or worker has
been executed. npm typecheck/test, original three-variant history gate, new actual
Edge/CDP integration, screenshot/visual review, broader Serena/persistent-Python/
HTTP browser transports, native whole-project CI and release gates remain open.

Protocol design references: [CDP domains](https://github.com/ChromeDevTools/devtools-protocol/tree/master/pdl/domains)
and [upstream pipe stdio launch contract](https://github.com/puppeteer/puppeteer/blob/main/packages/browsers/src/launch.ts).
The harness uses neither upstream implementation as a runtime dependency.
