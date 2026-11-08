# Complete native formatting checks

Run from the repository root:

    python scripts/native_fmt.py --check

The command checks the complete Cargo target inventory without changing source.
On Windows, cargo fmt --all can exceed CreateProcess command-line capacity when
hundreds of absolute target paths are sent to one rustfmt process. The native CI
Windows job uses this helper. Unix CI retains cargo fmt --all --check.

## Coverage and formatter policy

Cargo metadata --offline --locked --no-deps --format-version 1 supplies target
roots and each target's edition. All reported target kinds participate, including
library, binary, integration test, example, benchmark and custom build roots.
Local path dependencies are recursively inspected using their exact manifests;
cycles and repeated canonical roots are deduplicated. An unavailable dependency,
empty or malformed inventory, unsupported edition or conflicting editions for
one source path refuse before formatting. Missing coverage cannot become success.

Each admitted root runs as a separate literal argument to the selected rustfmt,
with its target edition and --check. Ordinary rustfmt module recursion and
configuration discovery remain enabled. The helper supplies no skip/config/emit
override and has no write mode or arbitrary formatter-argument forwarding.
The explicit host RUSTFMT executable override is retained as one command path;
otherwise rustfmt is resolved through the existing host PATH/toolchain.

All roots are checked even when one formatter returns a failure; the first
nonzero child status is retained. Missing tools, admission errors and timeouts
produce a nonzero incomplete result. --list prints the complete admitted
path/edition list as JSON without invoking the formatter.

## Limits and trust

Each metadata response is admitted at 16,000,000 bytes before JSON decoding;
duplicate JSON keys refuse. Metadata output spills to a private temporary file.
At most 128 requested manifests/pending entries and 16,384 distinct roots are
admitted. Each metadata/formatter child has a 120-second timeout. These are
application limits, not hard OS allocation, process or disk quotas.

This is a contributor tool for an explicitly selected trusted workspace and
host compiler installation. It invokes metadata and rustfmt, never Cargo build,
test, project build scripts or produced native executables. It is not an analysis
capability, source execution grant, Rust semantic check or formatting sandbox.

## Qualification

The actual Windows source has 361 target roots. The old Cargo verbose root set,
independent metadata and the helper list agree exactly; the original aggregate
still fails OS206 at a reconstructed 33,872 UTF-16-unit command. The new complete
check passes all 361 roots. A separate actual Cargo/rustfmt fixture checks a
Unicode/literal path, a local dependency with a different edition, a non-root
module and inherited hard-tabs configuration. Both canonical Cargo and the
helper refuse the malformed dependency and accept the corrected fixture;
source bytes stay exact and the build-script canary never runs.

Python 47, typecheck and npm 260 with 3 existing skips pass. Rust production,
original tests/ignores/deadlines/profiles and the current native executable stay
unchanged. No new native test, hosted-platform or whole-Windows pass is claimed.
See ../../docs/validation/integration-2026-10-23/native-formatting/README.md.
