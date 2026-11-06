# Hosted real TSGo qualification

This additive Windows CI profile exercises the real Microsoft TSGo language
server through Yoke's public native `session` CLI, host backend registration and
ordered `typescript_tsgo` selection. It reuses the **same job's fresh release
producer** from `host_vscode.py prepare`; it does not adopt a cached CLI or
download a Yoke executable artifact. The existing VSIX job, assertions, timeouts,
packagers, importer bounds and owned-Job launcher are unchanged.

The selected vendor is `@typescript/native-preview-win32-x64` version
`7.0.0-dev.20260707.2`, with 113 regular files and 28,366,072 unpacked bytes.
The [versioned official registry metadata](https://registry.npmjs.org/@typescript/native-preview-win32-x64/7.0.0-dev.20260707.2)
supplies the fixed tarball URL and SHA-512 integrity value:

```text
sha512-DL4u27stv0fo71sVhOzHSwE+YMZsbBijVI+kg5dLDLilSH79WFTJ8RSQ46vJrCMt+Gjlv/JOZP1PuLJDfioYeQ==
```

The package's source revision is
`9977d6d38fcc78de8ae71770f3aa08256e6cc861`. Microsoft's
[launcher source](https://github.com/microsoft/typescript-go/blob/9977d6d38fcc78de8ae71770f3aa08256e6cc861/_packages/native-preview/lib/getExePath.js)
identifies the platform package's `lib/tsgo.exe`; its
[LSP entry point](https://github.com/microsoft/typescript-go/blob/9977d6d38fcc78de8ae71770f3aa08256e6cc861/cmd/tsgo/lsp.go)
requires `--lsp --stdio`. This is an exact published preview version, not a
claim that a later upstream source or all TSGo features have been qualified.
No Microsoft implementation code is copied into Yoke.

## Process and input admission

Only hosted 64-bit Windows CI is admitted. The exact `GITHUB_SHA`, same-source
four-case Python Job lifecycle evidence, fresh target/dist producer JSON, unique
nonfresh-Cargo release role, selected CLI metadata and source/built maps are
rechecked. The original 12 GB pre-build disk guard and 160 MB CLI bound remain.
Two separate 180-second owned workers use the existing suspended-launch,
assign-before-resume, no-breakaway, kill-on-close runner. Each worker confirms
membership in its specific Job and closes that query handle before native,
vendor image or archive payload reads. The final gate requires natural whole
process-tree exit before cleanup and complete closed-handle cleanup evidence.

Vendor procurement happens only inside the tool worker. HTTPS uses the fixed
registry host with no proxy or redirect, 20-second socket timeouts and a
120-second cooperative procurement deadline. The compressed stream is capped at
32 MB and must match the exact SHA-512. Decompression is capped at 30 MB **before
TAR parsing**. At most 256 TAR entries are considered; regular-file count and
expanded bytes must match the registry pins. Links, sparse/special entries,
traversal, devices, alternate data streams, duplicate case aliases and parent
file conflicts refuse. Extraction is create-only with per-volume reserves;
every unpacked file is hashed and the complete inventory rechecked before and
after consumption. Package name/version/platform/architecture and the actual
`--version` output must agree. No npm install or lifecycle script is run.

Fresh random vendor and worker roots live under `RUNNER_TEMP`, outside the
checkout. The runner redirects home/config/temp directories to private owned
locations. Native/vendor subprocesses receive only basic OS lookup variables
and those private homes. CI tokens and ambient credential variables are not
passed to them. The real LSP server may execute source in general, so the profile
uses an explicit host `allowSourceExecution` grant limited to the newly authored
fixture. It does **not** fabricate a measured passive profile. The fixture
disables automatic type acquisition, has no package lifecycle, and selects no
user repository or account. This is not a general network or OS security sandbox.

## Actual acceptance and evidence

The fixture contains two TypeScript files, a decomposed Unicode module filename,
an astral character and combining mark before a queried identifier, and CRLF
bytes. A host profile selects only `typescript_tsgo`. Three native Session
queries require a typed nonempty hover at the exact UTF-16 range, an exact
definition in the Unicode module, and all three known declaration/import/call
reference anchors. Every returned location must bind the original raw file hash,
byte range, quoted token, selected executable and source-data trust. All three
queries must share one snapshot. Missing, foreign, duplicate, stale, partial or
incorrectly claimed complete results refuse. Native/vendor identities, all
selected source/built inputs and saved fixture bytes must stay unchanged.

The vendor stages follow the original VSIX steps and may run after an earlier
VSIX failure if the fresh producer succeeded. Their individual failures are
temporarily tolerated so both diagnostic stages can retain evidence; an
unconditional final vendor gate requires both successful stages and Job proofs.
No prior VSIX failure is cleared or converted to success. The original overall
60-minute CI job bound is unchanged.

Only the separate `tsgo-real-vendor-windows` text artifact is uploaded: receipts,
bounded logs and the owned-source protocol result. Vendor/native binaries,
archives, extracted libraries, private homes and workspaces are excluded. The
same-run native producer receipts remain in the original VSIX artifact.

Local pure admission tests extract named functions from the source AST and run
only against inert metadata and authored protocol data. They do not import the
controller, run a worker, read an image or qualify Windows kernel behavior.
Filesystem/log operations outside workers retain the existing cooperative/CI
bounds; this profile is not a universal hard-interruptible I/O certificate.

The profile is a candidate until its exact hosted artifacts are audited. Real
vendor correctness beyond these operations, all other 79 current backends,
passive TSGo certification, full feature/platform/VSIX/main/release acceptance
and the strict macOS memory issue remain open. The historical 353-entry ledger
is untouched.
