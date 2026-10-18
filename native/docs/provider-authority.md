# Provider authority and measured passive profiles

The 77 entries in `server_catalog` describe selectable public provider families.
They do not install software, select executables, grant source execution, or
certify every implementation or version. An unprofiled production provider is
unavailable at runtime by default. Metadata-only registration remains readable.

`HostConfiguration.passiveProfile` is selected by the host and is never derived
from a repository language, backend ID, executable filename, or MCP argument.
The measured Windows strategies are `rust_analyzer`,
`typescript_language_server`, and `jedi_language_server`. Protocol tests use the
explicit `contract_fixture` profile, whose output identifies the fixture boundary.
Backend launch trust and `allowSourceExecution` are separate host permissions.
The latter defaults to false and is omitted when false, preserving historical
configuration serialization. Historical absent permission is not an execution
grant or evidence that a provider did not execute source.

Rust's private provider tree omits captured `.cargo/config[.toml]`,
`rust-toolchain[.toml]`, and Rust Analyzer execution configuration selectors.
Original source, source hashes, snapshots, and structural graphs remain intact.
The owned process uses a private Cargo home, offline Cargo, and empty compiler
wrappers. Build-script execution, procedural macros, and check-on-save are disabled.
Cargo manifest workspace/member/dependency/source paths must remain within the
captured private tree; exact paths require captured membership. Host project,
compiler, wrapper, environment, extra-argument, or command overrides that lack a
measured private mapping require explicit source execution permission.

The TypeScript strategy disables automatic typing acquisition and refuses
additional host plugins, local-plugin loading, and redirected tsserver paths
without source execution permission. The measured TypeScript implementation does
not enable local project plugin loading by default. The Jedi strategy refuses
additional auto-import, interpreter, or import-path redirects. Its measured
implementation restricts compiled imports to base interpreter paths while unsafe
extensions are disabled; a captured `gi.py` did not execute in the negative gate.

These strategies configure explicitly trusted provider code. They are not an OS
sandbox, a binary supply-chain certification, or an attestation of transitive
provider effects. Reports distinguish the declared profile, selected executable
hash, original omitted path/hash/reason, and host permission. TCP peer code and
transitive plugins remain unattested. Other production families need a measured
strategy or an explicit host `allowSourceExecution: true` selection.

Semantic overlay schema 2 binds `providerContext` and its hash into immutable
node, evidence, and observation identities. The context contains only exact host
permission, a strict known policy string, and bounded ordered omission records.
Current and archived readers validate exact original manifest membership. Schema
1 remains readable with unchanged bytes and unknown historical authority; future
schemas and ambiguous v1/v2 registrations are rejected.

`restart_language_server` performs genuine fresh initialization, negotiated health,
and shutdown for each applicable registered provider. Operational generation
advances only after every check succeeds. Providers currently use request-scoped
processes, so status reports `persistentProcessesRestarted: 0`. An existing TCP
peer is never terminated or claimed to have been restarted. Immutable overlays
and source artifacts are preserved.

Actual Windows gates cover Rust Analyzer 1.94.1, TypeScript Language Server 5.1.3
with TypeScript 5.8.3, and Jedi Language Server 0.47.0. A harmless independently
compiled rustc probe reproduced source-config execution before the Rust policy;
the passive gate produced zero probe invocations, while explicit source execution
permission enabled the original source-selected command. Fixture and actual
provider results are recorded separately. Linux and macOS certification is deferred.
