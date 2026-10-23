# Explicit Git repository scopes and working-source capture

```powershell
yoke --root C:/explicit/project git repository-scope --allow-index-read --trust-git
yoke --root C:/explicit/project git repository-scope --capture --allow-index-read --trust-git --follow-submodules --follow-nested-repositories
```

Both operations require explicit host index-read permission and trusted Git. An
optional `--git` selects the executable outside the source root. The metadata
operation opens no Store and reads no Git blobs or working-source contents. The
explicit `--capture` operation additionally reads the selected saved source and
publishes an immutable native snapshot and structural graph. Neither operation
executes analyzed code, fetches/initializes submodules or grants a model permission.

Gitlinks (index mode160000) and untracked nested-clone candidates remain distinct.
`--follow-submodules` and `--follow-nested-repositories` are independent; both
default to false on each invocation. Every followed child needs a regular own
`.git` marker and a Git-reported top-level root matching the selected physical
directory. Missing markers are explicit states; broken selected repositories
fail instead of falling back to an apparently complete filesystem result.

Git runs through the existing hardened HostGit path: hooks/fsmonitor, lazy fetch,
external diff, signature display, implicit submodule recursion and protocols are
disabled. Child enumeration preserves its own ignore rules. Tracked files can
remain selected despite ignore rules. Symlinks/reparse points, protected paths,
unsafe names and ambiguous physical roots cannot expand source authority.
The native `.yoke` tree is excluded at every level, including case variants, so
cache publication cannot change the selected index observation.

The strict NUL protocol retains original paths, object IDs, modes and parallel
conflict stages. Each repository's index listing is read twice and must match.
The report producer is `yoke-git-repository-scope-observation-1`; its complete and
grant/publication fields remain false. These are trusted-host observations, not
independent Git attestation, committed-source bytes or execution evidence.

| Limit | Default | Maximum |
| --- | --- | --- |
| `--max-repositories` | 64 | 64 |
| `--max-entries` | 20,000 | 20,000 |
| `--max-index-bytes` | 8,000,000 | 8,000,000 |
| `--max-depth` (repository nesting) | 16 | 16 |
| Metadata global `--max-bytes` | 32,000 | 4,000,000 |
| Capture `--max-scope-bytes` | 1,000,000 | 4,000,000 |
| Global `--timeout` milliseconds | 120,000 | 600,000 |

Zero is a real bound. A repository limit of zero refuses even the root; zero entry
capacity accepts only an empty listing. Zero index-byte capacity permits no listing
acquisition. Depth zero permits the root and refuses a requested child traversal.
Exhaustion returns an error, never a silently truncated complete result. Index
bytes are shared across the initial observation and capture's final recheck;
fixed-size root/format queries have their own bounded outputs. Normal native
per-file/source capture caps still apply. Metadata and source deadlines are
cooperative, not a hard OS scheduler quota or race-isolation certificate.

New captures bind `repositoryScope.policy: exact-host-git-working-files-v1` into
the existing immutable capture-options proof. Original index observations remain
separate from actual working-file bytes, including dirty files. Physical Windows
names are normalized and checked before source reads; original observed aliases
remain separate bindings. Exact file membership and read-only crossed-repository
roots are stored. Native double source scans and a final index/physical-selection
recheck precede source-proof publication. Unfollowed, missing, protected and
unsupported boundaries retain explicit snapshot omission reasons.

Captured and current invocation write restrictions are independent. Included
children are read-only; ordinary selected parent files retain existing guarded
edit authority. Exact membership excludes new-file creation and moves outside the
saved selection. Approval, executed verification, stale source preconditions and
recovery journals remain required. The live trial applies an approved parent edit
and restores its exact original bytes through the journal.

Pure replay checks the stored metadata schema, hashes, protocol, ownership and
physical-name bindings without resolving historical roots or starting Git. Typed
retention and copied collections preserve that proof after the source disappears.
Saved-scope staleness checks reuse the fixed file selection without live Git;
changed/deleted observations publish no new capture-options proof or graph.
This supports guarded workspace application with Git administration unavailable.
It does not rediscover new files, ignore rules or repository boundaries.

Absent/null repositoryScope keeps preceding configuration bytes, snapshot IDs and
default behavior. Generic capture configuration cannot create a fresh Git-selected
capture; use the explicit trusted operation. Existing snapshot query/plan paths
can consume the checked saved selection. Older binaries explicitly reject the
new field while retaining access to preceding captures. No automatic preference
reuse, default/client adoption or whole-upstream/whole-Windows certification is
claimed. Call the explicit operation again with the desired follow choices.

The independently implemented observable distinction was checked against pinned
[Trail walking](https://raw.githubusercontent.com/trailhq/Trail-Code/3f10899e2fcee1fc990e8c11f27085016d79d0de/src/ingest/fs.ts)
and [build-choice state](https://raw.githubusercontent.com/trailhq/Trail-Code/3f10899e2fcee1fc990e8c11f27085016d79d0de/src/util/state.ts).
The implementation and bounded host authority differ; source-name agreement is
not full behavioral, ranking, runtime or performance equivalence.
