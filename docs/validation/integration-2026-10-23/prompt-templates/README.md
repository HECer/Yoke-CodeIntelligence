# Explicit bounded native prompt templates

Code e0c9ff6e29ba515e652768090947a996d4de27de adds host-selected prompt rendering to session-config plan/export/
discover. A separate --allow-template-evaluation grant and templateContext JSON
are required. Only decoded context/mode prompt and project/local initial_prompt
scalars reach the pure linked MiniJinja 3.0.3 engine. Original YAML policy fields
are parsed first and unsupported settings refuse before the host rendering hook.
Rendered text passes normal native steering/profile bounds. No registration,
activation command, provider/model, environment lookup or filesystem loader is
introduced. Literal/default replies remain unchanged; complete=false remains.

The normal parent capture imposes an OS memory ceiling before exec, with a
separate self-imposed/observed worker ceiling even for a direct helper call.
Windows uses 256MiB committed job memory; Unix uses address-space limits, with
fail-closed errors but no Unix runtime certification here. Each render uses
100000 fuel, recursion 64, output 8192 bytes, a 10s parent capture intersected with
the original request/cancellation budget, and existing checked process cleanup.
These limits do not constitute an OS filesystem/network access sandbox.

Final qualification: **722 passed, zero failed, seven existing ignored**, nineteen
whole Rust groups with no filtered functions. This includes all 584 library
functions, 13 new integrations, two new renderer units and one real Win32 limit
query unit; 16 added functions are already included. All-target Clippy-Dwarnings,
complete 363-root formatting, build, Python 47, typecheck and npm 260/3 existing
skips pass. All 1115 frozen source/control inputs remain exact through qualification,
actual CLI and allocation probes. Only MiniJinja is added to Cargo.lock; every
other package is equal, and other native manifest data is equal.

Ten actual CLI parents prove raw old/new literal plans equal, deterministic
rendered plans, byte-identical literal and rendered exported profiles, and raw
old independent-repeat/new named-session MCP replies equal without normalization.
The fifth MCP request obtains initial_instructions and observes the rendered
mode guidance. Complete original nonempty snapshot/graph/record rows and
original CRLF/Unicode/selected/protected-canary source files stay exact. Plans
and exports open no Store; these explicit session tests use only their own Store.

Four additional direct-worker cases check small/large repeat and indent. Small
cases succeed and report the actual 256MiB ceiling. The 1GiB repeat is a normal
engine refusal, without allocator-failure text: it is not called proof of OS
allocation denial. The 1GiB indent intermediate produces the actual allocator
failure and exit 3221226505, with no stdout. The driver and all owned Jobs end
empty. These cases are separate from Rust function counts. Preparation executes
no analyzed project code; only native template interpretation is explicitly
selected. No model call occurs.

Failing/storage phases remain intact. Bootstrap1 finds a duplicate libc target
table before resolution; corrected bootstrap2/3 pin/fetch the sole new package.
Green1 stops under the 500MB disk guard before tests; the old qualified CLI stays
exact. After proven passed-symbol optimization, serial builds complete. No
profile, original fixture, ignore, internal deadline, system-wide setting,
Developer Mode, execution policy or PS1 change; no release retry or deletion.
Windows error-dialog flags are set only within the dedicated worker process.
The source-independent storage controller optimizes only already completed
harness PDBs while Cargo builds different targets; it launches no Cargo.

All original Job test Git bytes remain an exact 10498-byte prefix, SHA
 da2f7414064fb4a3034af25e35e56c6949e37d373cc8280c97154764fb441c77.
Production Windows Job source normalizes 395 old CRLF lines to LF. Its original
attach/member/cleanup tail is LF-normalized exact; the existing capture pipes,
checked completed-stream cleanup, ManagedChild cleanup and Drop bodies are exact.
No missing-Windows-capture fix or historical member-list race resolution is
claimed. Default OS job flags remain unchanged without the new optional limit.

[checkpoint](checkpoint.json) pins source/runtime/results. [raw-index](raw-index.json)
indexes every exact byte/hash member of [raw-logs.zip](raw-logs.zip), all reopened.
Only six frozen architecture/security/contract/status documents change afterwards,
recorded in [post-review hashes](post-review-docs.json). Audit2 pins the pre-feature
base explicitly; Audit1 records the same comparison while HEAD was that base.

Wider template/catalog/dynamic-context/Jinja compatibility, current editor or
canonical VSIX, full Windows/platform/hosted, original 353/R1-R9/77-runtime/quality,
Google live export, main and release acceptance remain open. No approved Google
document is available. The historic failed release image remains unvalidated.
