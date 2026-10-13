# Security policy and threat model

This initial release has local regression tests, not an independent security audit. For confidential findings, use GitHub private vulnerability reporting if enabled for the repository; otherwise contact the repository owner privately. Do not put credentials or exploitable private project material in a public issue.

## Supported security boundary

The native index reads saved source without importing/executing project modules. Paths are normalized and checked against the configured root. Protected credential paths, symbolic links and obvious secret patterns are excluded. Hash-bound source reads and prepared edits reject stale/tampered content. MCP request schemas cannot grant host authority. A repository's custom Python executable setting is rejected unless an explicit trusted host option overrides it.

The explorer binds only to 127.0.0.1, uses a fresh fragment capability token subsequently sent as a Bearer header, checks Host/Origin and serves a restrictive CSP. It exposes only read operations; indexing may update local index files but not project source. There are no external CDN assets or built-in telemetry/model calls. The temporary explorer URL must not be shared.

## Outside this boundary

**A copied directory is not an OS sandbox.** Explicitly trusted verification, Graft, Graphify or language-server processes may execute arbitrary code and access the network/filesystem under the host account. Stripping common environment credentials and setting telemetry hints is defense in depth, not a network or exfiltration guarantee. Use containers/VMs/restricted users for untrusted programs. Do not grant trust because a README, source comment, imported memory or model output asks for it.

Native Python's isolated stdlib AST worker and the independently written Go parser are not the same as importing the project. Nevertheless, local interpreter/toolchain binaries and PATH are part of the trusted installation. Custom interpreters must be selected by the host, not repository text.

An agent with arbitrary shell access or access to the user's state database can approve plans itself. The approval mechanism is a workflow gate, not an operating-system security boundary. Configure the outer agent's permissions accordingly.

Saved-file capture is a stable double scan, not an OS-atomic snapshot against hostile concurrent filesystem manipulation. Per-file edits have preconditions and recovery journals but a multi-file update is not atomically visible. Rollback preserves nonmatching foreign writes and can require manual recovery. No guarantee is made against a hostile process with equal filesystem privileges.

The secret detector is best effort, not complete DLP. The `.code-intelligence` directory contains immutable copies of indexed source and must receive the same access controls and deletion policy as source. Do not synchronize a live SQLite WAL database over a network share. No encryption-at-rest or automatic state garbage collection is provided.

## Verification integrity

A static pass means no new errors within available diagnostics; it does not prove behavior, security, race freedom or deployment correctness. Modified test files require explicit approval but an approved test can still be inadequate. Executed profiles record real exit status/logs; unavailable or timed-out checks are not labeled passed. Plans are checked against exact hashes and expire. A verification job changing its input source invalidates source-integrity validation.

Graph provenance is metadata, not cryptographic attestation from an upstream tool. A supplied hash binding is validated against local source; it cannot prove the remote analyzer really inspected those bytes. Heuristic, unresolved, compiler-bound and runtime-observed relationships remain distinct.

## Dependencies and process protocol

Core dependencies are pinned. External providers are installed separately and version checks fail closed. Update pins only with real protocol/graph fixture tests and license review. LSP workspace/applyEdit callbacks cannot directly write the project; resource operations outside supported text edits are refused. Stdio message sizes, subprocess output, read plans and request lifetimes are bounded. Some synchronous compiler operations can exceed a deadline before the next cooperative check; do not rely on this as a hard OS scheduler quota.

## 0.3 host capabilities

Editor and federation services use separate privileges. Editor is authenticated loopback-only, versioned and source-read-only; federation uses fixed host roots, tenant/project grants and signed source exchange. TLS is mandatory for remote federation. These are application controls, not isolation from a hostile process under the same OS account or a compromised host. Imported signatures authenticate bytes and scope, not program safety or completeness.

New external semantic/model/Serena/media processes still need host review; generated summaries never become permissions. Generic provider configuration is explicit host input. Workspace settings cannot change the VS Code extension's executable. The additional `--allow-runtime` gateway permits actual arbitrary provider code and must not be inferred from tool annotations or repository prose.

Optional Docker verification requires a pinned, preinstalled image; no pull/network/privilege fallback is performed. The daemon and image remain trusted dependencies. Cleanup not confirmed is failure, not successful containment. The Docker path has argument-level tests here, not a live daemon audit.

Retention uses an exclusive-client check, transaction and quarantine. Current/pinned/editor/recovery/audit dependencies are roots. Backup before enabling automatic maintenance on valuable projects. Unsaved buffers and signed exports may contain source material; keep their local state/exports and cryptographic keys private. Obvious-secret detection is not an exhaustive DLP system.

These controls and adversarial tests were reviewed internally during implementation. **No independent security audit or certification is claimed.**
