# Windows metadata and lint qualification

Actual source `d80c6c784f462d5ad60c0f553986b2b4895108d4`, Rust/Cargo1.94.1,
host `x86_64-pc-windows-msvc`:

```text
cargo check --workspace --all-targets --locked -j 1 --message-format=json
cargo clippy --workspace --all-targets --locked --keep-going -j 1 --message-format=json -- -D warnings
```

Both commands exit0 with Cargo build-finished success and zero warning/error
diagnostics. Each has370 own-package artifact observations, all `fresh=false`,
over368 distinct target-name/kind pairs. Compiler source paths resolve into this
workspace's native tree and output paths into its target tree. All1003 original
native/Cargo/workflow input hashes match before/after both runs and the subsequent
library build. No source, profile, assertion, ignore or internal test budget is
changed. These are default-feature Windows metadata compilations, not370 tests,
whole-suite runtime coverage, doctest compilation or Unix acceptance. They now
establish typing for the new Windows test code which earlier checkpoints had
not compiled locally; execution still needs its own actual evidence.

Each command starts suspended in a private kill-on-close Job and is assigned
before resume. Before each start, free space meets the existing1.2GB admission.
The separate500MB floor and finite build/log budgets remain monitored. Both
metadata runs observe the owned tree empty before forced cleanup and prove the
root exited and Job empty afterward. Their1003 pinned inputs remain unchanged.
The Cargo profile stays at the repository defaults; job count1 is scheduling.
No Yoke program/test image is selected or executed by these metadata runs.

The separate command

```text
cargo test --lib --no-run --locked -j 1 --message-format=json
```

returns Cargo0 and build-finished success, producing one fresh library-test
artifact. However, its owner receipt is **incomplete**: one member remains after
the Cargo root exits and the bounded drain ends. It is then forcibly cleaned;
root exit and Job active0 are independently proven. The remaining member's
identity is unmeasured; it is not assumed benign. This is not accepted as clean
producer completion. The candidate is neither hashed/adopted nor executed.
There is no native test result. Its metadata-only observed name/size are not
binary qualification. The strict owner result remains false despite Cargo0.
Free space after linking is about1.06GB, below the unchanged1.2GB admission.
No retry, old executable substitution, cache deletion or limit relaxation follows.

`runs.json` preserves these separate outcomes, commands, compiler versions,
actual Job receipts and artifact observations. `source-inputs.json` pins the
1003 working-copy byte hashes, with platform checkout rendering kept distinct
from Git blob identity. `raw-evidence.zip` contains exact stdout/stderr, source
before/after maps and owner receipts, plus the actual private control scripts.
Every ZIP member is reopened and checked against `raw-index.json`; no executable
or PDB bytes are included. The archive is evidence, not a signature of runtime
correctness or a native executable distribution.

## Storage admission without changing evidence bytes

Only older own text logs and source/input manifests below `validation` and
`docs/validation` are selected. Native images, caches, linked/reparse paths,
read-only inputs and files modified in the preceding hour are excluded. Each
file is opened through a pinned single-link handle which denies concurrent
writers/deletion. SHA-256 and identity are checked before and after synchronous
ordinary NTFS compression; no directory default or WOF backing is requested.
The API is the documented
[FSCTL_SET_COMPRESSION](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_set_compression).

All1152 files retain their exact SHA-256, length, unique file identity and mtime;
all remain regular files without a reparse point. Their resulting format is
LZNT1. No file is deleted or renamed. See `compression.json` for every hash and
identity. Reported size fields are
[FILE_COMPRESSION_INFO.CompressedFileSize](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_compression_info),
not an independent extent-allocation measurement. Volume-free changes also
overlap compiler/other host activity and are not attributed exclusively to
compression. Measured free space nevertheless meets admission before the actual
check/lint/library-build starts; their own receipts retain subsequent minima.

The compression target margins are not all reached; no admission is weakened.
Neither that fact nor clean forced cleanup converts the incomplete library
producer into success. Full native execution, hosted PowerShell phases,
current platform/history/browser/VSIX, full STATUS, main and release remain open.

## Separate repository and fixed host checks

`npm run typecheck` exits0. `npm test` exits0 with260 passed,0 failed and3
existing skips (263 total). `checks.json` records the exact raw log hashes and
UTF-8 rendered log hashes. Original PowerShell stderr wrappers are retained;
those wrappers are not the command exit status. These checks execute no native
Yoke test image.

The separate inert PowerShell comparison in `powershell-cwd.json` uses an owned
temporary working directory in ordinary and Windows verbatim (`\\?\`) spelling.
Both commands exit0 in about0.24?0.28s, reach script/folder completion, observe
both Jobs empty before cleanup and prove root exit and both pipe EOFs. This
negative local observation does not identify the hosted native20s timeout cause
or qualify native execution. No product behavior changes follow from it.
