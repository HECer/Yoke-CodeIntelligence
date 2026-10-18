# Portable Meaning archive CLI

`meaning-archive` transports a complete typed Meaning View, its source overlays,
original and rebound proof closure, and exact frozen source bytes. Claims remain
inferred and provider attestation remains unverified. None of these commands
starts a model, language server, source capture or project code.

Export an exact already-published Meaning View from an existing source store:

```text
yoke --root C:\work\project meaning-archive export <view-id> --output C:\archives\project.json
```

The destination must be a new absolute local file with an existing nonlinked
parent. The export does not build a View, choose the latest snapshot or publish
records/artifacts into the source store. Its source selection admits all archived
scopes before reading View/source CAS. Opening the existing source store may use
its normal locks and SQLite setup; it does not initialize a missing workspace.
Before that constructor, export requires the existing blobs/trees directories,
maintenance/client lock files, database schema/workspace and an already selected
WAL mode. Missing state and databases requiring journal initialization or rollback
recovery refuse. Namespace handles remain held through export. Normal shared
locking and WAL reader bookkeeping can still occur on a complete store.

Replay a supplied archive independently of any source store:

```text
yoke meaning-archive inspect --input C:\archives\project.json
```

Inspection verifies the complete source/proof/View closure using only supplied
bytes. It prints metadata, including immutable IDs, canonical byte length,
typed claim counts, and counts of originally unknown cost/usage reports. It
does not print source quotations, labels, proof rows or provider commands.
JSON whitespace and object-key order can normalize; original arrays and embedded
raw payload strings retain their exact values. `archiveId` identifies canonical
archive bytes, while the original View, claim and receipt IDs stay intact.

Explicitly register those checked bytes in a host-selected existing directory:

```text
yoke meaning-archive register project --registry C:\collections --input C:\archives\project.json --allow-registration --expect-absent
yoke meaning-archive register project --registry C:\collections --input C:\archives\new-project.json --allow-registration --expected <registration-id>
```

Registration requires its separate authority flag and exactly one CAS form.
Create-only refuses an existing name; replacement requires the current exact
registration ID. The registration ID includes the host name and is distinct
from archive/View IDs. The library commits the registration and complete source
union together. Stale CAS leaves no additional binding or source blobs.
Replacement requires an existing registry; input and output admission complete
before writable registry opening. Normal writable SQLite opening can affect its
sidecars even when a later registration is refused.

Load a registered archive after the original source, store and worker are gone:

```text
yoke meaning-archive load project --registry C:\collections
yoke meaning-archive load project --registry C:\collections --output C:\archives\restored.json
```

Loading uses the existing read-only registry path and complete pure replay.
Without `--output` it emits metadata only. With `--output` it writes exact
canonical archive bytes to a fresh file. Missing registry/source objects refuse;
there is no live Store, provider, filesystem-source or other-registry fallback.

`inspect`, `register` and `load` do not select `--root`; a nondefault root
refuses. All four reject snapshot, analyzer, capture, language-server and source
write flags. Supplying an output path authorizes only that new host file;
registration authority authorizes only the named registry operation. No MCP
permission or execution authority follows from importing an archive. Existing
source-only exports and collection joins keep their typed refusal behavior.

The shared timeout is 10–600,000 ms. JSON stdout uses `--max-bytes`
(1,024–4,000,000 bytes including newline). Raw incoming and normalized bytes
must both fit `--max-wire-bytes` (1–32,000,000, default 32,000,000); distinct
decoded source bytes must fit `--max-source-bytes` (1–64,000,000, default
64,000,000). These are additional caps alongside all existing proof, file,
count and shared Meaning-closure bounds. A source set below its cap can still
exceed the JSON transport cap because escaping/proof bytes count too.

The complete metadata response is admitted before any durable write. Archive
bytes never fall back to stdout or source CAS. File publication writes and syncs
an owned temporary sibling, then publishes without replacing an existing file,
directory, hardlink or dangling link. Parent directories are not created.
After successful publication/registration, an output-delivery error names the
durable archive/path or registration ID so callers can recover the result.
No partial archive is reported as complete. Hard-power-loss durability and
universal ACL-race guarantees are outside this bounded host-file contract.
