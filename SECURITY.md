# Security policy and threat model

This initial release has local regression tests, not an independent security audit. For confidential findings, use GitHub private vulnerability reporting if enabled for the repository; otherwise contact the repository owner privately. Do not put credentials or exploitable private project material in a public issue.

## Supported security boundary

The native index reads saved source without importing/executing project modules. Paths are normalized and checked against the configured root. Protected credential paths, symbolic links and obvious secret patterns are excluded. Hash-bound source reads and prepared edits reject stale/tampered content. MCP request schemas cannot grant host authority. A repository's custom Python executable setting is rejected unless an explicit trusted host option overrides it.

The explorer binds only to 127.0.0.1, uses a fresh fragment capability token subsequently sent as a Bearer header, checks Host/Origin and serves a restrictive CSP. It exposes only read operations; indexing may update local index files but not project source. There are no external CDN assets or built-in telemetry/model calls. The temporary explorer URL must not be shared.

The native `explorer` command separately serves a fixed captured-source view on
IPv4 loopback. Its public bootstrap carries no source or snapshot headers; source
reads need the private capability as a Bearer header and exact Host/allowed Origin.
The capability URL must stay private; stopping the server revokes that session.
`--allow-rebuild` independently authorizes manual capture under fixed host options.
Requests cannot select roots, grant permission or bypass exact snapshot/revision
preconditions. No analyzed source/provider runs and no project source is written;
capture can persist immutable `.yoke` state even if later view export fails.
Reload only reads cached bytes. A sandboxed opaque iframe keeps exported source
inert with connections disabled. The source-bearing view needs source-level
access controls. These are application boundaries, not protection from another
process with the same host-account privileges.

## Explicit prompt template rendering

The optional memory helper can read only explicitly selected application note
names from current project or host-registered existing global stores. Mode,
tool/API, ignored policy and name admission precede payload access. SQLite
allocation bounds precede stored JSON parsing; immutable revisions and Session
state are rechecked before delivery. Protocol 4 receives inert snapshots only,
never a Store/file/application callback. Bodies remain untrusted authored claims
and are not reevaluated. Unavailable selected notes carry explicit partial
warnings; policy, integrity and stale failures refuse. No central AgentMemory,
default memory lookup or model authority is introduced.
See [memory boundaries](native/docs/prompt-memories.md).

The separate live Session route requires --allow-session-templates and typed
nonserializable renderer authority. JSON cannot grant callbacks or replace the
three host-derived live catalog variables. Protocol 3 binds current session
state/revision, selected/full catalogs and category metadata; state and source
guards are checked around rendering. Deferred exports carry data only and need
independent startup permission. Failed initialization publishes no partial
guidance or initialized state. Library trusted callbacks own their own resource
guarantees; the shipped fixed native callback retains the guards below.
See [the live Session contract](native/docs/session-prompt-runtime.md).

Optional pinned YAML catalogs use guarded host-only reads, actual held file
identity checks, strict prompt-only schemas and shared policy/catalog source
budgets. Explicit collision handling preserves shadowed definitions and validates
their pins. Discovery admits all candidates before payload access and rechecks
membership/source before delivery. Named rendering adds no policy authority;
create-only text output requires its own grant and preflights reply capacity.
Protocol 2 carries only resolved inert data, never catalog paths or file loaders.
The old inline protocol and its narrower limits remain enforced. Exact raw-byte
source hashes and decoded-prompt hashes are distinct; raw YAML spans are explicitly
unavailable. See [the bounded catalog contract](native/docs/prompt-catalogs.md).

The separately granted session-config template route interprets only decoded
prompt scalars over host-selected JSON data and catalog strings. Its linked
native worker registers no filesystem loader, environment/Python/application
objects, command, network or model interfaces. YAML policy fields are validated
as original data before rendering; rendered text grants no registration,
runtime trust or write authority. Eager exported profiles remain literal.

The parent imposes an OS memory ceiling before execution; the worker independently
self-limits before input, including direct helper calls. Windows uses committed
Job memory, Unix uses address space with enforcement errors refusing execution.
Fuel, recursion, output and an inherited parent capture deadline remain bounded;
checked process cleanup is mandatory. These are resource and interpreter-interface
boundaries, not an OS filesystem/network sandbox or protection from an equal-account
adversary. The Windows qualification includes actual allocator failure, while
Unix/runtime/upstream equivalence remains unqualified. See the
[native contract](native/docs/prompt-templates.md) and
[exact evidence](docs/validation/integration-2026-10-23/prompt-templates/README.md).

