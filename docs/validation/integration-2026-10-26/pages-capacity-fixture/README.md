# Page-revision capacity fixture preparation

Source: `33dfe1db43fe0d1a13b2580328035ee15193c6be`.

The existing test seeds 999 or 1000 signed foreign App revision records before
previewing and storing the current App's page. Each row previously committed
separately. The candidate batches only those fixture inserts into one SQLite
transaction, committed before preview. Keys, serialized signed values and row
counts remain exact. The original fixture deadline still starts before setup;
no request deadline, production transaction, capacity or assertion changes.

The earlier [Windows shard 1 artifact](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40376352865/artifacts/12679001955)
contains 101 passed functions, one failure and four existing ignores in the
whole `github_app_publication` group. The failure is the capacity fixture's
preview, which reports `Request deadline exceeded`. This demonstrates the expiry
location; it does not establish that fixture commit frequency was the sole cause.

The eight retained Windows artifacts use actual PR merge
`5a94df35d1de344b6dc08c5b1a69fc42fff1e2f8` for head `47142096`. An in-memory
text-only audit verifies their ZIP digests, matching source maps and inventory,
369 unique complete group entries, and all 738 log lengths/hashes. There are
368 zero exits and one nonzero group. The actual shard acceptance gate remains
failed; these counts do not infer combined function totals or current acceptance.

Scoped Rust 1.94.1 formatting, diff checks and independent source review pass.
Actual `npm run typecheck` and `npm test` pass on the checked candidate bytes:
269 total, 266 passed, three original skips, zero failures. All 1,136 selected
inputs and controller pins remain unchanged. Both owned roots exit with empty
Jobs before forced cleanup and complete stdout/stderr. The committed fixture
matches its checked source hash.

No local Rust build/test, native image read or execution, external publication,
release or full-project qualification is claimed. The complete Windows
`github_app_publication` target and affected hosted shard must still pass on
this candidate with their original deadlines and assertions. All prior failed
diagnostics and protected native producers remain retained privately.
