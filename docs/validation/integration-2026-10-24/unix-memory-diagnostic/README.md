# Unix memory API diagnostic, native acceptance pending

This checkpoint adds a CI-only observation before the existing native gates on
Linux/macOS shard zero. It changes no native Rust code, product memory limit,
assertion, timeout, Cargo profile or warning gate. Native acceptance remains open.

The fixed owned C fixture observes its starting virtual size and the result of
installing the original 256-MiB hard and soft address-space limits. Only after
both limits are read back exactly does it try separate 512-MiB malloc and mmap
reservations. It immediately releases successful reservations without touching
their pages. Header installation and allocation denial are independent facts;
neither is native Yoke qualification. Failed API calls remain explicit data.

Linux runs a direct self-limit and a shell-inherited-limit trial. macOS also
compares a direct trial with the child-only MallocNanoZone=0 setting; its shell
trial uses that setting too. No host console, permissions, OS configuration or
product environment changes. The probe cannot establish why the native worker
fails merely from its own public VM-size observation.

The controller requires Unix GitHub Actions and an exact GITHUB_SHA checkout
before creating output. It compiles only this owned fixture using a fixed system
compiler, with a private temporary directory and selected environment. Each
process owns a fresh session/process group, bounded file-backed logs and a
deadline. Cleanup requires both the root wait and observed process-group absence;
signal refusal remains an error even after attempted root cleanup. The controller
checks disk reserve, ordinary files, output membership/capacities, strict report
schema and unchanged fixture/binary hashes. Only JSON and bounded logs enter the
separate diagnostic artifact; its temporary executable is removed locally.

Every result says diagnosticOnly=true and nativeAcceptance=false. A missing or
failed compiler says diagnosticComplete=false and preserves the actual reason.
A complete set of attempted trials may include explicit OS/shell refusal; it is
never a successful native or enforced-memory certificate. Malformed evidence or
unproved cleanup fails the step. Existing native gates still run afterward when
the diagnostic produces valid data, and the original platform coverage gate
cannot consume this separately named artifact as a native shard receipt.

Actual local validation: 101 Python tests pass with warnings treated as errors,
including 14 diagnostic tests (nonadditive). The first whole-Python attempt had
two failures and one skip because this shell did not expose Cargo on PATH; its
log is retained. Adding the already installed Cargo directory only to the test
process environment produces 101 passes, zero failures and zero skips. This
exercises controller/schema/cleanup refusals, not Unix OS enforcement or C
compilation. Typecheck and npm260/0/3 existing skips pass; result.json retains
their separate counts.

No native Yoke image is read or executed locally. The current free disk remains
below the 1.2-GB full-native-build admission threshold. Actual C compilation,
Linux/macOS observations, current native matrix, Windows runtime failures,
history live acceptance, current VSIX and complete STATUS acceptance remain open.
Source and log hashes plus actual local counts are preserved in [result.json](result.json).
The local PowerShell log displays use UTF-8, LF and trimmed trailing whitespace;
the original UTF-16 capture hashes remain recorded separately. This display
normalization does not alter test results or source evidence.

The investigation uses primary kernel sources: XNU's
[VM limit implementation](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/vm/vm_map.c)
checks current map size for both address-space and data limits. The examined
[memorystatus control](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_memorystatus.c)
requires privileges/entitlement for the relevant controls. Neither establishes a
qualified unprivileged replacement for the current product ceiling. This
diagnostic measures actual runner behavior before any product change is chosen.