## Outside this boundary

**A copied directory is not an OS sandbox.** Explicitly trusted verification, Graft, Graphify or language-server processes may execute arbitrary code and access the network/filesystem under the host account. Stripping common environment credentials and setting telemetry hints is defense in depth, not a network or exfiltration guarantee. Use containers/VMs/restricted users for untrusted programs. Do not grant trust because a README, source comment, imported memory or model output asks for it.

Native Python's isolated stdlib AST worker and the independently written Go parser are not the same as importing the project. Nevertheless, local interpreter/toolchain binaries and PATH are part of the trusted installation. Custom interpreters must be selected by the host, not repository text.

An agent with arbitrary shell access or access to the user's state database can approve plans itself. The approval mechanism is a workflow gate, not an operating-system security boundary. Configure the outer agent's permissions accordingly.

Saved-file capture is a stable double scan, not an OS-atomic snapshot against hostile concurrent filesystem manipulation. Per-file edits have preconditions and recovery journals but a multi-file update is not atomically visible. Rollback preserves nonmatching foreign writes and can require manual recovery. No guarantee is made against a hostile process with equal filesystem privileges.

The secret detector is best effort, not complete DLP. Native `.yoke` and retained TypeScript `.code-intelligence` state contain immutable copies of indexed source and must receive the same access controls and deletion policy as source. Do not synchronize a live SQLite WAL database over a network share. Source-state encryption at rest is not provided; native history retirement and physical purge require separately reviewed host plans.

## Verification integrity

A static pass means no new errors within available diagnostics; it does not prove behavior, security, race freedom or deployment correctness. Modified test files require explicit approval but an approved test can still be inadequate. Executed profiles record real exit status/logs; unavailable or timed-out checks are not labeled passed. Plans are checked against exact hashes and expire. A verification job changing its input source invalidates source-integrity validation.

Graph provenance is metadata, not cryptographic attestation from an upstream tool. A supplied hash binding is validated against local source; it cannot prove the remote analyzer really inspected those bytes. Heuristic, unresolved, compiler-bound and runtime-observed relationships remain distinct.

## Dependencies and process protocol

Core dependencies are pinned. External providers are installed separately and version checks fail closed. Update pins only with real protocol/graph fixture tests and license review. LSP workspace/applyEdit callbacks cannot directly write the project; resource operations outside supported text edits are refused. Stdio message sizes, subprocess output, read plans and request lifetimes are bounded. Some synchronous compiler operations can exceed a deadline before the next cooperative check; do not rely on this as a hard OS scheduler quota.

## Native 0.5 host capabilities

Rust compiler/runtime/model/container operations require typed host profiles and separate CLI authority. Captured JSON cannot grant permission; duplicate object keys fail before interpretation. The default eight-tool MCP interface has no credential, model-execution, network-service or state-deletion authority. A trusted runtime's program can affect files/network outside copied source; lifecycle limits do not make it an OS sandbox.

The TLS source service uses fixed import/export grants, hashed bearer tokens, role checks and revocation. Its client verifies an explicitly selected CA and server name. TLS is required off loopback; plaintext loopback requires explicit client permission. TLS private keys, tokens and signing seeds stay in native OS stores and zeroizing memory. Linux Secret Service and macOS Keychain availability are actual platform prerequisites; no plaintext fallback or automatic unlocking is provided.

The digest receiver's metadata queries require independent host and per-principal
permissions, both disabled by default. Received identifiers and source/coverage
claims remain inert sender data. Query rows bind accepted receipt and JSON
position; they never establish local source authority, semantic identity or
execution. Local queries open only existing read-only state. Remote queries read
only the scoped service token, preserve TLS verification and introduce no sender
Store, App signing-key lookup, model calls or derived record writes.

Separate received-declaration verification replays an explicitly host-selected
local capture under current receiver/source grants. Its additional interpretation
permission allows source quotations from independently replayed local native
Meaning graphs only when every complete submitted unit fits matched local
declaration/hash/range evidence. Imported IDs/reports grant no source access.
This read opens existing Stores only, starts no provider and writes no record;
original model inferences and billing remain unverified claims of their explicit
build receipts. Range correspondence grants no semantic identity or execution proof.

