# Native VSIX archive admission

The offline packager combines an explicitly selected native archive with the
extension. Optional --include-typescript also carries retained TypeScript 0.3.0.
Native/extension versions must match. Supported native targets remain Windows
x64, Linux x64, macOS arm64 and macOS x64; target identity is recorded in the
VSIX manifest. Packaging does not execute or certify the selected executable.

## Input admission

Before Python ZIP directory allocation, bounded end-of-directory inspection
admits a single-disk ordinary ZIP with at most 8,192 entries and 8,000,000 central
directory bytes. The physical archive cap is 256,000,000 bytes. Unsupported
ZIP64 end records, multidisk or inconsistent directory layouts refuse.
The existing entry cap stays 128,000,000 bytes. Combined declared payloads may
not exceed 256,000,000 bytes; manifest.json is limited to 1,000,000 bytes.
All entry metadata is checked before any payload is read.

Names use canonical forward-slash relative components. Empty/dot/traversal,
drive/ADS/reserved Windows names, controls, backslashes, trailing dots/spaces,
overlong components, exact duplicates and ASCII-case aliases refuse. File-parent
conflicts refuse. Ordinary explicit empty directories are accepted but not
shipped. Symlink, special-file and encrypted entries refuse. Unicode names and
payload bytes are preserved; no Unicode normalization or expanding case-fold
identity is invented. This is not a certificate of every filesystem alias rule.

The manifest must be strict UTF-8 JSON with unique object keys and no NaN or
Infinity constants. Version, supported target and lowercase SHA256 are checked;
the selected executable must be a regular archive member with exactly its
expected name and matching checksum. ZIP CRC/entry lengths are checked while
reading. If the existing corresponding local release executable is available,
its size and SHA256 must still match. No stale executable gate is bypassed.

Only after native admission and required local build inputs are accepted does
the packager create a fresh output. Existing output is preserved. A later I/O
failure may leave a partial newly created archive; it is not a committed runtime
or release receipt. Final ZIP integrity is checked. Native Unix executable mode
and the original platform identity remain intact.

These are finite format/byte budgets, not an OS memory sandbox or publisher
signature. License completeness, source/build provenance, canonical packaging,
actual bundled extension-host behavior and release publication require their
own evidence. Synthetic fixture archives are not deliverable native releases.
See ../../docs/validation/integration-2026-10-23/vsix-archive-admission/README.md
for the qualified source, adversarial trials and actual CLI packaging scope.
