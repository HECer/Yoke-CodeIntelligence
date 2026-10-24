# Explicit global document history archive

Implementation is undergoing qualification. No completed acceptance or full
STATUS closure is claimed by this contract alone.

The host can archive bounded whole closed journals of a global document whose
old write selection was retired by an explicitly completed binding migration.
Both complete selections, both independently writable host authorities, the
exact completed migration/current destination owner generation and a separately
selected private archive root remain required. Project and unfinished journals
are ineligible. Existing read grants remain independent. Nothing reloads a
profile or edits configuration/source bytes.

## Explicit host workflow

Selection flags precede the subcommand, as in the binding-migration workflow:

~~~text
yoke host-documents --from-selection old.json --to-selection new.json history-plan --migration committed-migration.json --archive-root C:\HostState\archive
yoke host-documents --from-selection old.json --to-selection new.json history-apply --plan reviewed-history.json --archive-root C:\HostState\archive --allow-host-writes
yoke host-documents --from-selection old.json --to-selection new.json history-inspect --plan reviewed-history.json --archive-root C:\HostState\archive
yoke host-documents --from-selection old.json --to-selection new.json history-recover --plan reviewed-history.json --archive-root C:\HostState\archive --expected-event CURRENT_EVENT_SHA --direction resume --allow-host-writes
~~~

Save exact UTF-8 plan stdout as reviewed-history.json and review its record
names, physical identities/security, lengths/hashes, source/owner witnesses and
unselected-journal count before applying. Archive-root paths come from the host,
never from serialized authority or dashboard requests. They must already exist,
be absolute and physically disjoint from every analyzed root and active journal
namespace, and cannot contain a selected host document. Directory/file bindings
and both selections are rechecked under deterministically ordered journal locks.

The independent host option --history-record-budget precedes the subcommand;
its default is512, with32–512 records allowed. Supply the same value for plan,
apply, inspect, read and recovery. Packages contain complete journals only, in
deterministic name order. Larger selections leave explicit
unselectedClosedJournalsAtReview and
allClosedJournalsSelectedAtReview:false. A terminal operation completes its
reviewed package; it does not certify all current or future history. Review and
apply a fresh package for the remainder. Empty history refuses to create an
empty complete archive. Unknown/unfinished selected history and occupied original
backup/output roles refuse even if they would be outside the current package.

## Durable ordering and recovery

An immutable bounded frame retains every original canonical header/event and
its original identity/security/length/hash before any original record is removed.
The native decoder validates complete canonical plans and event chains, including
phase, lineage, direction, gaps, names and exact lengths, without filesystem
fallback. A durable operation header and fixed per-target pending marker precede
copy publication. The immutable frame is flushed, published without replacement,
reopened and checked before a durable removal-intent event.

While the marker exists, ordinary writes, ordinary recovery and new binding
migrations refuse. Dashboard inspection reports blockedByHistoryArchive:true
and complete:false, retaining a copyable read-only draft. It cannot archive
journals, supply roots, grant permission or recover host maintenance.

Before removal intent, explicitly reviewed rollback cancels while retaining
all originals and any already published immutable frame. After removal intent,
only resume is supported. Every present original is opened by its recorded
physical identity before content admission, then checked for exact length,
hash and security and removed through that held handle. Headers are removed last.
The admitted immutable archive stays held against writing/deletion throughout
removal. Missing originals are accepted only after durable removal intent;
their absence does not prove who deleted them. Unknown replacement occupants
remain untouched. Originals are never reconstructed from archived/guessed bytes.

Completion retains the operation header, hash-chained events, immutable frame
and per-operation completion marker. A lost response requires inspection and an
exact current event review, not ordinary apply retry. Foreign output/marker/
completion paths, changed source/owner/generation/security, unavailable roots
or damaged evidence preserve uncertainty. No cross-directory rename or relaxed
document sharing is used. The archive and selections must remain private and
available; no physical archive purge or standalone owner deletion is implemented.

## Reading retained evidence

Read one exact manifest-selected record from the admitted immutable frame:

~~~text
yoke --max-bytes 1000000 host-documents --from-selection old.json --to-selection new.json history-read --plan reviewed-history.json --archive-root C:\HostState\archive --record EXACT_MANIFEST_RECORD_NAME
~~~

The result includes exact Base64 bytes, original identity/security hash, SHA-256
and byte length. These are original record evidence, not current file identity,
new authority or observed execution. Unknown names and absent/corrupt frames
refuse; live files are never substituted. A larger explicit output budget can be
needed for a full header record. Inspection/read operations do not remove files.

Frames have a16,000,000-byte bound; manifests remain below512,000 bytes with event
headroom. Independent archive-root limits are256 entries and64,000,000 bytes,
with operation reservation before mutation. The original active-journal
2,048-entry/16,000,000-byte limits remain unchanged. These finite bounds can
refuse additional work; there is no automatic cleanup, secret access, model call,
analyzed project execution, privilege or system-setting change.
