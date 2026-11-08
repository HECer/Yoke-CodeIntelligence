# Complete native default baseline at bcf442b

All 316 registered groups were attempted: 2951 passed,
10 failed and 53 remained explicitly ignored. Examples compile.
All 941 frozen inputs, tool binaries and the existing native CLI
remain unchanged throughout the run. Cargo used the ordinary locked test profile,
one target at a time and one test thread; no ignored case became a default pass.
The digest-history group compiles and all thirteen tests pass in the ordinary
profile without an alternate output flag. Earlier held processes/files remain
untouched; the parser feature changes the dependency-based output hash.

Every runtime failure is one of the original Windows symlink fixture cases and
shows OS1314 before the intended security assertion. There are no compile failures
or additional unexplained runtime failures. The [checkpoint](checkpoint.json)
lists each case separately. This is a complete attempted default suite, not a
passing Windows acceptance or a completed R1-R9 claim.

The user requested progress without systemwide Developer Mode changes. A direct
owned probe creates a directory Junction and file hardlink successfully, while
CreateSymbolicLinkW fails with 1314 for both file/directory kinds, with and without
the unprivileged-create flag. The current token lacks the symlink privilege.
Existing links can be used independently of permission to create new ones.
Both actual Junction security cases pass in this same full suite: decision
import/export and provider-start refusal for directory moves. Junctions and
hardlinks do not substitute for file-symlink test coverage. No setting or privilege
was changed; new probe links were removed and target bytes stayed intact.

[raw-logs.zip](raw-logs.zip) contains the original compile JSONL, stderr, test
logs, per-group results, runner, source/tool manifests and assessment. Its members
are indexed by path, byte length and SHA-256 in [raw-index.json](raw-index.json).
Every decompressed member was verified against its original bytes, including
trailing whitespace. ZIP compression avoids copying repeated compiler metadata
uncompressed into the checkout. ZIP verification only reads member bytes.

This baseline qualifies the shared float-roundtrip feature at bcf442b alongside
the earlier scoped retention/JSON tests. New community-policy work, authenticated
Google export without an approved test document, other runtime/upstream/hosted/
platform gates, main integration and release remain separately open.
