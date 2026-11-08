# Successful native stdio browser checkpoint

[Diagnostic job 123868515540](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40995799274/job/123868515540)
completed successfully at source `ed92e9f818ef5662f7c597bb4809f631bad15d0e`.
It built and admitted the release CLI, passed the original three owning-Session
history variants, and passed the actual Edge/CDP browser exercise. The browser
was installed headless Edge 155.0.4283.62 with Node 24.22.1 and inherited CDP
pipes in a fresh private profile.

This qualifies one native owning stdio Session dashboard flow at desktop width
1440 and mobile width 390. The assistant visually inspected all six actual
images and checked their matching DOM observations:

| State | Desktop | Mobile | Inspection |
| --- | --- | --- | --- |
| Before review | [PNG](desktop-before.png) | [PNG](mobile-before.png) | Original and draft content readable; reviewed Save available. |
| Pending archival | [PNG](desktop-pending.png) | [PNG](mobile-pending.png) | Exact draft retained, Review/Save disabled, visible recovery focus ring and archival blocker, no recovery actions. |
| After host resume/save | [PNG](desktop-after.png) | [PNG](mobile-after.png) | Fresh explicitly saved content and success notice; no pending journal. |

Long paths and hashes wrap on mobile. No visible clipping, overlap or horizontal
overflow requires a layout correction. [Agent visual review](agent-visual-review.json)
records this inspection independently. The original automated receipts retain
their false human/visual-review flags; no captured result was rewritten to
claim human review or visible desktop-shell interaction.

The real browser traffic goes to the unchanged native dashboard. The fixture
verifies both original authorities refuse while archival is pending, blocked
actions make no mutation, all 14 selected original archive records read back,
and both fresh UI saves retain the original owner generation and exact target
readback hashes. The analyzed source remains unchanged. The two pages record
zero script/console errors. Browser close, raw browser/driver exit zero, both
output EOFs, and both native authority exits precede forced cleanup. The worker
Job exits zero with an empty tree before cleanup and afterward.

The separate unchanged owning-Session harness also passes native, Serena and
persistent-Python variants, each with two packages and 42 original records.
Those are protocol/history results, not additional browser variants. Its full
498-member text artifact and independent exit receipts are retained here.

[Index](index.json) pins all 23 retained files and every archive member by exact
size/SHA-256. Both selected-source maps agree: 1,099 entries match source Git
blobs, with 45 raw and 1,054 LF/CRLF-only Windows checkout matches. Validator,
driver, served dashboard and producer revision match those admitted inputs.
The browser and original history receipts agree on the same native image hash;
the producer image timestamp falls inside the successful build interval. Local
auditing reads text and PNG evidence, not Yoke executable/PDB payloads.

This acceptance is limited to that exact source and the native stdio browser
flow. Serena, persistent-Python and HTTP **browser** flows, visible-shell/manual
interaction, complete current native/platform suites, strict macOS memory
enforcement, matching VSIX/extension host, main and release/publication gates
remain separate. It does not establish full project or release acceptance.
