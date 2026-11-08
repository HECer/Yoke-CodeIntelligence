# Sustain actual socket backpressure through the original response deadline

The verified macOS shard2 artifact at source84e11866 records the library
backpressure test failing its timeout assertion. Its original fixture fills a
real bounded outbound socket until WouldBlock, then sends a tiny response.
That instantaneous observation does not guarantee sustained pressure while
in-flight bytes can move into an unread receive window. Other failures in that
library group retain their separate status; this checkpoint claims no group pass.
The archive digest, selected log length/hash and exact old test source are
reverified from the retained artifact. [Prior macOS proof](prior-macos.json).

The cfg(test) fixture now sends a4-MiB response instead. The original8-KiB socket
buffer settings, observed WouldBlock assertion,64-MiB maximum fill,250-ms response
deadline,3-second wait/elapsed assertions and peer-shutdown cleanup all remain.
Only the fixture body and failure diagnostics change; the production transport
prefix is unchanged. Diagnostics retain the actual optional failure status,
queued byte count and elapsed time without response content.

An actual [owned Python socket probe](inert-socket-probe.py) on this Windows host
uses two private loopback connections with the same socket-buffer settings and
the same bounded prefill. After observed WouldBlock, an additional40 bytes finish
in about0.0003s; the4-MiB write reaches its250-ms deadline (about0.263s measured).
Both connection pairs close through scoped ownership. No thread, child process,
native Yoke image, analyzed code, model or existing credentials is used. These
are Python/kernel fixture observations, not Rust serialization or macOS evidence.
[Actual observations](inert-socket-probe.json).

Changed-file rustfmt passes. The whole Python suite passes103/0/0 with warnings
treated as errors; typecheck and npm260/0/3 existing skips pass. Exact source/log
hashes are in [result.json](result.json). Local PowerShell log displays use UTF-8,
LF and trimmed trailing whitespace; original capture hashes remain separate.

No native compilation/execution occurs locally below the1.2-GB full-build
admission floor. The changed Rust test has not yet run. Hosted confirmation,
other macOS memory/runtime/timing failures, Windows runtime checks, archive live,
current VSIX and full STATUS acceptance remain open. The probe is not counted as
a native test or a completed platform gate.
