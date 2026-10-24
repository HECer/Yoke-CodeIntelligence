# Host Git traversal preferences

The host can persist the two repository traversal choices for subsequent explicit
`git repository-scope --capture` calls. The control is local to one canonical root
and Store workspace. It is separate from immutable snapshot evidence.

```sh
yoke --root /absolute/project git repository-preferences show
yoke --root /absolute/project git repository-preferences set --allow-host-writes --expect absent --follow-submodules true --follow-nested-repositories false
# Use the returned selection.id for every subsequent update or clear.
yoke --root /absolute/project git repository-preferences set --allow-host-writes --expect SAVED_ID --follow-nested-repositories true
yoke --root /absolute/project git repository-scope --capture --allow-index-read --trust-git
yoke --root /absolute/project git repository-scope --capture --allow-index-read --trust-git --no-follow-submodules
yoke --root /absolute/project git repository-preferences clear --allow-host-writes --expect CURRENT_ID
```

`show` opens initialized state read-only and does not create a Store when absent.
SQLite may coordinate existing WAL state through sidecars. `set` and `clear` need
`--allow-host-writes` and an exact `--expect` value. `absent` admits only an absent
selection. At least one boolean patch is required for set; omitted fields retain
the preceding choice. Clearing an already absent selection with `--expect absent`
is a no-op and creates no Store. Allowed initial set initializes ordinary `.yoke`
state; it writes no project source and starts no Git process.

Updates use one short immediate SQLite transaction with compare-and-swap. The
strict bounded record binds its schema, workspace ID, canonical root, monotonic
revision within its lifetime, random incarnation and both booleans to its hash ID.
Concurrent writers sharing one expected ID have one winner. Clear/recreate gets a
new incarnation even for the same booleans; an old ID cannot regain validity.
Foreign/copied roots, unknown fields, malformed/duplicate JSON, oversized records,
stale IDs and expired deadlines refuse. This is host-account control state, not
protection against another process with equal database/filesystem privileges.

Only explicit trusted capture resolves these saved choices. Metadata-only
`repository-scope` and ordinary `index` retain their preceding behavior. Explicit
`--follow-submodules`/`--no-follow-submodules` and
`--follow-nested-repositories`/`--no-follow-nested-repositories` override one
invocation without rewriting the record; opposite flags conflict. Without a
record, both choices default false. With one, the capture summary includes its ID,
revision and effective choices. No preference ID is added to immutable source
identity: the effective physical file/index selection already binds that proof.

An invocation owns its observed preference binding. Capture checks the exact
current record before source work and around the final Git/physical recheck,
before publishing source proof. A changed, cleared or newly appearing record
refuses. No transaction spans Git or source acquisition. Cooperative checks and
double scans are not OS-atomic isolation against hostile concurrent mutation.

Preferences contain no executable, Git trust, index/source/write permission,
root selector or budget. Every capture still needs `--allow-index-read` and
`--trust-git`, and retains its own finite limits. Crossed repositories remain
read-only. Snapshot query, copied collection, source-only saved-scope staleness
checks, approval, verification and journal recovery retain their existing gates.
Default/client automatic adoption, new repository formats, whole Windows and full
upstream equivalence remain separate acceptance work. See the
[repository-scope contract](git-repository-scope.md).
