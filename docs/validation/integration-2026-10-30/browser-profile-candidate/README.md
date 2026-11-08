# Owning browser profile candidate

The selected source baseline is `ed92e9f818ef5662f7c597bb4809f631bad15d0e`.
The additive harness and workflow changes require fresh CI; no new Serena,
persistent-Python or HTTP browser acceptance is claimed.

[Review receipt](review.json) records source hashes, unchanged guard ASTs and
five deliberately failing in-memory erasure checks. It describes mocked Python
checks, not actual Yoke/browser runtime results. The focused Python suites pass
58 tests, including inert Windows Job fixtures and in-memory Node CDP checks.
Typecheck and npm tests pass on unchanged TypeScript/package inputs in the clean
primary checkout (266 passed, three skipped). Scoped diff checks pass.

The original host-history harness and CDP driver Git blobs are unchanged. The
independent Job runner, Browser transport, six-image validation, producer/native
admission, CI identity, private profile and Node-path guard ASTs are unchanged.
No Yoke native image was read or executed by these checks. The locally installed
PyYAML 6.0.3 review helper is outside the project; it parsed the workflow and
verified preservation of all original jobs, steps and limits. It is not a product
or runtime dependency.

The new fixed variants select Serena for both owning actors. Persistent Python
alone explicitly grants the already running CI worker's unlinked host interpreter.
Both runtimes must retain 45 and independently exit after their owning Session.
The old Session/value must survive destination shutdown. A private nonce-bound
variant and exact fresh output directory prevent cross-profile receipt reuse.
Each run preserves the original 180-second worker, disk/log/image limits, and
four-minute CI step. Added steps still require successful release and original
three-variant history gates; a sibling failure cannot fabricate success.

After a successful hosted run, independently audit source/producer/Job receipts,
all original archive readbacks and UI guarded saves, both owning profiles and
Python identities/values/exits. Visually inspect all six PNGs for each new
variant and iterate on any actual defect. Automated human/visual/visible-shell,
all-transport and release flags remain false.
