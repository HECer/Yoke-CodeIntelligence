# Windows CI temporary-directory spelling: partial correction

The eight Linux shards at development source `84e11866` completed successfully
in [push run40101366598](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40101366598).
Windows shards1/2/7 and seven macOS shards failed. All ten failed-shard artifacts
were downloaded by their API IDs, checked against their reported byte lengths
and SHA-256 digests, and parsed without filesystem extraction. Entry names,
types, capacities, source revision, inventory digest and retained log hashes were
checked. [Diagnostics](diagnostics.json) retain the observed failures, not a
whole-platform or release certificate. No native image was downloaded or run.

The Windows source-boundary test fails earlier than its intended assertion:
the runtime temporary parent contains a shell-disallowed spelling. Its explicit
fixture suffix contains only ASCII letters and hyphens. A DOS short alias in the
ambient parent is a candidate cause, not yet proven on the hosted runner.
The new CI step observes the existing runtime temporary directory, resolves its
ordinary long spelling, rejects links/junctions in either ancestor chain, and
requires repeated matching filesystem identities. Unknown identities and
non-ASCII, short-alias, metacharacter, space or traversal spellings refuse.
It emits only TEMP/TMP into the job environment for subsequent steps. It does
not create or relocate a directory or change the product's strict shell policy.
These repeated observations are not an atomic filesystem guarantee.

The initial root comes directly from GetTempPath2W, matching
[Rust1.94.1](https://github.com/rust-lang/rust/blob/1.94.1/library/std/src/sys/pal/windows/os.rs)
and the [Windows API contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppath2w),
including TMP/TEMP ordering. Python's separate temporary-directory selection
does not choose the root. This setup runs only on Windows in GitHub Actions,
in both shard and aggregate jobs. Original deadlines, assertions, test threads,
warning gates, Cargo profiles, release/live steps and permissions remain.

**13 focused Python tests pass**, including actual GetTempPath2W, an owned real
DOS8.3 alias, same-directory long-path resolution and exact environment-file
output. Identity drift, unavailable identity, links, path injection, missing
roots, API failure/truncation and non-CI use refuse. The complete local Python
suite passes87/0/0. Its first invocation failed because Cargo was absent from
PATH; adding the installed Rust directory for that invocation resolves both
metadata-contract failures. Typecheck and npm260/0/3 existing skips pass.
The changed Rust test passes rustfmt; it adds only elapsed/pipe failure diagnostics
and has not been compiled locally. [Result and exact source/log hashes](result.json).

An independent inert PowerShell/Python stdio probe transfers164 input bytes and
Unicode/quoted arguments with exact expected hashes in0.269 seconds, exits0 and
leaves no active process in its owned Job. This is neither Yoke nor hosted
acceptance, so bootstrap quoting/transport remains unchanged.

Windows REPL shell timeouts, the source-hook subprocess refusal and one library
provider-query deadline remain open. macOS diagnostics additionally expose PDF/
template memory-limit failures, runtime cleanup failures, filesystem case/Unicode
assumptions and timing assertions. No test is skipped, relaxed or counted as
passing by this correction. New hosted confirmation, history archive live/browser,
current VSIX, main/release and complete STATUS acceptance remain pending.
