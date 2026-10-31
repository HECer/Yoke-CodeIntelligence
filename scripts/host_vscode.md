# Fresh Windows combined VSIX acceptance

The additive push-only `windows-vsix` CI job qualifies only its exact source and
the two stated Rust extension-host modes. It depends on the same-run
`vscode-owned-job` success, downloads that small text artifact, and independently
requires its source revision and all four expected lifecycle outcomes. A probe
receipt is not a native, editor or VSIX result.

## Fresh production

The profile requires 64-bit hosted Windows, exact `GITHUB_SHA`, the fixed Node
22.16.0/Rust 1.94.1 setup, original controlled TLS/runner policy, and absent
`target`/`dist` outputs. Release build admission requires 12 GB free. One fixed
locked workspace release Cargo command produces JSON evidence; the selected
native CLI must be a unique matching fresh artifact at `target/release/yoke.exe`,
with successful final completion, expected source role and at most 160 MB.
No produced Yoke CLI image is read or executed in this preparation phase;
compilers and their normal build tools still execute as part of compilation.

Locked TypeScript dependencies are installed without lifecycle scripts. The
original build produces fresh `dist`; all selected repository/package inputs and
the full bundled `dist`/TypeScript files are hashed and rechecked. Repository
source, generated JavaScript and dependency-package byte identities stay
separate. The selected source map includes packaging inputs, not every compiler
or external build input.

## Four owned workers

Each phase gets an assigned-before-resume kill-on-close Job through the existing
launcher, an at-most-180-second lifetime, disk/output checks, independent natural
tree-exit-before-cleanup proof and checked closure. Before the first native/editor
payload read, the worker confirms membership in that specific Job, closes its
non-inheritable query handle, rechecks current source/built inputs and admits the
fresh Cargo role and unchanged metadata.

1. **Tool:** Download only the pinned official VS Code 1.143.1 archive, using its
   exact 360,125,130-byte length and SHA-256
   `b4f9643b8a549ca2c85a1bbc9063fc76f1e08fc0036a7f6a1a181cdc05235eca`.
   Redirects stay on the fixed Microsoft download host. Bound the directory
   before ZIP parsing, then require all 3,940 entries / 1,058,631,563 expanded
   bytes. Extract into a fresh short runner-temp root and pin every file plus
   the actual versioned package-metadata path. No editor is executed here.
2. **Package:** Run the unchanged native packager with its existing version and
   license gates, then the unchanged combined native+TypeScript VSIX packager.
   Reopen complete inventories/manifests, original native/entry limits, every
   licensed/native/source/built member hash and the bundled CLI identity. Unpack
   only after the complete VSIX inventory is admitted.
3. **Explicit:** Run actual selected Code with the original extension source and
   the fresh release CLI in a fresh private profile/workspace/extensions fixture.
4. **Bundled:** Use a different private fixture and the actual extracted VSIX
   extension/native CLI, retaining the original no-global-override assertions.

Both real workspaces live outside the checkout under runner temp, preserving
the original launcher's isolation from parent Git-ignore/source policy. Code
uses the original fixed arguments and unchanged `test-host.cjs`: real extension
activation, ignored workspace executable selection, protected open-buffer
exclusion, unsaved source-bound overlay/hash, actual native symbol lookup,
unchanged saved source and stop. The reported editor version must be 1.143.1.
No user profile or extension installation is selected. Token environment fields
needed only by the license phase do not enter the editor child.

Only after all four source-bound phase receipts and all four complete natural
Job exits pass can `finish` write a complete result. A successful editor marker
cannot qualify a leaking process tree. Partial stage files, changed inputs,
foreign roles, stale probe receipts and missing scopes refuse.

## Budgets and evidence scope

The native importer is unchanged: 128 MB ordinary entry / 160 MB selected Windows
CLI / 256 MB native archive and total payload. The new combined-VSIX envelope is
256 MB compressed / 384 MB expanded / 12,288 entries; this does not enlarge the
native importer. The fixed editor archive has its separate pinned 1.06 GB role.
Before any ZIP decoder allocation, directory/count/framing/ZIP64-override limits
are checked. Only stored/deflated members are admitted; traversal, aliases,
devices, links, encryption and file/directory conflicts refuse.

Procurement has a 120-second loop and 20-second socket bounds. Release production
has a 30-minute subprocess / 35-minute step; the whole new job has a 60-minute
ceiling. Parent/source/stat/scan operations and post-producer log checks retain
cooperative/CI bounds; this is not universal interruptible-I/O or kernel-sharing
certification. Runtime fixture/cleanup bounds remain those of the launcher.
The editor/extraction destination volume is admitted separately with its full
known expansion plus reserve; bounded writes recheck that volume rather than
assuming it shares the checkout drive.

`combined-vsix-rust-host-windows` uploads only selected text receipts and bounded
logs, including closed failure-phase labels. Editor/native/VSIX payloads and
private profile trees stay outside it. The four workers and two host modes keep
their own results; counts are not native test totals.

The retained TypeScript 0.3 runtime is bundled and byte-verified; these host runs
select Rust 0.5 and do not execute the advanced TypeScript extension mode. The
profile changes no UI, native implementation, packager, importer, extension or
original test body. Local checks use only inert source-extracted admissions,
syntax/source audits and required npm checks. Actual procurement, compilation,
native packaging, Windows kernel behavior and both real host modes remain
unrun locally and require hosted evidence. Full backend/feature-combined parity,
all platform gates, main/publication and release acceptance remain open.
