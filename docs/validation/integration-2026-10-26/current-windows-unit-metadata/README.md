# Current Windows unit and metadata validation

Actual native source: `b3a87d5d5de32668ddf7f36f230424e5f3a3a7ca`, Windows x86_64,
2026-10-26. This closes the previously unrun local library and Clippy checks for
the HTTP completion diagnostic. It does not close macOS diagnosis, integration,
release or full-project acceptance.

| Check | Actual result |
|---|---|
| Whole fresh library harness | 773 passed, 0 failed, 7 original ignores, 0 filtered |
| Whole fresh CLI unit harness | 16 passed, 0 failed, 0 ignored, 0 filtered |
| Workspace/all-target Clippy, `-D warnings` | Passed; 757 artifact observations are metadata |
| New metadata-controller protocol/helper guards | 116 passed, 0 skipped; not native runtime coverage |
| Actual metadata-only protected-role preflight | 10 roles checked, 9 present and 1 absent |
| Typecheck and npm | Actual source `771833bb`: passed; npm 260 passed, 0 failed, 3 original skips |

The Node commands were not rerun: all 1,135 selected raw source files and seven
original controls from their actual receipts match `b3a87d5d`. This proves selected
input equality, not a frozen environment, dependency cache or a new test run.
[Original Node receipts](../editor-http-response-completion/README.md) retain
their actual source identity. Historical `704b647d` release, archive, VSIX and
extension-host evidence remains historical and is not relabelled here.

## Guard correction and retained failures

The original schema-5 controller refused before Cargo because a protected EXE's
path stat reported mode `0100777`, while stat of its execute/attribute-only held
handle reported `0100666`. Identity, length, timestamp, link count and Windows
attributes agreed. This is a mode-domain mismatch, not evidence of changed bytes.
The local trusted interpreter reports CPython 3.14.7. Its published
[path-stat implementation](https://github.com/python/cpython/blob/v3.14.7/Modules/posixmodule.c#L1915-L1937)
adds executable suffix bits; the
[handle-stat implementation](https://github.com/python/cpython/blob/v3.14.7/Python/fileutils.c#L1005-L1016)
and [attribute mapping](https://github.com/python/cpython/blob/v3.14.7/Python/fileutils.c#L1148-L1196)
derive regular-file permissions from the read-only attribute.

A separate schema-6 control bundle retains the complete byte-exact 24-file
schema-5 snapshot. It keeps both raw mode observations and checks each throughout
its lifetime. Cross-view identity, size, time, links and attributes must match;
the path/handle modes must satisfy the exact known EXE or PDB relation. Unknown
modes, malformed types, changed routes and unavailable observations refuse.
No permission bits are discarded, no executable bytes are read by this check,
and it does not attest ACLs, executable content or all hard-link aliases.

All 61 original protocol test-method bodies remain intact (72 effective cases).
Two protocol cases and 42 helper cases bring the new suite to 116. Static checks
also retain 69 unchanged top-level policy functions/classes outside the explicitly
listed metadata/control-map changes. The immutable preparation manifest still
describes its original pre-execution review state; actual results are the separate
dated receipts in this packet.

The first schema-6 build was stopped by the unchanged 500,000,000-byte free-space
reserve, with 444,710,912 bytes observed. Its forced cleanup emptied the owned Job;
it remains a failed build, with no admitted image. A separate post-refusal check
confirmed unchanged source/control/protected observations.

A separately leased, exact inventory then removed only 1,386 untracked,
single-link reproducible Cargo files (`.rlib`, `.rmeta`, `.o`, `.obj`, `.bc`) from
the historical RootB checkout's `target`. It removed 2,163,447,333 logical bytes
without payload reads; all directories and 8,829 noncandidate target files stayed
unchanged. Native images, PDBs, controls, sources, evidence and deliverables were
preserved. The private full deletion manifest is retained with SHA-256
`473534a8609f2f1532e202dd02f489bebf38991250b87e1986fa961002ef3804`.
The later fresh producer has its own run ID; it does not turn either failure into
success.

## Actual native acceptance

Each accepted suite used its own fresh matching producer and a private selected
test image. The library image is 156,569,088 bytes, SHA-256
`ad21938fbfb7ff62bb3ca86e6909fcd5567a7968b3c965e19d2664b09d84afa8`;
the CLI unit image is 104,839,680 bytes, SHA-256
`d5b24f052d7b83eb4524eea720d701055098aca18e3bd8d2415416b66512427a`.
These are test harnesses, not release CLI qualification.

Both consumers ran whole harnesses with only `--test-threads=1`. The exact seven
library ignores are retained in `checks.json`; none was promoted to a pass.
Selected 1,050 native/build source inputs, 50 new controls, seven original controls,
protected metadata and selected image checks remained stable through final checks
and context closure. Streams completed and owned roots/Jobs exited naturally
before forced cleanup. The inner build quiescence allowance is not a test deadline
extension. Disk, output, image, original profile, absolute build/test deadlines and
cleanup gates remain unchanged. Installed tools, external dependencies and global
caches remain trusted inputs, not a sandbox against equal-privilege processes.

## Packet and scope

[checks.json](checks.json) records the actual counts and false broader gates.
[index.json](index.json) lists all 413 [ZIP members](evidence.zip), lengths,
SHA-256 values, origins and representation. Expanded size is 7,751,788 bytes;
the 1,976,191-byte ZIP SHA-256 is
`9f72f3832761e7c36c514383630da49154011eb22ae1048ffa4f9e6aef00a9e8`.
All members were reopened and verified against the index.

The ZIP contains only source, logs and metadata. Native image/PDB/cache payloads
are excluded. Named Job nonces are redacted in explicitly labelled public views,
with original byte lengths and hashes retained; private worker specifications are
omitted. Full original producer proofs were held and admitted locally. This public
packet is not itself a complete executable producer bundle or an authorization to
adopt an old image. Redundant full Git listings and the full private cache deletion
inventory are omitted with recorded hashes.

Same-source hosted shards, integration targets, doctests, actual language servers,
current live history/maintenance/browser checks, matching release/VSIX,
main/publication and broader inventory qualifications remain open. The known
macOS strict address-space-limit refusals, original real-time history fixture and
HTTP completion-path diagnosis remain open. No production runtime repair is
claimed by these Windows unit results.
