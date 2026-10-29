# Source-bound native and owning-Session history checkpoint

At source `76035e4be3d9a9f9984364190eae9b49764a4209`,
[run 40858654238](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40858654238)
has complete default Cargo group coverage on Windows and Ubuntu. Each platform
has eight complete, distinct shard receipts and 371 unique groups, exactly
matching its recorded inventory. Every group exits zero; all 742 referenced
stdout/stderr files per platform match their recorded lengths and SHA-256 hashes.
Build-only examples remain build-only evidence. Individual test totals, ignored
gates and nondefault features are not inferred from group counts.

Each platform retains 1,099 selected source hashes. Ubuntu matches the committed
Git bytes exactly. Windows has 45 exact matches and 1,054 differences consisting
only of LF/CRLF conversion. These are distinct byte identities, explicitly
reported rather than described as raw-byte equality. The instrumented original
nonreading-client HTTP test passes in the Windows evidence; that observation
does not identify the cause of its earlier intermittent failures.

At separate source `8f6878ae0da9197df268cac45f24fed4b24b2891`,
[the Windows history diagnostic](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40864175630/job/123414606506)
passes the unchanged release build and original three owning-Session history
variants: native, Serena and persistent Python. Each variant completes two
archive packages and reads back all 42 original records. The receipts require
pending-state authority refusal, unchanged configuration/owner during archival,
a fresh same-host save, the actual current journal generation, immutable frames
and unchanged analyzed source. Both native authorities exit zero while stdin is
still open and before Job cleanup. Persistent Python separately proves runtime
exit before cleanup. The outer worker exits zero with an independently empty
assigned Job before forced cleanup. No model call or analyzed-source execution
is claimed.

That diagnostic job subsequently fails the separate browser gate at the strict
private-fixture link check. Its overall failure remains a failure. There are no
accepted browser PNGs or desktop/mobile visual results. The source-only link
classification at `43fc1b73` preserves refusal and observes `area=home`,
`entry=other`, `kind=junction`. That run again passes the release build and
original history variants, then fails browser acceptance. The assigned worker
has no successful exit receipt; forced cleanup leaves zero active processes.
There are no PNGs. [The supplementary receipt](browser-link-receipt.json) and
[exact text artifact](browser-link-43fc1b73.zip) preserve that failed attempt,
its source map and the closed parent diagnostic. The junction's exact entry and
origin remain unknown. The coherent private-profile layout at `83227913` also
fails on a home junction, at depth seven, with no PNGs or successful worker-exit
receipt; forced cleanup leaves zero active processes.
[That diagnostic job](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40956689835/job/123738494312)
does not establish the cause or an accepted fix. Bounded route categories and
opaque per-component fingerprints are the next diagnostic candidate. No link
target is followed or exempted.

[The receipt](receipt.json) binds the source revisions, native coverage,
owning-Session observations, original GitHub artifact identities/digests and
successful build/history steps. [The member index](index.json) records every
retained file length and hash. [The raw text archive](raw-text-evidence.zip)
contains all 2,000 selected native/history text receipts and logs, totaling
5,571,255 uncompressed bytes. Its SHA-256 is
`66678b3b471e5f86faf4096f18f807a57a0c652eb5e0054988b76f64144b1870`.
Every member was reopened and checked after compression. No EXE, PDB, DLL or
other executable payload is included or read during this audit.

These are source-bound checkpoints. Later workflow/browser changes do not gain
full current-head acceptance from unchanged native source. Strict macOS
address-space enforcement, browser interaction and six actual visual
inspections, the integrated PR stack, matching VSIX/extension host, main,
publication and full project/release acceptance remain open. No memory limit,
deadline, permission, assertion, ignore or recovery requirement is relaxed.
