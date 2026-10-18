# Host-configured Session collections

`HostProfile.collections` is omitted when empty, preserving prior serialized
profiles and the original empty-policy session fingerprint. Nonempty collection
policies are sorted, normalized, and included in a distinct fingerprint
extension. Resuming a session under changed collection policy fails.

Each `HostCollection` has `name`, an absolute existing `registryRoot`, bounded
unique `allowedMembers`, `allowRegistration` (default false), and optional
`receiverProject`. The receiver must be `default` or another host-registered
project name. Registry ancestors must be directories without links. Session
initialization validates paths without opening or creating a registry database.

## Native invocation contract

`Session::invoke` accepts these additional operations:

| Operation | Arguments |
| --- | --- |
| `collections.list` | `collection` |
| `collections.describe` | `collection`, `member` |
| `collections.join` | `collection`, `members` |
| `collections.query` | `collection`, `members`, `arguments` |
| `collections.read` | `collection`, `member`, captured relative `path`, explicit `startByte`, `endByte` |
| `collections.register` | `collection`, `member`, `kind`, registered `project`, mandatory `expectedRegistration`; `viewId` for `view`, or `grantId`/`remoteId` for foreign kinds |
| `collections.remove` | `collection`, `member`, exact `expectedRegistration` |

Unknown fields, collections, members, project roots used in place of names, and
duplicate member selections are refused. A source `path` selects only an exact
member of the archived manifest; it never authorizes a filesystem probe.
`arguments` uses the existing immutable registry query contract.

Read operations use `Registry::open_existing`. Native and checked-view archives
remain readable after the original project/store is deleted, without capturing
or activating a Session project and without creating registry state. Listing
shows only currently registered members in the host allowlist; unused allowed
names are omitted. Metadata remains bounded and does not expose signatures or
source excerpts. The existing Windows read-sharing/checkpoint requirements
apply to registry inspection.

## Registration changes

The independent `allowRegistration` permission is required for registration and
removal. It grants no source edits, provider execution, or project activation.
Native registration passively indexes the explicitly selected registered
project. `view` requires an existing checked view bound to that project's exact
current snapshot and graph. Query-only/read-only projects may supply immutable
source data.

`expectedRegistration: null` explicitly requires absence; a SHA-256 ID requires
that exact current binding. The adapter checks stale expectations and existing
foreign authority before opening a source project, and the final registry CAS
transaction independently enforces the binding. There is no implicit upsert.
Removal requires an exact existing ID. Returned receipts carry member, kind
where applicable, and `executionAuthority: "none"`.

## Invocation-scoped foreign authority

`CollectionInvocationAccess::new(collection, receiverProject, &receiverEngine,
&currentPrincipalGrants)` is a trusted Rust API, deliberately not deserializable.
Pass it to `Session::invoke_collection_with_access` for one operation. The host
must retain its current policy read lock through the complete call and supply
only grants currently authorized for that principal. The adapter binds the
capability to the configured collection, receiver project/root, and the
workspace of an already opened receiver project; removed receivers fail.

Foreign archive access revalidates the current grant, receiver identity,
signature, scope, receiver path policy, and original bytes on every operation.
Ordinary invocation cannot read foreign entries, even when a receiver is
configured. Grants, keys, roots, and execution permissions never enter Session
state. `grantId` and `remoteId` are selectors into the current host context and
receiver store, not authority supplied by request JSON.

Foreign registration requires the same host registration permission, configured
receiver project, transient capability, and explicit CAS. `federatedCanonical`
requires the bridge's checked current canonical graph. `federatedSources`
retains source-only authority with no invented graph nodes. Both can expose
exact archived source bytes through the bounded `collections.read` operation;
source-only joins and queries remain honest about the absence of graph facts.
Revoking a grant or changing receiver policy prevents later reads even when the
registry retains complete archived source bytes.

The HTTP host injects `Target.collection_grants` from its current principal
policy, grouped by registered collection name. Its service CLI accepts the same
`collectionGrants` map only in the explicitly trusted host service file. These
are public-key import grants, never service bearer credentials. Client JSON
cannot supply grants, roots, or receiver identities.

The actor keeps its policy read lock through dispatch and rechecks the current
principal generation before constructing invocation access. A policy upsert or
revocation invalidates existing transport sessions. Neither connection state,
Session records, nor queued jobs retain accepted grants. Stdio and ordinary
protocol dispatch retain no foreign access by default.

`Session::invoke_collection_with_grants` resolves the receiver exclusively from
its validated host profile, requires its existing store, and applies the current
capture policy without capturing sources or starting providers. Every foreign
archive operation still validates its signature, scope, source bytes and current
grant. This receiving-store handle is distinct from source execution authority.
