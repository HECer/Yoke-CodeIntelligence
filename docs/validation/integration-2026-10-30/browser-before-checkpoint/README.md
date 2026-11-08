# Partial desktop/mobile browser evidence

[Diagnostic run 40974744304](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40974744304),
[job 123799884303](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40974744304/job/123799884303),
source `76333e2e74727d546c432b8da10dbb010091d091`, artifact `12944921471`.

The job reaches the actual guarded browser step and captures two before-review
PNG/DOM pairs. The worker then exits 1. No junction refusal is reported in this
invocation; the exact later failure phase is unresolved. This is failed, partial
evidence. Pending and after states, complete automation and normal browser/native
shutdown receipts are absent. Empty Job accounting before forced cleanup does
not establish successful natural shutdown on this failure path.

Both actual images were visually inspected:

- [Desktop, 1440 × 1612](desktop-before.png): readable draft, original content and
  review/save controls, with no obvious clipping or horizontal overflow.
- [Mobile, 390 × 2110](mobile-before.png): readable stacked controls and panels;
  long owned paths and hashes wrap within the panel, with no horizontal overflow.

This inspection covers only the two before states. Overall visual acceptance
and all harness acceptance flags remain false. All six states must be generated
and inspected in a complete later run.

[index.json](index.json) pins all eight retained artifact/audit files byte-exact,
including both PNGs and selected DOM observations. The 1,099 selected source
hashes match candidate Git blobs: 45 raw matches and 1,054 LF-to-CRLF-only matches.
Those Windows checkout bytes remain distinct from normalized Git bytes. The
validator and driver hashes agree with that source map. No native image/PDB
payload was read or executed during this audit; no full image or release
qualification is inferred from these partial files.

The next controller diagnostic adds failure-only fixed exercise-stage labels.
Original browser/CLI operations, assertions, deadlines, output limits, named Job
admission, cleanup and six-image requirements remain unchanged. The diagnostic
does not replace the failed run with a successful result or publish raw exception,
protocol, capability or endpoint data.
