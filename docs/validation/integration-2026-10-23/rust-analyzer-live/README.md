# Actual current Windows Rust Analyzer navigation

Code Intelligence native 0.5.0 / retained TypeScript 0.3.0, 2026-10-23.
Selected source 41d66b04accd89277ca2ae13a6247f82c0ce286c; native product code remains
666656bae3c4e5a0abb9918d943505098266e455. No product or test source is changed
for this acceptance.

## Result

Both existing explicit Rust Analyzer live gates pass: **2 passed, 0 failed,
0 ignored, 0 measured, 2 filtered out**. They were originally ignored host
tests, explicitly selected using actual_rust_analyzer and --ignored. The two
other live functions are not run. These counts are separate/nonadditive to
the historical default Windows baseline and previous scoped 788-test result.

- actual_rust_analyzer_definition_references_and_rename checks the actual
  provider's definition/reference results and a prepared rename, including
  unchanged original Unicode source after planning.
- actual_rust_analyzer_mcp_references_bind_immutable_snapshot changes the live
  disk source after capture, then checks actual MCP references against the
  historical snapshot ID, exact original content hash and original UTF-8 byte
  slices. Current disk bytes remain unchanged by the semantic request.

The observed provider is rust-analyzer 1.94.1 (e408947b 2026-03-25), SHA256
8780a0a408169100c7b4df49ba2c9dcc4b40b48dd375e3985bd585f1a4fe0a57. It is selected through the explicitly trusted
host PATH with the existing RustAnalyzer passive profile and no source-execution
grant. The provider executable hash remains exact through the run. This is no
OS sandbox, binary supply-chain certification or attestation of transitive effects.

All **1,109** frozen source/control inputs stay exact. Original Rust fixtures,
ignore annotations, internal deadlines, Cargo profiles and grants are unchanged.
The current debug CLI stays SHA256
aa391752ae06896894e0610e65f7a006ac8757dccf32eb19428882e5cf72f7c7. Cargo reports its existing library/binary fresh;
only the current live test harness relinks. Compile time is 9.57 seconds;
the two tests finish in 35.19 seconds. No concurrent Cargo, source edit,
privilege change, Developer Mode or release-artifact substitution is used.

The launcher assigns each suspended child to a reviewed kill-on-close Windows
Job before resume. Both provider-version and full Cargo/test parent Jobs end
with zero active processes after cleanup. Private home and temporary directories
are used. A bounded free-disk stop protects critical headroom; no stop is needed.

## Evidence and limits

[raw.zip](raw.zip) has 8 reopened members, 83,939 bytes,
SHA256 0f58b26e2b064bb0fd0fa63f60396e8b276ec6f7c3133d7a97aebdeee3f9080d. [index.json](index.json) lists every member's checked
length/SHA256 and the exact selected executable/harness bindings. Controllers,
raw Cargo/compiler/test logs, source hashes and structured results are preserved;
no executable, cache or private profile contents are included.

[post-review-docs.json](post-review-docs.json) records four documentation-only
changes after the run: STATUS, REMAINING_WORK, native README and one additive
reconciliation key. The native README now points to the accepted complete
formatting command. No later product test is claimed. Removing the new key
leaves the entire preceding reconciliation deep-equal; original 353 inventory,
73 unreconciled assessments, 77 runtime unknowns and false full certifications
remain. Python47/typecheck/npm260+3Skip and the complete361-root format gate
retain their separate source-41d66b04 [qualification](../native-formatting/README.md).

This proves only the selected real Windows navigation/MCP operations. It does
not qualify new resource-setting behavior in this vendor, all backend/runtime
versions, all provider effects, other languages, Linux/macOS, current full-native
or hosted checks, canonical bundled VSIX, UI, broader R1-R9/quality/Google,
main integration or release. Those requirements remain open.
