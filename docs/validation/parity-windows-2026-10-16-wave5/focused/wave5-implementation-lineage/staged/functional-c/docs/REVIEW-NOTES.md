# Candidate documentation review

This directory is ignored staging. Nothing here is installed in normal docs.
The candidate describes the versioned contract, not a successful runtime gate.
Root owns review, promotion and release acceptance.

Proposed target: `native/docs/manifest-balanced-retrieval.md`.
`workflows-link.patch` adds one discoverability row and qualifies the existing
blanket statement that oversized results become artifacts. That statement
conflicts with the new hard retrieval output ceiling; other workflow behavior
is not being changed.

Normative inputs:

- CONTRACT.md SHA `4dc1105faa6731076dde6f0d3e5003624049a3392a4a7c1dafd1ba94f6fad53e`.
- API.md SHA `f94a6cde51979e4cc5e740eb39561533d0095e22360280d1e7158b0d7045b5f5`.
- Root's settled global-metadata rule: no invented per-scope scoring counters;
  if source omission rows are not emitted, the omitted-row count equals the
  known aggregate source-budget omission count.
- Root's reviewed policy precision: an unnamed npm marker is virtual at any
  supported same-root workspace boundary, including pnpm or Cargo, and any
  supported negative selector matching an actual workspace-root package is a
  conflict, not only the literal `.` selector. The initial candidate is kept in
  `revisions/manifest-balanced-retrieval.870f0f507c147c79.md`.

Implementation reads covered current native mode dispatch, adapter conversion
and logical-envelope guard, checked-run/deadline/error types, partition marker
policy, local scoring/fusion and metadata builders. A/B repairs are in progress;
these reads are not a frozen full implementation audit.

Known pending evidence is kept out of the product candidate. Root reported
retrieval library `71 PASS / 6 FAIL / 6 ignored`, including the known A policy/
range failures, Ready/global scope metadata failure and invalid-proof fixture;
the enhanced C shared-budget case reached its intended omission-counter RED
after all earlier 29 MB / 4 MB / 28 MB / shared 32 MB / code07 / no-artifact
controls (raw `88957e8accb7be0679b1dee65dd903906a92f6117efb503696f85c3ad8a83be0`).
No pending case is described as GREEN. No tests, Cargo, Node or project code
were executed by C for this documentation task.

The candidate deliberately makes no bound on all inherited snapshot decoding
allocations or adapter input reads. Its 32,000,000-byte ledger applies to checked
probe/scoring, and it distinguishes the result-data budget from the Trail/Graft
logical status/data envelope. It does not certify compiler/reference
completeness, upstream equivalence, universal parity, performance or deferred
Linux/macOS execution.

Normal protocol/trail/counter/profile files and installed pre-feature test stay
frozen. Existing ignored functional manifests are historical pins and are not
rewritten by this documentation task.
