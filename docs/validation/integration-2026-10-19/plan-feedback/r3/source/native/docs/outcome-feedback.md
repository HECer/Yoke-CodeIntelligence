# Source-bound outcome feedback

`feedback` records explicitly authored outcomes for existing project knowledge,
replays their original source proofs, and ranks a selected set with a visible
integer policy. These project records are separate from coding-agent AgentMemory.
Actor, session, timestamp, success and corroboration remain host assertions.

All operations require an existing native Store. `record` and
`import-transcript` additionally require global `--allow-writes`:

```text
yoke --root REPOSITORY --allow-writes feedback record --input EVENT.json
yoke --root REPOSITORY feedback rank --input RANK.json
yoke --root REPOSITORY --allow-writes feedback import-transcript --input SESSION.txt --label selected-session --author Ada
yoke --root REPOSITORY feedback mine --transcript TRANSCRIPT_ID
```

Each source selection uses five exact fields: `workspaceId`, `snapshotId`,
`viewId`, `viewArtifactId`, `baseGraphHash`. Obtain them from a published native
graph View. JSON pins are selectors; complete frozen native source, overlay and
Meaning receipt replay supplies the checked capability. Serialized labels or
an event's own content hash cannot substitute for that replay.

## Immutable event input

```json
{
  "schemaVersion": 1,
  "origin": {
    "workspaceId": "WORKSPACE_SHA256",
    "snapshotId": "ORIGIN_SNAPSHOT_SHA256",
    "viewId": "ORIGIN_VIEW_SHA256",
    "viewArtifactId": "ORIGIN_ARTIFACT_SHA256",
    "baseGraphHash": "ORIGIN_GRAPH_SHA256"
  },
  "evaluated": {
    "workspaceId": "WORKSPACE_SHA256",
    "snapshotId": "EVALUATED_SNAPSHOT_SHA256",
    "viewId": "EVALUATED_VIEW_SHA256",
    "viewArtifactId": "EVALUATED_ARTIFACT_SHA256",
    "baseGraphHash": "EVALUATED_GRAPH_SHA256"
  },
  "knowledgeId": "KNOWLEDGE_SHA256",
  "actor": "Ada",
  "session": "selected-session",
  "sequence": 1,
  "occurredAtMs": 1000,
  "outcome": "success",
  "rationale": "Explicit outcome assertion with selected source evidence.",
  "sourceEvidence": [{
    "path": "billing.rs",
    "contentHash": "SOURCE_SHA256",
    "startByte": 0,
    "endByte": 4,
    "quote": "text"
  }],
  "transcriptEvidence": []
}
```

Replace every placeholder with its actual lowercase SHA-256. The original
knowledge record must belong to the origin snapshot. Source quotes must match
exact original UTF-8 bytes in evaluated primary snapshot files and overlap a
knowledge source path. Historical receipt-only snapshots remain replay proofs;
they do not become evaluated quote inputs. At least one source or transcript
quote is required. Unknown or duplicate JSON fields are refused.

Outcomes are `success`, `failure`, `inconclusive`, `corroboration`, or
`contradiction`. A sequence is 1..1,000,000, unique per exact origin selection,
knowledge, actor and session. Replaying an identical event is idempotent.
Changing an occupied sequence refuses publication. Timestamps must be
nondecreasing by sequence, including when events arrive out of order.

## Ranking policy

Rank input contains `schemaVersion: 1`, the original `origin` pin, a `current`
pin, 1..128 distinct `knowledgeIds`, `asOfMs`, `decayWindowDays` (1..3650), and
`limit` (1..128). Every selected event's complete original proof is replayed.
Events for other origin pins or unselected knowledge are counted separately.

Future events do not vote. For each actor/session label pair, the highest
eligible sequence contributes once. Older events remain in the response with
their exclusion reason. For `windowMs = decayWindowDays * 86400000`:

```text
weight = floor(1000 * windowMs / (windowMs + ageMs))
success, corroboration: +weight
failure, contradiction: -weight
inconclusive: 0
```

The policy uses integer hyperbolic decay with overflow-safe intermediates.
Each event exposes its age, weight, contribution and disposition. Changed,
excluded or deleted knowledge anchors, and observed authored supersession,
make an item ineligible with effective score zero. Its raw score stays visible.
The response identifies the authored successor. Its original snapshot and
anchor hashes must be present in the checked origin/current source closure;
an unchecked or corrupt successor refuses ranking. A source closure outside
those selections requires a suitable explicitly selected current View.
Results sort by eligibility, descending effective score, then knowledge ID.
Neither labels nor scores establish independent corroboration, correctness,
authenticated identity or observed execution.

## Explicit transcript candidates

Import selects one plain UTF-8 file of at most 1,000,000 bytes. It preserves
original CRLF and Unicode bytes in immutable CAS storage. Label and author are
unverified assertions. Protected or linked host paths, obvious secrets and
unsupported controls are refused. No session directories are discovered.

Mining searches these case-insensitive literals:

| Candidate | Literals |
| --- | --- |
| success | `tests passed`, `test passed`, `it worked`, `funktioniert` |
| failure | `tests failed`, `test failed`, `did not work`, `funktioniert nicht` |
| corroboration | `confirmed`, `bestätigt` |
| contradiction | `contradicted`, `widerlegt` |

Each candidate carries its exact original UTF-8 byte range and quote.
Overlapping matches and negated phrases can yield multiple candidates: this
literal policy does not establish semantic understanding. Mining creates zero
events. A host must select the evidence and author an explicit event.
`transcriptEvidence` entries use `transcriptId`, `contentHash`, `startByte`,
`endByte`, `quote` from the selected candidate's `source`; omit its descriptive
`kind` field. Transcript coordinates are never native-source coordinates.

## Bounds and persistence

Default limits are 10,000 events, 32 MB stored feedback JSON, 1,000 transcripts,
32 replay pairs, 64 MB replay input, 64 MB source reservations, 10,000 mined
occurrences, and a 4 MB whole response. Each event allows 32 source and eight
transcript quotes, 32 KB aggregate quote bytes, and a 64 KB draft. CLI limits
`--max-events`, `--max-input-bytes`, `--max-source-bytes`, `--max-pairs`,
`--max-candidates` and global `--max-bytes` can reduce the corresponding bounds.
Inherited native replay materialization limits remain independent. A capacity
failure refuses the whole operation.

Write operations validate permission, host input, source proofs, sequence,
quota and bounded output before SQL publication. They use an immediate SQLite
transaction and refuse a caller-owned transaction. Read operations preserve
caller read transactions and SQLite limits. A late transcript I/O or SQL
failure can leave an unreachable immutable CAS object; filesystem and SQL
publication are not one atomic transaction. Retention validates feedback and
transcript schemas and original proof closures before recognizing their roots.

There is no provider call, implicit capture, source edit, automatic promotion,
or overflow artifact. Original View limitations, overlays, Meaning selections,
packet coverage, provenance and source-scope roles remain visible. The current
scope covers native knowledge outcomes and literal transcript candidates;
general semantic learning and outcomes for other object types remain open.
