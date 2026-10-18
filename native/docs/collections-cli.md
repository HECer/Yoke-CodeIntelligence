# Windows collection CLI

The `collections` command uses an explicit absolute registry directory. Native
and checked-view archives can be inspected after the original project is deleted.
Read operations do not open the default `--root`, capture source, activate a
project, or write registry files. The current enforced read-only registry guard
is Windows-only; Linux and macOS runtime certification is deferred.

```powershell
yoke --root C:/work/billing collections --registry C:/collections/team --allow-registration register billing --expect-absent
yoke collections --registry C:/collections/team list
yoke collections --registry C:/collections/team describe billing
yoke collections --registry C:/collections/team query --member billing --args '{"operation":"find","query":"invoice","kind":"function"}'
yoke collections --registry C:/collections/team read-source billing src/invoice.rs --start-byte 0 --end-byte 120
yoke collections --registry C:/collections/team join --member billing --member payments
```

The host creates the registry directory explicitly. Registration copies verified
source bytes into the registry's own archive; removing a registration retains
those blobs. A missing registry is never created by an inspection or removal.
`--allow-registration` is a separate host permission. New bindings require
`--expect-absent`; replacement and removal require the exact `--expected` ID.
The check and publication share one SQLite transaction. A stale operation cannot
replace the binding or publish new source blobs.

For native registration, the global source capture options and optional
`--snapshot` select the source graph. `register NAME --view VIEW_ID` instead loads
the exact checked graph view and its capture proof. It refuses conflicting
snapshot or capture selectors. Collections do not launch language servers.

## Foreign archive authority

A foreign archive additionally needs a host-selected profile on every invocation:

```text
collections --registry ABSOLUTE_DIRECTORY --federation-profile HOST_JSON --trust-grant describe MEMBER
```

The strict profile has `receiverRoot`, optional `captureOptions`, and `imports`.
Each import is `{ "id": "host-grant-name", "grant": IMPORT_GRANT }`, using the
existing strict import-grant schema. The receiver root must identify an existing
native store. Import grants contain public verification keys and receiver policy;
they are not credentials. Request/query JSON cannot supply profiles, roots,
grants, public keys, or permission overrides.

The profile is read afresh for each CLI invocation and held as that invocation's
host context. Every foreign metadata, query and source-read operation rechecks
the archive receipt against that context, the receiver workspace and current
receiver capture policy. Removing a grant from the next invocation's profile
denies access even though archived bytes remain. This CLI context is not a
persisted Session grant or a claim of mid-invocation service policy rotation.

`register-federated NAME --grant-id HOST_GRANT_NAME --remote-id REMOTE_ID`
requires the federation profile, registration permission and the same exact CAS
guard. It registers verified canonical graph facts. `--source-only` registers
only signed source bytes; it never upgrades source-only provenance to canonical
facts. Metadata reports the distinct `federatedCanonical` and `federatedSources`
kinds. `read-source` supports exact captured paths and bounded UTF-8 byte ranges
for both kinds; foreign reads always require current host authority.

## Bounds and output

Queries accept only the registry's bounded operation schema and selected member
names. Source ranges admit at most 256,000 bytes. Unknown captured membership is
an error, rather than permission to probe live files. Original parallel and
directed observations are preserved by joined graph queries.

Output is counted against `--max-bytes` before allocating its serialized buffer,
including the trailing newline. The exact JSON bytes are written once after full
admission. An oversized read fails without a partial JSON body or an implicit CAS
write in the default source project. Narrow the query or select a larger explicit
output budget. Registry input/source limits and the absolute operation deadline
remain independent bounds.

This is an unreleased Windows development capability, not a declaration of full
Graft, Serena or Graphify parity. Session/principal integration and analyzer
migration have their own acceptance gates.
