# Host CLI for authored decisions and preferences

`yoke decisions` manages local authored records bound to immutable captured source. These records are distinct from central AgentMemory, Serena named notes and skill takeaways. Author names, dates and optional costs are supplied attribution. They do not authenticate authorship, measure billing or grant execution authority.

All operations accept one strict JSON object through `--input /absolute/host-input.json` or `--args '{...}'`. Inputs are limited to 128,000 bytes; duplicate keys and unknown operation fields are rejected. Omitting both input options supplies `{}`, which is useful for `list`. Do not pass secrets as authored content or command arguments.

```sh
yoke --root /absolute/project decisions list
yoke --root /absolute/project decisions write --input /absolute/host/write.json --allow-record-writes
yoke --root /absolute/project decisions read --args '{"key":"storage-policy"}'
yoke --root /absolute/project decisions check --args '{"key":"storage-policy"}'
```

The independent host switch `--allow-record-writes` is required for `write` and `import`. The CLI checks it before opening source or personal state. `--allow-writes`, repository JSON, authored records and imported bundles cannot substitute for this permission. Record operations never launch model providers, language servers, network services or project code.

The source root, optional `--snapshot`, `--capture-config` and `--include-ignored-dir` use the ordinary native source-selection workflow. Without a snapshot, a fresh saved-source capture is selected. With `--snapshot`, checks and new revision anchors use those historical immutable bytes. `read` reports anchor currentness against the selected snapshot; `check` compares authored literal rules against it. A check is explicitly a local literal heuristic, not semantic verification or proof that absent text is absent from uncaptured files.

## Creating and updating revisions

An example `write.json`:

```json
{
  "draft": {
    "key": "storage-policy",
    "kind": "decision",
    "title": "Keep source snapshots immutable",
    "body": "Retain captured bytes so earlier evidence stays reviewable.",
    "author": "Ada",
    "date": "2026-10-15",
    "paths": ["src/lib.rs"],
    "rules": [
      {"kind": "follows", "path": "src/lib.rs", "literal": "pub mod"}
    ]
  }
}
```

Use `kind: "preference"` for authored preferences. Optional costs have the form `{"currency":"EUR","microunits":1200000,"basis":"supplied estimate"}`. Rules also support `ruled_out` with `path` and `literal`, and `changed_value` with `path`, `before` and `after`. Every rule path must appear in `paths`.

A missing `expectedRevision` means create only. For an update, include the exact current revision ID at the top level beside `draft`. A stale ID fails without moving the named head. Every successful revision preserves its parent ID, authored draft and exact captured source anchors; source files remain untouched.

## Explicit personal storage

Repository records use the source workspace's own native store. Personal records require both an existing absolute `--personal-root` directory and an explicit `--host-id` label. The directories must be distinct. Personal root ancestors must be ordinary directories without links or parent traversal. Host labels are nonempty, at most 200 UTF-8 bytes, and contain no control characters.

```sh
yoke --root /absolute/project decisions write --input /absolute/host/write.json --personal-root /absolute/personal-store --host-id workstation-a --allow-record-writes
yoke --root /absolute/project decisions list --personal-root /absolute/personal-store --host-id workstation-a
```

The personal store pins its host label and rejects a different label later. It maintains separate named heads for each source workspace. Repository and personal records do not appear in each other's lists. There is no home-directory discovery, implicit account lookup or automatic synchronization.

## Reviewable Markdown bundles

```sh
yoke --root /absolute/project decisions export --args '{"destination":"/absolute/review/new-decisions"}'
yoke --root /absolute/project decisions import --input /absolute/host/import.json --allow-record-writes
```

Export requires a fresh absolute destination with an existing ordinary parent directory. It creates a strict manifest and deterministic Markdown projections of the named heads, preserving revision history in the manifest. Existing destinations and documents are never overwritten. A fresh directory inside the repository can be selected for normal human review and version control; the CLI does not stage or commit it.

Import input explicitly names the bundle directory and every expected destination head:

```json
{
  "directory": "/absolute/review/new-decisions",
  "expectedHeads": {"storage-policy": null}
}
```

Use `null` for create only, or the exact current destination revision ID for an update. Import validates the whole bundle, source bindings, revision chains and every Markdown projection before atomically admitting revisions and moving heads. Edited projections, extra files, links, unrelated source workspaces and stale expected heads fail. The bundle's documents and source files are preserved. Add the personal scope flags to import into the explicit personal store instead.

`--max-bytes` applies to the complete JSON response including its newline. If the full result exceeds that output budget, the CLI stores it as an immutable artifact in the selected record store and returns a bounded `artifactId` reference with `ci_read` instructions. Personal results stay in the explicit personal store; their reference includes that store's host root in `readWith.root`. A partial transport response does not mean a partial write, import or check.

See [decision-records.md](decision-records.md) for library schemas, persistence, retention and trust boundaries.
