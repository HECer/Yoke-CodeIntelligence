# Authored skills and takeaways

The native Rust lifecycle independently implements the pinned Trail skill workflow: learn a correction, list or show skills, confirm another author's takeaway, fold waiting takeaways into a versioned skill, and publish a personal skill into the repository. These are application records in an explicitly selected personal Store. They are separate from the central AgentMemory service and confer no execution authority.

Use an existing absolute repository and personal directory, plus an explicit stable project ID:

```text
yoke --root <repository> skills <operation> --personal-root <personal-directory> --project <sha256-project-id> --args <JSON>
```

Operations are `learn`, `list`, `show`, `confirm`, `fold`, `publish`, and explicit interrupted-publication `recover`. Personal mutations require `--allow-personal-writes`; changing a repository skill additionally requires global `--allow-writes`. The CLI validates these explicit roots before opening a source Engine. It does not discover a home directory, read Git author configuration, launch a provider, or execute skill content.

`learn` takes `skill`, `text`, and `author`, with optional `date` (`YYYY-MM-DD`), `description`, and `section`. Its expected revision is null for a new skill and the exact current `revisionId` for an existing one. Names follow the pinned lowercase/NFKD/ASCII eight-word slug convention; `trail` and `graft` are reserved. New skills start at version 1 in the personal namespace. An existing repository skill takes precedence over a personal copy.

`list` returns skill metadata and revision/file hashes. `show` returns the authored body followed by waiting corrections, newest date and filename first. Public takeaway metadata includes its skill, actual presentation path, authored date, teacher, confirmations, optional section, and folded version. `where: "home"` means the explicitly selected personal Store, not an implicitly discovered operating-system home.

`confirm` accepts an optional known takeaway ID, filename suffix, or returned path. This is a string selector over existing takeaways; it never grants an arbitrary file read. The teacher's own confirmation and duplicate confirmations compare Unicode lowercase names and do nothing. These names are declared labels, not verified human identities.

`fold` inserts waiting corrections under their named level-two heading or `Takeaways`, preserves other frontmatter fields and comments, and increments the skill version once. `confirmed: true` selects only corrections with another declared confirmer. A no-op leaves the version and revision unchanged. Folded takeaway history remains available. `publish` moves only the personal `SKILL.md` presentation to `.claude/skills/<name>/SKILL.md`; personal takeaway files and immutable history remain.

Named mutations bind `expectedRevision`. Fold and publish also bind `expectedFileHash` from the checked prior result. A fold without `skill` accepts `expectedRevisions` and `expectedFileHashes` maps covering the current library. All predictable input admission occurs before publication. Batch results explicitly state `atomicAcrossSkills: false`: each skill has its own durable journal.

The host CLI fills only missing revision and file-hash guards through a bounded read-only preflight in the same authorized command. Explicit supplied guards remain unchanged and stale values fail. Direct library `invoke` continues to require its checked guards. This convenience performs no personal-state registration or repository writes.

Original and proposed presentation bytes are hash-addressed. File changes use the exact originally observed bytes as preconditions; a later manual edit is never adopted as a replacement precondition. Prepared journals bind their strict revision, project, repository, and rendered file projections. Read-only queries refuse pending operations. Explicit recovery or the next valid authorized mutation replays a journal, accepting only the original or already-applied bytes and refusing unrelated edits. SQLite state and filesystem changes are not described as one atomic transaction.

The implementation bounds source files, metadata, history scans, query output, and parsing work. Admission happens before publishing a journal. Retention validates the exact revision, head, operation, and receipt schemas and their artifact bytes before recognizing their roots. Unknown schemas or corrupt projections fail closed.
