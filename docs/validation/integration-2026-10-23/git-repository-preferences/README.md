# Host Git traversal preferences - 2026-10-23

Source `7a31b97f783ae924e31ce151c1b221748a0f92ee` adds explicit root/workspace-bound host controls for
submodule and nested traversal choices. Exact expected IDs and short SQLite CAS
transactions protect update/clear; random incarnations distinguish clear/recreate.
Only a separately authorized trusted capture reuses the booleans. See the
[contract](../../../../native/docs/git-repository-preferences.md).

[index.json](index.json) pins 1,089 frozen inputs, seven product Git blobs and all
176 members of [raw.zip](raw.zip). Every member was reopened and its
size/SHA256 verified. ZIP SHA256 `b79c9f5e25021182814b7b36aa3c02f8bc7b00368e5bf38487b0163227af909a`;
212,118 bytes. External fixture homes/executables stay local.

| Check | Result |
| --- | --- |
| Full library | 577 passed, 7 existing ignored |
| New preference integration target | 8 passed, 0 ignored |
| Other 23 scoped targets | 169 passed, 2 existing ignored |
| Total 25 Rust groups | 754 passed, 0 failed, 9 existing ignored |
| All-target Clippy with warnings denied; locked build | passed |
| Typecheck; npm | passed; npm 260 passed, 3 existing skips |
| Formatting | 353 metadata roots pass; fmt-all OS206 retained |

`git-repository-preferences-qualified1` is the frozen final gate. Tests exercise
cold no-state lookups/refusals, permission/CAS lifecycle, foreign roots/workspaces,
unknown schemas/fields, duplicate/oversized records and expired deadlines. Two
connections sharing one expected ID have exactly one winner. Changing a bound
record or creating one after absent resolution refuses before snapshot/graph
publication. Clear/recreate invalidates preceding IDs. Actual CLI capture reuses
choices, supports an invocation-only disable and still needs current Git authority.

`git-repository-preferences-test1` first passes six new and sixteen existing scope
tests. Its unused-import warning is retained; the import is removed and two
concurrency/staleness cases are added before the final all-target warning-denied
gate. No failure is suppressed or skip added.

The actual native live trial completes 29 owned process calls on
Unicode/apostrophe/CRLF parent/nested repositories. Old/new warm default data and
absent-preference explicit capture IDs match. Save/show/update/clear, stale SQL
preservation, clear/recreate identity and capture overrides pass. Original
snapshot/graph/control rows and all source/Git file hashes remain exact. Every
process starts suspended, joins its owned kill-on-close Job before resume and
ends with no active owned processes. No model calls, automatic project execution,
Developer Mode/system changes or original OS1314 fixture modifications occur.
The protected failed capture_options executable/PDB remains untouched; all-target
Clippy uses metadata checks for that target.

Final CLI SHA256 `337d98b1cbca49d0d5b33587fd8d41927c29438ae1db54c0677d01f5109145a6`.

The original 353-entry inventory, assessments and summaries remain unchanged.
This is dated scoped evidence; default/client adoption, broader selectors/formats,
quality, current whole Windows/main/release and full upstream certification remain
open. No new product test is claimed after documentation updates.
