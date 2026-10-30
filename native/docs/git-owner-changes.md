# Captured owner-rule changes across all saved paths

```sh
yoke --root /existing/workspace --max-bytes 8000000 git owner-changes REPORT_ID
```

This separate read-only query compares the union of paths in the saved base and
head source manifests. It includes unchanged files whose declared candidates
change when only CODEOWNERS changes. The complete original impact/area parent,
its result identity, directed parallel observations and source evidence remain.
The existing `git owners` options, policy and raw result identities are unchanged.

Both sides use the same independently owned `captured_codeowners_path_rules_v1`
selection/parser/evaluator as [the existing owner query](git-owners.md). Every
original rule, ordered duplicate owner token, UTF-8/CRLF byte range and quote
stays qualified by side, source path, source hash and snapshot. Unsupported or
missing selected rules produce indeterminate rows with supported matches retained.
An explicit empty-owner list remains different from missing or unmatched rules.

Changes distinguish added/removed captured paths, changed ordered candidates,
changed winning-line/token evidence and indeterminate interpretations. Snapshot
IDs or the complete rule-file hash alone do not change an otherwise identical
winning line. Changing its source path, original line bytes or byte/token
positions does. A comment or unrelated rule appended after the winner can leave
the selected assignment unchanged while both complete rule sources remain visible.
Earlier matching alternatives remain in those sources and in optional unchanged
rows; the comparison concerns the final selected assignment and winning evidence.

`sourceContentChanged` compares exact saved file hashes independently of these
candidate changes. New source validation rereads and hashes every captured file
body in both manifests, including repeated identical CAS selections. It performs
no current-file, Git-ref, provider, network, model or project-execution fallback.
It creates no record, artifact, registry entry or latest pointer.

Owner labels remain authored candidates. No account/team existence, identity,
write permission, branch protection or actual review requirement is established.
GitHub uses base-branch rules for review requests; this base/head authored-rule
comparison requests no review. See [GitHub's CODEOWNERS documentation](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).
Only captured paths and selected captured rules are compared. Omission counts
and uncaptured higher-priority rule locations remain visible; neither an empty
change list nor `allCapturedPathsEvaluated` proves repository-wide completeness.
The result always retains `complete:false` and the unverified authority flags.

Host `--options /absolute/options.json` contains `areas` (the existing complete
[area options](git-areas.md)), `limits` (all fields below) and an optional
`includeUnchanged` boolean, default false. Unknown, duplicate or missing required
fields refuse. Default invocation requires no options file. Source, server,
snapshot, analyzer, trust and write overrides refuse before Store initialization.

| Limit | Default and maximum |
| --- | ---: |
| `maxPaths` (distinct captured union) | 20,000 |
| `maxManifestEntries` (both physical sides) | 20,000 |
| `maxRows` (emitted rows) | 20,000 |
| `maxFileBytes` | 4,000,000 |
| `maxSourceBytes` (both physical body selections) | 64,000,000 |
| `maxRuleBytes` (both physical rule selections) | 2,000,000 |
| `maxRules` | 4,096 |
| `maxOwnerTokens` | 16,384 |
| `maxMatches` | 65,536 |
| `maxMatchWork` (pattern-byte × path-byte charges) | 32,000,000 |
| `maxOutputBytes` | 8,000,000 |

The shared rule parser also retains its existing per-file 1,000,000-byte,
per-line 4,096-byte, pattern 1,024-byte, token 256-byte and 50-token-per-rule bounds.

Both manifest, file, source-byte and rule-byte reservations precede new CAS reads.
The parent's separately bounded replay has already occurred. All paths and all
supported matches are evaluated even when unchanged rows are omitted. Quotas
refuse the whole result, never clip it. Zero remains zero. The final canonical
result, including `resultId`, must fit; CLI `--max-bytes` separately includes its
one final newline. These are logical/input bounds, not a total heap or hard CPU
quota. The shared deadline, caller-transaction refusal and scoped SQL/connection
policy remain in force. A missing or corrupted captured source refuses.

Windows scoped evidence currently covers six new integrations plus 65 existing
Git regressions, formatting and all-target Clippy. Actual CLI controls preserve
the old owner result bytes, repeat the new result, enforce the exact newline
budget and leave every owned source/Store/Git file unchanged. All 993 validated
inputs match the integrated primary checkout byte-exactly.
Primary Cargo freshly builds its library and CLI; formatting, typecheck and
TypeScript260/3 existing skips pass. The independent actual primary CLI also
passes, with the complete new stdout exact across both binaries. Full R5,
parser/upstream,
platform, runtime/review truth and the complete current native suite remain open.
