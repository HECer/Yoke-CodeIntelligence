# HTTP owning-Session browser candidate

Baseline: `d6b9da55be23f3c7810292bc3412d67884b664cc`.
This source-only candidate adds a fourth explicit browser transport variant,
`--variant http`. No actual HTTP/browser/Credential Manager acceptance is claimed.

[Review receipt](review.json) records the preparation checkout's raw source
hashes, 79 passing focused tests, and six deliberately failing in-memory
check-erasure cases. All native, HTTP and credential peers in those tests are
mocked; inert Job fixtures remain distinct from product execution. The ABI field
order and `CRED_PERSIST_SESSION == 1` declaration were checked against installed
Windows SDK 10.0.26100.0. No existing Yoke image/PDB was read or executed.

The [actor](../../../../scripts/host_history_browser_http.py) starts only inside
the admitted Windows CI worker Job after the wrapper's unchanged producer,
source and native-image admission. It calls the existing public `serve-http`
CLI with fixed loopback Session-profile principals and host-owned source/document
configuration. It creates no new native production route or permission switch.

Each actor creates a fresh random-named generic credential with logon persistence
and an independent 32-byte random token. Creation shares the native store's
fixed named mutex and refuses existing roles. Exact name/type/scope/bytes are
read back; deletion rechecks identity and verifies absence. No existing user
credentials are read or enumerated. Only references enter private fixture config;
secrets stay in RAM and the OS store. Mutable copies/read buffers are cleared,
but secure erasure of CPython/OS copies is not claimed. Handled teardown attempts
all owned resources and refuses uncertain cleanup. A forcibly terminated worker
may retain its own logon-scoped entries until ephemeral runner logoff; no successful
receipt can result. Equal-privilege hostile-process containment is not claimed.

MCP requests use fixed IPv4 loopback `/mcp`, exact bearer/session/version headers,
16 KiB/48-field headers, 1 MB bodies, Content-Length plus real EOF, and one
absolute ten-second request deadline. JSON and at most 32 tool-catalog
notifications plus one response in finite SSE retain exact IDs/Unicode. Duplicate
keys/headers, truncation, extra data, redirects and renewed deadlines refuse.
Actual missing/wrong bearer, foreign Origin and foreign transport Session must
refuse before proceeding.

The same real Edge driver executes all original desktop/mobile draft, pending,
refusal, archive-readback and guarded-save checks unchanged. HTTP shutdown must
revoke only the current MCP Session, verified by 404 while its process remains
alive. The old Session's fingerprint and real tool catalog must survive. Both
Sessions close before service-exit validation. Both service roots then exit
naturally after their fixed 120-second public CLI duration, with raw zero status,
stdin still open, stdout EOF and the stopped receipt. Exact credentials must be
removed before acceptance. Session acknowledgements and Job force cleanup
cannot substitute for these observations.

The copied Job runner, stdio Browser transport, six-image verification, original
history script and CDP driver are unchanged. Existing CI jobs/steps/commands and
limits are preserved; HTTP has a separate four-minute step, original 180-second
worker and exact fresh evidence directory. Each of its six PNG/DOM pairs is
required independently. Automated human/visual/visible-shell, all-transport and
release flags remain false.

The separately integrated ingress fixture is byte-exact to source `99b73b3f`
that passed its 22-test target on all three platforms in
[PR8 run 40986285723](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40986285723).
That evidence retains its original identity and does not qualify this combined
source. Fresh whole-pipeline CI, real HTTP credentials/lifecycle and all six
screenshots require independent review. No native production build/test was run
locally for this candidate; scoped ingress rustfmt/source checks pass.
