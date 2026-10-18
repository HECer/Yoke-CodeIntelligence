# Explicit read-only ignored sources

Hosts can set `CaptureOptions.includeIgnoredDirs` to normalized relative directory
prefixes. An empty list is omitted from serialization and preserves released
default and configured capture identities. This option captures eligible ignored
source under those prefixes as read-only data; it grants no execution authority.
The normal secret, binary, link, internal directory, file and corpus limits remain
effective. Dependency directories still require their separate host scope.

The native walk runs before the explicitly selected walks. Each captured file has
an immutable path, source hash and `ignoredAtCapture` observation in the strict
`native_capture_scope_ignored_v1` record. The policy name is
`observed_native_ignore_filter_at_capture`: these are observed native filter
outcomes, not a reconstruction or cryptographic attestation of every ambient
ignore rule. Both scans compare file contents, omissions and outcomes. The full
member table, options and proof hash bind the new snapshot identity. Missing,
future, incomplete and reordered proofs fail closed.

All files under an opted-in prefix are read-only, including files admitted by the
ordinary walk. Existing and prospective write paths are checked before a derived
snapshot or change plan is published. Windows case and existing DOS short-name
directory aliases resolve through the same restriction. Ordinary edits outside
these prefixes remain available. Derived snapshots, editor overlays and offline
archives preserve the original metadata and forbid changes to read-only members.

`ci_find.skipIgnored` is a strict boolean for captured symbol, file and text
search. Its native default is false. Filtering happens before pagination and
source reads. Session `fs.list` and `fs.glob` support the same option. The Serena
facades preserve their pinned defaults: `list_dir` and `find_file` use false;
`search_for_pattern` uses true. Basename file masks search recursively. The flag
only filters already captured files and never triggers additional capture.
Changing current ignore files cannot change queries against an older snapshot.
Registered external project directory queries use the same strict captured-member
projection as active projects, without activation or provider execution. Removed
registrations and unregistered roots are rejected before project storage is opened.
Malformed directory arguments are rejected before capture publication.
Recursive listing includes every captured directory level; nonrecursive listing
includes the first level. Directory membership comes from immutable captured file
prefixes before filtering. A captured file is not a directory. Empty or otherwise
uncaptured directories are explicitly unknown, rather than inferred from current
filesystem state. Large directory projections fail explicitly at 100,000 distinct
directories or 32 MB of directory names; refining the requested path preserves the
entire captured projection within those limits.

Historical Git production with this opt-in remains unavailable until an
independent commit/tree-bound ignore outcome receipt exists. A working-tree
observation is never reused as authority for a historical commit. Existing
historical workflows without the new option keep their prior contracts.

Windows validation includes real DOS8.3 alias denial, source-free filtered reads
with a deliberately corrupt ignored blob, unchanged released identities, strict
proof negatives, guarded plans, editor buffers, retention and offline archives.
Linux and macOS runtime certification remain deferred.
