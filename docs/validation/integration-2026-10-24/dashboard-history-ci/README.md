# Code Intelligence 0.5.0: owned host-history CI gate

Product `9c32ef68afef7adaf21586ff586b11f0001748e0` is pushed; all three changed
Git blobs are independently fetched and matched. The [workflow](../../../../.github/workflows/rust.yml)
retains its original prefix exactly, including complete native tests, permissions,
45-minute job bound, release build and ordinary release artifact upload. A new
Windows-only step then runs [the driver](../../../../scripts/host_history_live.py)
with a five-minute step bound; its owned worker has a180-second bound. The
separate `host-document-history-windows` artifact is collected even on failure,
without claiming that missing evidence passed. Native stderr stays in private
temporary fixtures outside the uploaded evidence tree.

The driver selects only the just-built release in explicit Windows CI with the
actual GITHUB_SHA matching the checkout. The worker is created suspended and
assigned to a kill-on-close Job before resume. Private HOME/profile/temp roots,
bounded JSON/stdout/evidence/source inventories, the unchanged500MB stop and
explicit empty-Job proof constrain the run. No account credential or user
configuration is selected. Capabilities are never transcribed by the driver.
Actual checkout revision and executable/source/validator hashes identify the
producer; selected source hashes are not a complete build-input inventory.

Three actual planned variants use native, Serena and persistent Python stdio
Sessions with real private HTTP management. Each creates six explicitly saved
closed journals, migrates the old binding, retains cached proposals in old and
new owning Sessions, and tests both authorities while a foreign completion path
leaves a real archive operation pending. Exact file/owner/journal bytes must
remain on refused saves. A stale event refuses; explicit reviewed resume fixes
completion. Two separately reviewed whole-journal packages are independently
decoded against the original raw bytes and every record read back through the
real CLI. A fresh review in the same destination host then saves with the current
owner generation in its actual journal. Archives remain exact, old authority
stays retired, Python45 remains live and own exit is proved before stdin EOF
and Job cleanup. There is no native browser claim.

**Only the admission layer has run at this checkpoint:**11 pure Python tests
passed /0 failures. They cover exact Unicode/CRLF bytes, corruption/truncation/
lengths/trailing data, whole-journal membership, duplicate JSON, bounded reads,
unsafe record/source paths, private endpoint admission, foreign fixture
preservation, non-CI refusal before native reads and the internal worker's disk
preflight. No local native execution was attempted. A real source-inventory
preflight records1071 selected files. All nine prior archive product inputs
remain exact; previous694 native and260/3 npm counts are not reassigned or added.

Actual hosted execution, complete archive integration/live acceptance, native
browser acceptance, current VSIX/release and full STATUS remain pending. The
local volume still lacks the unchanged native reserve. This gate makes that
remaining test executable in the existing CI; it does not make an unexecuted
test a passed test.

[Local result](result.json), [raw Python output](python-tests.stderr),
[selected source hashes](selected-source-inputs.json),
[remote product verification](remote-product.json),
[documentation-only proof](post-validation-docs.json),
[unchanged preceding reconciliation](reconciliation-proof.json).
