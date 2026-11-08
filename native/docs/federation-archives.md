# Foreign source archives and current host authorization

`federation_archive::FederationArchiveAccess` is a Rust host context borrowing a
receiving `Engine` and its currently allowed import grants. It cannot be decoded
from JSON. An HTTP host must filter the map to the current principal's readable
grants and retain its policy read lock throughout an operation.

`capture_sources(grant_id, remote_id, deadline)` produces checked snapshot,
signed receipt and source bytes without graph authority. Its binding contains only schema version
1, grant ID and original receipt. It contains no `ImportGrant`, receiver ignore
configuration, execution permission or permanent source authorization.

Every `authorize_sources` or `authorize` call resolves that grant again and checks its scope,
public key, sender workspace, receiving workspace and current local receiving
policy for **all** paths before the first archived source read. Removing a grant,
rotating its key or adding local ignores denies the next operation. Closing the
sender or removing the original receipt rows does not require contacting either
store again, but the receiving host must retain or explicitly reestablish the
same workspace identity. A different root is not a substitute receiver.

Source paths remain in their federated namespace. Signed manifest membership,
hashes, sizes, modes, source UTF-8, secret screening and protected/link policy
remain required. A local v1 receipt's graph hash is unsigned and cannot establish
graph origin. Canonical `capture` and `authorize` additionally require current
language classification and reconstruct every graph fact from the exact admitted
source bytes with the current analyzer. The entire graph, including coverage
annotations, must match. Entity/evidence membership, exact UTF-8 ranges, line
bounds and observation endpoints are also checked. These checks
preserve structural candidates; a signature does not establish compiler binding,
source completeness, safe execution or current remote freshness.

Version 1 import receipts did not record the receiver analyzer. The sender
analyzer must never be interpreted as the receiver epoch. Whole-manifest current
or released native 0.5/analyzer-4 language compatibility is accepted while the
original language labels and receipt bytes remain unchanged in source-only access.
Receiver epoch is
**not recorded**. An unknown preserved classifier is unavailable for archived
source authorization until a typed legacy classification proof exists. Original
historical graphs that cannot match current extraction remain archival data;
they are refused as canonical graph proof. A compatible language vector does not
prove an original receiver extraction epoch.

Receipt/graph SQLite reads admit type and byte length under a temporary cap that
respects and restores the prior limit. Source reads admit declared size before
allocation and require exact hash and length. Archive snapshot and graph hashing
stream with deadline and byte bounds before allocating serialization. Capture retains
at most the signed federation limit of 16 MB, 2,000 files and 4 MB per file.

This module is the checked bridge. Generic graph collection APIs still require
their separate authorized integration; ordinary native archive proof does not
grant federated source access. Unknown virtual source producers remain denied.
