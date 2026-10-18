# Authored decision and preference records

The native `decision_records` library owns authored repository decisions and personal preferences. These application records are separate from central AgentMemory, Serena named notes, source observations and skill takeaways. They never grant execution, model, network, filesystem or Git authority.

A host constructs `DecisionStore::new(source, storage, scope, deadline)`. Repository scope requires the source store itself. Personal scope requires a separate, explicitly opened `Store` and a stable host label. Its durable host pin rejects accidental reuse with a different host label. There is no home-directory discovery, implicit account lookup or synchronization.

`write(snapshot, draft, expected_revision, deadline)` creates an immutable revision and moves its named head in one SQLite transaction. An absent expected revision means create only. Updating requires the exact current revision ID; stale revisions fail. Authored authors, calendar dates and optional costs are attribution supplied by the caller, not authenticated identity or measured billing. Captured paths record the source files touched by the authored record, not proof that an author edited those files. Prior revisions remain immutable.

Personal publication first commits conservative dependency pins in the source store so source-history retirement cannot invalidate personal records. Pins contain only source snapshot/hash anchors, opaque store/revision IDs, a hashed host binding and parent pin links. Personal titles, bodies, authors and costs stay in the personal store. If subsequent personal publication fails, orphan pins deliberately keep source history. There is currently no automatic pin-release or record-deletion operation. Removing a personal store externally does not automatically release its source pins. Personal records still require their original source store for validated reads and exports; this is not a self-contained source archive.

Drafts carry a lowercase ASCII slug `key`, `kind` (`decision` or `preference`), `title`, `body`, `author`, `date` (`YYYY-MM-DD`), explicit `paths`, optional `cost` (`currency`, integer `microunits`, `basis`) and an optional array of `rules`. Schemas reject unknown fields. Source anchors bind the workspace, immutable snapshot and exact file hashes. Import and write validate these bindings against the source store; live files cannot silently replace captured bytes.

## Explicit Markdown bundles

`export_bundle(destination, deadline)` requires an absolute, fresh destination with an existing ordinary parent directory. The bundle contains a strict versioned `manifest.json` and a deterministic Markdown document for each head. The manifest preserves the entire parent chain for every exported head. Markdown escapes authored markup and uses a fence that authored text cannot close. The exact original body remains in the manifest.

No existing destination or Markdown file is overwritten. Export never stages or commits files. Select a fresh directory in the repository when the documents should be reviewed and committed. The destination is a host input; indexed source cannot select it.

`import_bundle(directory, expected_heads, deadline)` accepts only this authored bundle schema. Every Markdown projection must exactly match its manifest, and the directory must contain exactly the expected ordinary files. Unknown files, links, invalid UTF-8, duplicate JSON keys, mismatched source bindings, malformed revision chains, stale expected heads and exceeded bounds fail. Import validates all records before atomically admitting immutable revisions and moving heads. It never edits source or imported documents.

Expected heads explicitly name every imported key and its expected current revision, or `null` for create only. A repository bundle can be explicitly imported into a personal store because authored revision identities are source-bound, while each destination head is separately namespaced. Import into an unrelated source workspace is refused. This is local authored-record portability with validated source history, not an arbitrary Markdown ingestion or cross-workspace synchronization format.

Updating an existing imported key must preserve its current revision in the imported ancestry. Reimporting the same head with its exact expected revision is allowed; moving a head to an older or unrelated authored history is refused even when the caller supplies the current CAS value.

## Local literal checks

Rules are opt-in authored data:

- `follows`: require a literal in an explicitly anchored candidate file.
- `ruled_out`: require a literal to be absent in that file.
- `changed_value`: report a literal transition only when the old literal is present and the new literal absent in the baseline, with the inverse observed in the candidate.

Each result quotes the authored rule, record revision, baseline and candidate source hashes, and exact UTF-8 byte spans for literal matches. Absence is scoped to the complete bounded captured file. A missing candidate file is inconclusive. Checks return `literal_heuristic_not_semantic_proof`: text in comments and strings counts, equivalent implementations are not recognized, and these checks do not infer intent, behavior or universal compliance. No model or external command is started.

All reads, writes, imports and exports have byte/count/deadline admission limits. The filesystem checks reject links and reparse points but do not provide an OS-atomic defense against another process with equal privileges replacing files concurrently. Preserve ordinary source-store protections and do not share a live SQLite database over a network filesystem.
