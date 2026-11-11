# Current main acceptance and next gates

## Verified baseline: 2026-11-11

[PR #7](https://github.com/HECer/Yoke-CodeIntelligence/pull/7) merged normally on
2026-11-08 as `3d58d07b382040e135a47e36fb24e81dc36185d7`. Remote `main` retains
the qualified tree `401254215948957f61a77034135358b6c24b5cc6` and original
feature histories. The local integration checkout is not a release receipt.

Both post-merge workflows completed successfully on that exact commit:

- [CI](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/42469372002):
  Node 22.16/24 platform checks, owned Windows launcher and combined VSIX job.
- [Rust core](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/42469372178):
  all platform shards/aggregates and the selected credential-backed TLS gate.

These workflows report 36 successful checks and one conditional history-browser
skip across 37 checks. The [metadata snapshot](validation/main-2026-11-08.json)
retains every job identity, source commit, conclusion and URL. This snapshot is
workflow metadata, not a new independent raw-artifact or visual audit. The real
VSIX job's declared scope is explicit/bundled Rust hosts and a separate pinned
TSGo query. Bundled TypeScript bytes alone do not qualify TypeScript host mode.

Earlier timed-out/failed runs remain historical evidence. In particular, passing
runs do not explain the intermittent facade or Python-launch timeout causes.

## Next implemented gate: packaged TypeScript host

The follow-up adds a fifth independently owned VSIX phase using a new
`scripts/vscode_typescript_host.cjs` fixture. It requires the extracted
TypeScript CLI, an explicitly selected and byte-pinned Node 22.16.0 executable,
real VS Code activation, refusal of workspace runtime/Node/CLI overrides,
protected-buffer exclusion, exact unsaved TypeScript source hashes, symbol
lookup in that overlay, unchanged saved source and stop. The original Rust
fixture remains unchanged. Final acceptance requires all five source-bound
phase receipts and independently observed natural process-tree exits.

Local typecheck passes. The complete Node suite reports 270 passed, zero failed,
three existing skips; its four new fixture tests use a labeled inert VS Code
API substitute. Separately, 60 admission/lifecycle/inventory tests pass,
including missing/forged TypeScript results and failed final cleanup. These
local tests are not actual editor-host acceptance. Fresh hosted compilation,
packaging and all three real host scopes must still pass on the follow-up head.
No native/VS Code payload execution is claimed locally for this change.

## Acceptance still open

- Reconcile each of the original 353 contract identities against current source
  and evidence. The retained ledger and R1-R9 residuals are not completed by CI.
- Qualify selected real backend runtimes separately. All 80 catalog declarations
  are not runtime certification; real Roslyn/GraphQL and authenticated Google
  export retain their own explicit prerequisites and host permissions.
- Complete the remaining strict resource/DAP and full platform/feature scopes.
  A passing finite fixture does not establish unrestricted upstream equivalence.
- Inspect actual current visual/motion and visible-shell behavior where required.
  Historical screenshots and browser metadata are not current visual approval.
- Produce and verify the intended release artifacts and public publication to
  `HECer/Code-Intelligence`. A merged implementation or CI-built test VSIX is not
  a release, deployment or publisher receipt.

The dated sections of [remaining work](REMAINING_WORK.md) and [status](STATUS.md)
retain their original source checkpoints. Their old pending/main statements
must be interpreted with this baseline, rather than treated as new failures or
silently rewritten as universal acceptance.
