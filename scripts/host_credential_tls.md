# Separate Windows credential-backed TLS acceptance

`host_credential_tls.py` adds explicit hosted-CI execution of the existing
`digest_receiver` test
`actual_tls_receiver_and_sender_cli_recover_using_os_credentials_and_selected_ca`.
The original Rust fixture, its ignore, assertions, 20-second CLI lifetime and
three-second exit bound are unchanged. Default native shards keep their whole
targets and original ignored cases. This profile records its one selected case
and filtered count separately; it cannot establish full target/platform coverage.

The existing fixture creates three random, uniquely named Windows credentials
for its own token, TLS identity and signing key. It uses private temporary stores
and loopback TLS services, checks sender acceptance/recovery and the separately
authorized remote metadata query, and deletes its three credentials on return.
Successful libtest completion includes its existing cleanup assertions. It does
not select existing user credentials, user editor profiles or external services.
Credential values are not arguments or acceptance JSON. A failed/terminated case
does not establish successful credential cleanup; the dedicated hosted runner
remains responsible for its ephemeral logon/VM lifecycle.

## Source and producer admission

Both operations require 64-bit Windows GitHub Actions, the matching `GITHUB_SHA`,
the controlled TLS environment and the existing runner-policy inspection.
`prepare` creates separate, previously absent output/build namespaces and runs
the fixed locked Cargo `--no-run` command. Cargo must report one fresh native
`yoke` CLI and one fresh `digest_receiver` test image for the selected workspace
package, in the expected output roles, followed by successful build completion.
Other paths, duplicate roles, cached role images and mismatched source refuse.
The producer has a 30-minute subprocess timeout inside a 35-minute CI step.
It is a compilation phase, not the runtime Job proof.

## Explicit owned execution

`run` creates a fresh private fixture/home/temp namespace. Its launcher is copied
byte-exact and AST-exact from `host_history_live.py::bounded_worker`: the worker
starts suspended, joins its named kill-on-close Job before resume, and retains
the 180-second deadline, disk/output checks, one-second accounting drain and
checked cleanup. Membership in that specific Job precedes any worker image read.
The selected fresh image metadata and hashes are checked around execution;
source pins are checked around production and runtime. These checks are evidence
for the private CI producer, not independent Windows file-sharing qualification.

Only the admitted test image receives the fixed `--exact`, `--ignored` and serial
arguments. A 120-second outer case timeout supplements the original fixture
limits. Acceptance requires exactly one successful, nonignored case, matching
logs/source, and natural root/tree exit before forced Job cleanup. The main
`result.json` stays false on failure. Partial logs remain available in the
separate text-only `host-credential-tls-windows` artifact; compiled images and
private fixture files are outside that upload directory.

The producer, source hashing, metadata and parent receipt reads use cooperative
checks/CI timeouts. They are not an interruptible-filesystem or whole-host
sandbox guarantee. Runtime Job enforcement and the actual Credential Manager
case must still be qualified on hosted Windows. Synthetic admission tests do
not establish that execution or general TLS/backend, VSIX/main/release acceptance.

## Local checks

`python -m unittest scripts.tests.test_host_credential_tls -v` uses fabricated
Cargo/receipt data, owned text and mocks at every native boundary. It does not
compile or execute Yoke, read native image payloads or access credentials.
Production operations refuse non-CI use before producer/namespace/image access.
