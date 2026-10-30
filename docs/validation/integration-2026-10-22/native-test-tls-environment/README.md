# Native test TLS-environment follow-up

Code Intelligence 0.5.0. Source d87dd7e2f9f91203285cfcdb8149d7d2a40a6f98; base 2ea772a1a343e2b326f0febf4ac1d850ada6cec5.

The reported Ubuntu job passes fmt/Clippy but fails seven Google HTTP library
tests and three CLI tests at the intentional ambient-TLS refusal. The complete
job logs, metadata and retrieval errors are retained in the indexed raw ZIP.
The first gh run-view request cannot retrieve logs while other matrix jobs run;
the completed job API provides them. Its raw output is captured before terminal
escapes are removed for inspection. No CI status polling is used.

scripts/run_native_tests.py now removes only SSL_CERT_FILE, SSL_CERT_DIR and
SSLKEYLOGFILE from a copied child environment. It forwards the same Cargo test
arguments as before, including Windows/Unix thread limits and no-fail-fast.
The caller's environment is unchanged. The native subtree is byte-identical at
the Git-tree level to the base commit: no production refusal or adversarial test
is weakened. Existing CLI negatives explicitly reinject each override and still
refuse it. This fixture launcher grants no application network/source authority.

An isolated Windows worktree runs 13 Google HTTP library tests and four CLI tests
with all three synthetic ambient overrides present in the launcher. All 17 pass,
including genuine owned wire/TLS refusal, empty private-pipe deadline and explicit
override refusals. Nineteen Python tests pass; the two new tests execute real
child processes to prove environment isolation, literal argument forwarding and
nonzero exit propagation. Focused Clippy -Dwarnings, npm ci/typecheck, npm260 with
three existing skips and all338 actual Rust format roots pass. Validation binds
1041 frozen inputs and empty Windows Jobs. The reported Ubuntu all-target Clippy
pass remains evidence for its exact unchanged Rust tree; local checks are scoped.

raw-index.json gives a size/SHA256 for every ZIP member. The one later STATUS
edit is separately hashed in post-validation-docs.json. Primary pending symbol-
budget work stays outside these commits. No system settings, OS1314 fixtures,
protected processes, original inventory assessments or release flags change.
The next Ubuntu/Windows/macOS CI run remains pending; no remote green result or
full native/Windows/R1-R9/353/main/release completion is asserted.
