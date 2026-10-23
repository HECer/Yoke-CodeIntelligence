# Host-only repository marker discovery

`yoke --root C:/explicit/search repositories discover` inspects bounded filesystem
names and `.git` marker metadata. The root must be explicit and absolute. No
Engine or Store is opened. Discovery never reads marker contents, Git configuration,
source or ignore rules, executes Git/project code, or registers/activates projects.
It is a host CLI/library operation, not automatic MCP discovery.

The producer is `yoke-repository-marker-discovery-1`. Each candidate has a relative
`path` (`.` for the search root), `marker` (`git-directory` or `git-file`) and
optional nearest marked ancestor `parent`. That parent is a directory relationship,
not a proven Git superproject/submodule relationship. Invalid or external `.git`
file redirects remain opaque markers. Bare repositories and uninitialized
submodules without markers are not recognized. `complete`, `registrationGranted`
and `sourceAccessGranted` remain false.

`--select "primary/nested"` returns only that exact reported relative candidate.
It does not resolve arbitrary requested paths or create a HostProject. Selection
is literal and case-sensitive, even on Windows; use the path returned by discovery.
Existing host registration and session activation gates remain independent.

| Bound | CLI default | Accepted range |
| --- | --- | --- |
| `--max-depth` | 4 | 0 through 32 |
| `--max-entries` | 20,000 | 1 through 100,000 |
| `--max-directories` | 2,000 | 1 through 10,000 |
| `--max-repositories` | 256 | 0 through 1,000 |
| Global `--max-bytes` | 32,000 | 1 through 4,000,000 |
| Global `--timeout` milliseconds | 120,000 | 1 through 600,000 |

Depth zero checks the selected root's marker and enumerates its immediate names,
then records each unvisited ordinary directory as `depth-limit`. Entry counts
include each yielded directory entry, including the `.git` marker entry; separate
marker metadata probes are bounded by visited-directory capacity. Zero candidate
capacity accepts a scan without candidates and refuses when a marker is found.
Other capacity exhaustion fails the entire call before stdout publication;
there is no silent truncated or empty successful result. Output rows reserve a
conservative serialized-byte allowance incrementally. Library Options use the
same caps with a 1,000,000-byte default report budget.

Protected trees, links/reparse points and unsupported paths have explicit omission
reasons. Marker directories are never descended into. Linked marker files are
refused as candidates; regular marker files are never followed. Linked roots or
ancestors and protected root ancestors refuse. Ordinary directory traversal is
rechecked before use. Results are sorted, repeatable metadata observations, not
an OS-atomic snapshot or a defense against an equal-privilege racing process.

Source/capture/analyzer/provider/write flags refuse before discovery. No existing
capture policy, snapshot/graph ID, session fingerprint or permission is changed.
Discovery does not yet isolate nested repositories during capture, persist their
lifecycle, initialize/fetch submodules, validate repository health or refresh
registrations. Those remain separate work and acceptance requirements.

Windows qualification and actual Git/worktree/junction evidence are recorded in
the [dated report](../../docs/validation/integration-2026-10-23/repository-discovery/README.md).
Unix-only symlink coverage is present in the test source but was not run here.
