# Scoped Windows native runtime acceptance

Actual source: `6b9a60dda07d546ce09108cf40018a4fe29e3099`, Rust/Cargo 1.94.1,
`x86_64-pc-windows-msvc`, unchanged default features and Cargo profiles.

| Actual native group | Passed | Failed | Existing ignored | Filtered out |
| --- | ---: | ---: | ---: | ---: |
| Complete library group | 703 | 0 | 7 | 0 |
| Complete `process` integration group | 7 | 0 | 0 | 0 |
| Complete `agent_repl` integration group | 22 | 0 | 0 | 0 |
| Each of four exact `compiler_inline_body` regressions | 1 | 0 | 0 | 24 |

The first three groups run serially without a name filter. Existing ignored
library tests keep their original annotations; `runs.json` names all seven.
They are not promoted, executed or counted as passes. No ignore or assertion
is added. The four selected body tests are distinct scoped executions, not a
complete 25-function body group or four whole Cargo targets:

- `body_free_references_keep_separate_type_value_and_namespace_bindings`
- `body_namespace_classification_still_refuses_actual_lexical_shadows`
- `complete_identifier_assignments_keep_write_order_completion_and_branch_ownership`
- `effectful_targets_and_compound_or_consumed_assignments_refuse_without_publication`

The complete process and REPL groups now execute the d80 phase-instrumented
tests successfully here, including managed known folders and the explicitly
selected shell cell. This is local Windows evidence at the pinned source. It
does not explain the older hosted failures or establish current GitHub passes.
The body regressions exercise actual compiler observations and independent
native admission. The full library group also executes the new original-source
AST/admission and scoped SQLite callback tests. These scopes supersede earlier
local execution-pending notes for those named tests, not full R1/STATUS claims.

## Fresh producer and image ownership

Four separate `cargo test --no-run --locked -j 1 --message-format=json` commands
build the process, REPL, body and library targets. Each has Cargo exit 0,
build-finished success, zero warning/error diagnostics and its owned tree empty
before forced cleanup. Each selected test artifact is `fresh=false`. Integration
builds also report ordinary executable byproducts; those byproducts are not
hashed, adopted or executed here. Protected plain/default Yoke images and PDBs
remain outside image selection.

The earlier [d80 library build](../native-windows-typing/README.md) remains an
incomplete owner result despite Cargo 0. This later library build deliberately
recompiles identical source bytes: the working `native/src/lib.rs` mtime is
temporarily advanced, then restored in `finally`. Its SHA-256 and file identity
remain exact. No content, compiler flag, profile or deadline changes. The
original and restored mtimes and byte hashes are in the archived invalidation
receipt. A fresh successful producer qualifies the later image independently;
it does not relabel the earlier result or identify that surviving child's cause.

Every selected image is an ordinary non-reparse file with one link in the
expected Cargo deps directory. A pinned read handle denies concurrent writers
and deletion throughout hashing/execution. Identity and SHA-256 are checked
again afterward. The archive contains receipts and logs, no image or PDB bytes.

All eleven owned build/test runs start suspended, join a private kill-on-close
Job before resumption, and observe the tree empty within the original one-second
drain before cleanup. Root exit and empty post-cleanup accounting are proved.
Some post-root Job member queries return access error 5 before the member exits;
those image identities remain unknown, rather than being labeled benign.
The initial incomplete library receipt is separate from these eleven runs.

The unchanged 1.2-GB admission is checked before each build and image read;
the 500-MB running reserve and finite log/time/cleanup budgets remain. Later
volume free space rises above admission independently; no exclusive cause is
assigned to that host change. All 1,003 selected native/Cargo/workflow byte
hashes match before/after and across all eleven runs. This selected-input map
is not every build/environment/dependency input. A separate post-regression
dependency observation records installed TypeScript 5.8.3 and Node v24.21.0;
it is not retroactively presented as a pre-run dependency freeze.

`raw-evidence.zip` preserves original stdout/stderr, per-run input maps, owner
receipts, selected-image receipts and actual private control scripts. Every
member is reopened and verified against `raw-index.json`. Expected caught panic
and failure-injection diagnostics in stderr remain intact; the original parent
libtest summary and owned exit determine the recorded group result.

Current hosted PowerShell acceptance, remaining default native target groups,
Unix execution, actual rust-analyzer/release/history/browser/VSIX qualification,
the complete inventory and R1–R9, main and release remain open. These results do
not turn a partial application/platform qualification into full completion.
