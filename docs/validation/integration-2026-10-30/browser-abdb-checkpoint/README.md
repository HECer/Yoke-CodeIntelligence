# Source-bound native, Serena and HTTP browser evidence

[Diagnostic job 123922975632](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41013033443/job/123922975632)
tests source `abdbdb0308aee2acec821f9f3da710cbb1976535`. The job is failed because
the persistent-Python browser invocation failed. Its successful sibling steps
retain their own evidence and do not make the complete job pass.

[Original artifact/member hashes](artifact-index.json) cover five exact ZIPs,
563 PNG/text-only members and 2,201,874 compressed bytes. ZIP SHA-256 values match
GitHub's artifact digests. Every member was reopened and hashed; names, types,
sizes and extraction paths were admitted before reading. No native executable,
PDB or compiler payload was downloaded or opened. The artifacts are:

- [Native browser, 12959274216](12959274216.zip): successful automation and six PNG/DOM pairs.
- [Serena browser, 12958979925](12958979925.zip): successful automation and six PNG/DOM pairs.
- [HTTP browser, 12960152034](12960152034.zip): successful automation and six PNG/DOM pairs.
- [Persistent-Python browser, 12959169660](12959169660.zip): failed automation and only three PNG/DOM pairs.
- [Original three-variant history harness, 12959274185](12959274185.zip): complete, with 42 original records read back per variant.

[The audit and separate agent visual review](audit.json) bind all five equal
[source maps](source-inputs.json) to the tested Git revision. All 1,101 selected
inputs match: 45 raw Git bytes and 1,056 explicitly identified LF/CRLF-only
matches, totaling 18,082,590 committed blob bytes. Line-ending equivalence is
disclosed; this is not a claim that all hosted raw bytes equal Git blobs.

All 18 successful PNGs and the three available Python PNGs were individually
viewed. Successful desktop/mobile states show readable Unicode draft and
original content, retained drafts with disabled Review/Save controls during
archival, a visible keyboard focus ring on Inspect recovery, host-only recovery
explanations, and fresh saved content with no pending journal afterward. No
blocking clipping, overlap or horizontal overflow was observed. Missing Python
states were not inferred. Automated human/visual flags are preserved unchanged;
agent review does not claim human acceptance or visible desktop-shell activity.

The successful profiles each prove raw worker exit zero, an empty named Job
before forced cleanup, browser/driver exit zero and their pipe EOFs. Their old
and current native authorities exit before Job cleanup. HTTP also proves actual
401/401/403 authentication/origin refusals and foreign-Session refusal for both
principals; stopping one owning Session returns 404 for it while services and the
peer survive. Both services exit naturally at the fixed 120-second host lifetime,
with stdout EOF and exact owned-credential removal before Job cleanup.

The original history harness separately confirms two packages, all original
records, source/configuration/owner preservation, fresh current-generation saves,
retained Python value 45 and runtime exit before cleanup for its Python variant.
Those protocol results do not qualify the failed Python browser sequence.

## Pending-click focus correction

The Python worker reports `checkpoint=pending-click; viewport=mobile`, exits 1
and retains desktop/mobile-before and desktop-pending images. It has no complete
automation receipt. Its Job becomes empty before force but does not have a
successful root receipt; the failed gate remains failed.

The app's `manage()` synchronously disables the initiating button, which may
move focus to the body before CDP acknowledges the input. The old driver then
waits for focus that the app may already have removed. A controlled reproduction
runs the actual pending-phase orchestration, click and keyboard helpers with
CDP/DOM/network doubles: the old driver fails when input immediately blurs; the
correction passes with and without blur. This establishes the source race and is
consistent with the hosted checkpoint; it does not identify the exact host CDP
exception or replace the missing real run.

The correction observes focus through bounded real Tab input after each actual
reply. It preserves both exact pending receipts, disabled editor/Review/Save
assertions, keyboard skipping, final focus and screenshot checks, network
admission, 180-second worker, four-minute step and all resource/lifecycle guards.
Missing targets/replies, invalid blocking state, enabled Review, disabled or
unreachable controls still refuse in the actual-phase doubles.

[Checks and log hashes](checks.json) retain the causal RED (one expected failure),
13 passing Node protocol tests, 80 passing focused Python tests, passing typecheck
and npm tests (266 passed, zero failed, three skipped). The npm command succeeded;
the display helper's subsequent console-encoding failure is disclosed separately.
No local Yoke native build/test or real-browser rerun occurred.

Fresh hosted execution and all six Python images remain required for the focus
correction. Native/Serena/HTTP evidence remains source `abdbdb03` evidence. Strict
macOS memory, complete combined platform qualification, VSIX/extension host,
human acceptance, main/publication and full release acceptance remain open.
