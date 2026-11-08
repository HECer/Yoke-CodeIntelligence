# Captured per-path owner candidates

```sh
yoke --root /existing/workspace --max-bytes 4000000 git owners REPORT_ID
```

`git owners` replays the saved Git impact/area publication and reads captured
CODEOWNERS text in the same pinned database read transaction. `impactAreas`
retains the complete original parent, including its legacy owner suggestions,
directed parallel observations, source quotes and result identity. Existing
report identity and replay behavior are unchanged. No current source, Git ref,
provider, network, credential, hook, review request or write supplies a fallback.

`ruleSources` separates base/head snapshots and contains every active rule with
its original line, byte range, exact quote, pattern and ordered owner tokens.
Token byte ranges refer to the original file, including Unicode and CRLF bytes.
Each `ownership` row references the unchanged parent's classification ordinal,
side, path and content hash. It exposes every supported matching rule ordinal
and the final supported winner, including the winner's own source binding.
Different rules for the same label retain separate positions and quotes.
An empty winning owner list produces `explicit_empty_owner_list` and `[]`.
Missing captured rules or indeterminate syntax produces a named unknown status,
with null labels/winner and the available supported candidates retained.

The selected file is the first captured exact path among `.github/CODEOWNERS`,
`CODEOWNERS` and `docs/CODEOWNERS`. Uncaptured higher-priority paths remain listed
and `uncapturedPrecedenceMayDiffer` stays visible. Absence in a captured manifest
does not prove absence in the actual repository. Snapshot omission counts remain
visible. Owner labels are authored candidates; no user/team existence, membership,
write permission, branch protection, review requirement or ownership is verified.

The independently owned `captured_codeowners_path_rules_v1` policy is informed
by GitHub's documented [CODEOWNERS rules](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).
It preserves last-match order and empty-owner rules. Its finite pattern behavior
is explicit: simple unanchored patterns match components; leading slash anchors
the root; slashful wildcard terminals match complete paths; literal terminals
and explicit directory patterns include descendants. Thus `docs/*` excludes
deeper files while `/docs/` includes them. Whole-component `**` supports recursive
segments. Matching is case sensitive without normalization; `?` matches one
Unicode scalar. This is not full GitHub parser/schema equivalence.

Negation, character classes, backslash escapes, brace alternatives, traversal,
drive paths, embedded/noncomponent double-star, controls, BOM patterns and
unsupported owner-label syntax remain explicit unsupported active rules.
Any such rule makes every path's winner indeterminate, even when later supported
rules also match. Comments and blank lines create no rules. Duplicate owner
tokens remain ordered evidence. Supported labels are bounded ASCII user/team
spellings or email-like labels; they are never authenticated identities.

`--options /absolute/host/options.json` is a strict object with `areas` (the
existing full [area options](git-areas.md)) and `limits` (all six fields below).
Unknown, missing or duplicate fields refuse. There is no automatic execution
configuration. CLI source/provider/write flags refuse before opening the Store.

| Field | Default and maximum |
| --- | ---: |
| `maxRuleBytes` across both physical selections | 2,000,000 |
| `maxRules` across both selections | 4,096 |
| `maxOwnerTokens` | 16,384 |
| `maxMatches` (all supported matches, before choosing winners) | 65,536 |
| `maxMatchWork` (pattern-byte × path-byte charges per evaluation) | 32,000,000 |
| `maxOutputBytes` | 4,000,000 |

Each selected file is at most 1,000,000 bytes; each line at most 4,096 bytes;
each supported pattern at most 1,024 bytes; each rule at most 50 owner tokens,
each supported owner label at most 256 bytes. Both source-byte selections are admitted before new
rule reads; the parent's separate replay can already read captured source.
Identical CAS hashes do not deduplicate physical selection or rule charges.
All supported matches are charged and preserved; quotas never clip results.
Zero remains zero. Original parent and new row serialization are admitted under
the output budget, with a final whole-result check including the result ID.
CLI `--max-bytes` additionally includes its final newline and does not alter the
logical report. A small query budget may refuse the entire request.

The parent retains its independent metadata/source replay bounds. New counters
are not a total memory or hard scheduling quota. Deadline/cancellation and SQL
policy are scoped and restored; caller-owned transactions refuse without being
closed. No Store/source record, artifact, registry or latest pointer is created.
Output reports `ownerIdentityVerified: false`, `githubPermissionVerified: false`,
`complete: false` and zero Git/network/model operations. Wider ownership,
compiler semantics, merge safety, pushed-ref freshness and full upstream/R5
acceptance remain separate work.
