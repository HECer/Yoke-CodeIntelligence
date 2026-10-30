# PR7 f41 native and four-profile browser evidence

This checkpoint records actual hosted results for candidate
`f41e56136b1704ff3c3742445f2b0138c78fdc6c`, tested merge
`6af15e69e1f4b28e1cc9296c23ae82f9105460f9`, and their identical tree
`65dde1f434636caee2209309576375fd0c365ef7`.
[Run 41030808968](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41030808968)
is completed with an overall **failure**; the Windows native job
[124024122346](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41030808968/job/124024122346)
is successful. This documentation commit does not transfer those results to later
runtime source changes or establish whole-project/release acceptance.

| Default Cargo target inventory | Zero-exit groups | Failed groups | Referenced logs rehashed |
| --- | ---: | ---: | ---: |
| Windows | 371 / 371 | 0 | 742 |
| Linux | 371 / 371 | 0 | 742 |
| macOS | 360 / 371 | 11 | 742 |

All eight shards per platform, 24 ZIP digests, 2,226 raw log hashes, exact target
inventories/partitions/weights and selected source proofs were checked. The
recorded inventories cover default Cargo groups, not additive individual test
totals, nondefault features, or full backend certification. Original ignored
tests and build-only example scopes remain. macOS includes explicit failure to
install the unchanged 268,435,456-byte address-space bound (EINVAL22); generic
startup/protocol failures still do not establish their individual causes.
[Native target receipts](native-targets.json) retain shard/artifact/log pins.

The Windows browser harness completes **native, Serena, persistent Python and
HTTP**, with six PNG/DOM pairs each: desktop/mobile before, pending and after.
All 24 original PNGs were individually inspected and re-inspected at their
original resolution for this publication. Drafts and Unicode content remain
readable, Review/Save visibly disable during pending history, Inspect recovery
has visible keyboard focus, and fresh saves after explicit host resume show no
pending journal. No blocking visual or unexpected credential/private-content
finding was observed in these owned-fixture screenshots.

The original three-variant history harness passes separately. Across its three
variants, 126 original archive records were independently read back. Source and
original-content hashes, BOM/CRLF targets and separately LF-normalized DOM text
were checked. Browser/driver, native authorities and worker exit before forced
cleanup. HTTP additionally retains authentication/origin/principal refusal,
owning-Session revocation with its peer preserved, natural 120-second service
exits, EOF and removal of the two fresh CI-only logon credentials. Browser
acceptance covers these owned flows; it does not certify other browser transports.

All five source maps agree on 1,101 selected inputs / 18,088,958 Git blob bytes.
Windows has 45 raw matches and 1,056 explicitly classified LF/CRLF-only matches.
Those are selected inputs, not every build input or a byte-identical checkout.
All 570 browser artifact members were rehashed. ZIP digests and non-expired
artifact metadata were checked again against GitHub before publication.
[Browser receipt](browser-evidence.json) and [provenance](provenance.json)
bind the exact sources, images, Jobs and artifacts.

The uploaded visual/human/visible-shell/release flags remain **false** and the
uploaded artifacts remain unchanged. The agent's individual visual review is
recorded separately. Hosted native-image hashes are retained as hosted claims;
no native-image payload was read, rehashed or executed locally for this audit.

## Original screenshots

These are byte-exact PNGs from the hosted artifacts. Open any image at its
original resolution. The SHA256, byte length and dimensions are in the browser
receipt and [file manifest](files.sha256.json).

### native

| Phase | Desktop | Mobile |
| --- | --- | --- |
| before | [PNG](images/native/desktop-before.png) | [PNG](images/native/mobile-before.png) |
| pending | [PNG](images/native/desktop-pending.png) | [PNG](images/native/mobile-pending.png) |
| after | [PNG](images/native/desktop-after.png) | [PNG](images/native/mobile-after.png) |

### serena

| Phase | Desktop | Mobile |
| --- | --- | --- |
| before | [PNG](images/serena/desktop-before.png) | [PNG](images/serena/mobile-before.png) |
| pending | [PNG](images/serena/desktop-pending.png) | [PNG](images/serena/mobile-pending.png) |
| after | [PNG](images/serena/desktop-after.png) | [PNG](images/serena/mobile-after.png) |

### persistent-python

| Phase | Desktop | Mobile |
| --- | --- | --- |
| before | [PNG](images/persistent-python/desktop-before.png) | [PNG](images/persistent-python/mobile-before.png) |
| pending | [PNG](images/persistent-python/desktop-pending.png) | [PNG](images/persistent-python/mobile-pending.png) |
| after | [PNG](images/persistent-python/desktop-after.png) | [PNG](images/persistent-python/mobile-after.png) |

### http

| Phase | Desktop | Mobile |
| --- | --- | --- |
| before | [PNG](images/http/desktop-before.png) | [PNG](images/http/mobile-before.png) |
| pending | [PNG](images/http/desktop-pending.png) | [PNG](images/http/mobile-pending.png) |
| after | [PNG](images/http/desktop-after.png) | [PNG](images/http/mobile-after.png) |

## Remaining gates

The earlier `abdbdb03` persistent-Python focus failure remains historical evidence;
the later successful four-profile result is qualified only at `f41e5613` above.
Strict macOS memory enforcement, unrelated generic startup failures, subsequent
combined source/stack acceptance, all 77 backend runtime profiles, broader
original 353/R1-R9 parity, matching VSIX and real extension host, authorized
Google live qualification, main integration and the separate public publisher/
release remain open. No assertion, timeout, ignore, permission, memory cap or
recovery rule was relaxed. This documentation-only publication runs no new
native tests; its checks verify metadata, source/image pins, retained historical
text, allowed file scope and remote publication.
