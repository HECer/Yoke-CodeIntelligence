# Scoped macOS cleanup evidence and timing/profile fixture candidate

Collected on 2026-10-29 UTC. Full native, browser, integrated release and main
acceptance remain false.

## Actual hosted macOS results

[Rust run 40950941727](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40950941727)
tests PR8 merge `788913c0877afa5269527b71ebeffed52d05732d`. Its Git tree
`3bd5cc75b4df6aa525780127a79fa6f4dcb7a2d3` equals candidate
`f02195852470c3f6ea9f749c3a9422f17b3edb9c`. All eight macOS receipts agree
on inventory and source map. All 372 unique default Cargo groups occur exactly
once; 360 exit zero and 12 fail. All 744 log lengths and SHA-256 hashes match.
All 1,109 selected source hashes match candidate Git bytes exactly, totaling
18,089,515 bytes. Build-only groups remain build-only evidence.

The actual library passes 506 tests with six original ignores. All 12
`process::cleanup_tests` pass, including the Darwin exit-between-poll-and-signal
race, unreaped exit/status, surviving descendants, complete-output refusal and
missing raw receipts. The process integration target passes five tests; runtime
passes 15. This qualifies those cases at this exact PR8 tree, not complete macOS
or later-source acceptance.

The 12 failing groups retain their failures. Forty-three PDF fixture tests report
`ResourceLimits / SetAddressSpace`, errno 22, at 268,435,456 bytes. Four direct
template-worker cases explicitly report failed RLIMIT_AS installation. Sixteen
other cases report generic spawn EINVAL; five have downstream/generic
initialization errors. Those generic cases are not independently attributed to
memory installation. The remaining failure is the ingress stimulus below. The
owned C diagnostic on Darwin 25.6 arm64 independently observes failed 256 MiB
installation with errno 22, with and without nano malloc disabled. Its eight logs
match. This establishes no conforming macOS memory backend.

## Ingress stimulus correction

In the failing ingress trace, the first write succeeds at about 0.214 ms, the
reader sees EOF at 134.256 ms, and the requested 25 ms sleep wakes at 168.899 ms.
Only one byte was sent. The fixture did not establish its required continuous
peer.

The candidate retains the 120 ms server deadline, 25 ms drip and 60-write cap,
at least two successful writes, clean closure and no durable records. A fixed
60-slot thread-owned observation additionally requires write gaps and the final
write-to-EOF gap below 60 ms. Inadequate scheduling stimulus may make at most
three sequential requests against the same Fixture. Retries require clean EOF
after at least 120 ms and remaining budget. An early unpaced close, invalid
response, read error or exhausted budget cannot retry. One 750 ms acceptance
timer covers all connections, writers and joins, starting earlier than the old
single-attempt timer. Socket reads use only its remaining time. All requests
share the same durable-state check. No production ingress behavior changes.

A fresh std-only Rust comparison compiles the copied fixture and trace test
against a fixed standalone loopback peer. Its sole alteration to the copied test
is replacing the sleep call with a controlled delay injector. Normal pacing and
an injected first 170 ms sleep both pass two tests. A peer that renews its deadline
fails at the 750 ms read boundary; a 900 ms stall, invalid response and early
unpaced close also fail. Each negative trial has one passing trace test and one
failing socket test. This is executed comparison evidence, not an executed
mutation or test of the application server. The standalone peer has no database
and does not qualify the durable-record assertion.

The final comparison uses pinned Rust 1.94.1 and its LLD linker, in a fresh private
package without dependencies. Its worker is created suspended, assigned before
resume and checked for membership in that exact named Job. The worker exits
zero; the tree is empty before forced cleanup and afterward. Original
link/output/disk checks remain. The comparison shortens the parent's 180-second
worker deadline to 20 seconds. Only the fresh owned comparison image is admitted
and hashed. No Yoke image or PDB is accessed locally.

## Private Windows profile candidate

An earlier local comparison reproduces a junction at
`home/AppData/Local/Microsoft/Windows/INetCache/Content.IE5` without launching Edge
or Yoke. The original guard refuses it and empties the Job during cleanup. Its
origin is not established. An earlier Edge-only blank-page trial had no links.

The candidate precreates that exact compatibility name as an empty ordinary
directory inside the fresh private profile. All ancestors are checked; all later
links, including a replacement junction at this name, still refuse. No existing
user directory changes and no link target is read. Two regressions verify
ordinary creation and same-name junction refusal. The final owned comparison
passes the scan. Its linker also differs from the first failed trial, so this is
not an isolated causal proof of the profile change. Hosted browser qualification
must confirm the actual CI route and repair. No screenshots or dashboard visual
states are accepted here.

The other retained development attempt had a nonblocking standalone accepted
socket and returned prematurely. That peer was corrected to blocking mode for
the final comparison. Its parent also refused incomplete owned-tree exit. Both
failed Job receipts remain; neither is accepted.

## Checks and retained text

The candidate passes 180 Python tests, scoped Rust 1.94.1 formatting/source
parsing and whitespace checks. `npm run typecheck` passes; `npm test` passes 266
tests with three original skips. TypeScript inputs are unchanged by the fixture
and Python edits. All other ingress fixture bytes remain exact.

The full modified native target, application build/Clippy, hosted profile repair,
six desktop/mobile dashboard states, matching VSIX/extension host and integrated
release remain unrun or incomplete. No memory contract, permission or original
acceptance ceiling is relaxed.

[Receipt](receipt.json), [member index](index.json) and
[raw text/source evidence](text-evidence.zip) retain 787 UTF-8 members, totaling
2,904,622 uncompressed bytes. The 966,269-byte archive has SHA-256
`57e0970466ef6af9f192a1e13f712e33b134f59ff7aea963661edd356139a644`.
Every member was reopened and checked against the index. No executable, PDB,
browser profile payload, credential or dashboard capability is included.