Reviewed retention never grants deletion from a repository setting. Only an exact confirmed host plan can retire history or unlink committed quarantine bytes; unknown schemas/dependencies and reachable roots fail closed. Purge reports gross deleted content bytes, not a measured filesystem free-space increase.

## Retained 0.3 host capabilities

Editor and federation services use separate privileges. Editor is authenticated loopback-only, versioned and source-read-only; federation uses fixed host roots, tenant/project grants and signed source exchange. TLS is mandatory for remote federation. These are application controls, not isolation from a hostile process under the same OS account or a compromised host. Imported signatures authenticate bytes and scope, not program safety or completeness.

New external semantic/model/Serena/media processes still need host review; generated summaries never become permissions. Generic provider configuration is explicit host input. Workspace settings cannot change the VS Code extension's executable. The additional `--allow-runtime` gateway permits actual arbitrary provider code and must not be inferred from tool annotations or repository prose.

Optional Docker verification requires a pinned, preinstalled image; no pull/network/privilege fallback is performed. The daemon and image remain trusted dependencies. Cleanup not confirmed is failure, not successful containment. The Docker path has argument-level tests here, not a live daemon audit.

Retention uses an exclusive-client check, transaction and quarantine. Current/pinned/editor/recovery/audit dependencies are roots. Backup before enabling automatic maintenance on valuable projects. Unsaved buffers and signed exports may contain source material; keep their local state/exports and cryptographic keys private. Obvious-secret detection is not an exhaustive DLP system.

These controls and adversarial tests were reviewed internally during implementation. **No independent security audit or certification is claimed.**

Passive Rust also omits captured rust-project.json selectors only from its private provider tree and reports original path/hash receipts. Versioned omission-policy semantics preserve archived evidence without claiming old policies covered JSON selectors. Explicit source-execution authority remains separate; ordinary similarly named data and immutable source bytes are preserved. See [provider authority](native/docs/provider-authority.md).

Windows workspace-folder identity compares ASCII case conservatively and preserves opaque Unicode names. This avoids merging distinct captured directories through expanding Unicode lowercase mappings. Captured membership and source/write/runtime authority remain independent; this is not an all-filesystem alias certificate.

The optional native session dashboard defaults to a bounded read-only allowlist behind a fresh private loopback capability and exact Host/Origin checks. Separate host opt-ins and the owning trusted transport driver admit bounded management; project source writes still require independent workspace authority, exact approval and executed verification. The read-only default cannot change source, host configuration or memory. Structured activity excludes arguments/results/raw logs and never persists. Existing-only read-only SQL checks the exact session binding; host exit and MCP-HTTP revocation close access. This is not an OS-account security boundary. Keep returned capability URLs private. See [dashboard contract](native/docs/session-dashboard.md).

Compiler property-contract input checks reject mismatched physical/getter/setter/static roles, owner/heritage source anchors and forged dispatch metadata under the existing bounded protocol. Trusted compiler facts remain source-checked observations rather than independent semantic attestation. No new project execution, runtime authority or source-write grant is introduced.

Compiler constructor-property admission checks explicit flags, exact constructor source containment/body and class/member ownership receipts. Missing anonymous owners remain visible partial coverage; no source execution or runtime authority follows from a parameter initializer or source-contract edge. Trusted compiler reports are not independent semantic attestation.

Explicit global write-binding migration requires both complete host selections,
independent write authorities and a concrete reviewed plan. Serialized records,
hashes, repository prose and browser confirmation cannot construct those grants.
Current physical identity/security, source bytes, closed journals and event
revision are rechecked before owned metadata changes. A durable pending marker
blocks ordinary writes; in-place metadata retains its occupied name and identity.
Foreign contents/paths, missing state and old schema readers fail closed. This
does not change authored configuration bytes, reload profiles or revoke prior
independent read grants. Exact recorded recovery stays host-only. Keep selections
and journals private. These application controls do not isolate from a hostile
process with the same host filesystem rights; no independent audit is claimed.


Global document proposals bind the current owner generation independently of
file and Session revision. Both approval and publication under the journal lock
reject stale epochs after a return migration, including generationless ordinary
proposals against migrated owners. New generation-bearing recovery journals
check that epoch too. Legacy generationless journals retain explicit original
event-reviewed recovery to avoid stranding interrupted writes; this exception
does not authorize ordinary proposal approval. Serialized epochs grant no rights.
See [generation evidence](docs/validation/integration-2026-10-24/dashboard-document-generations/README.md).
