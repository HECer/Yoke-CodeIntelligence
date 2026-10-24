from pathlib import Path
import json,hashlib,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';source=json.loads((base/'source-integration.json').read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest();tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old_raw=tracker.read_bytes();(base/'frozen-tracker.json').write_bytes(old_raw);old=json.loads(old_raw);value=json.loads(old_raw);r7=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R7');paths=[*source['paths'],'docs/native-falkor-publish.md','docs/validation/integration-2026-10-19/falkor-publish/index.json']
for path in paths:
 if path not in r7['evidence']:r7['evidence'].append(path)
r7['missingBehavior']='Bounded selected PostgreSQL catalogs, anonymous HTTP text/raw-HTML/PDF, shared-rate-coordinated arxiv.org metadata/PDF, reviewed Windows native saved-base-graph Neo4j and FalkorDB publication now have actual evidence. FalkorDB6.0.2 has pinned TCP/TLS/OS credentials, a fresh-graph WATCH/EXEC bootstrap, operational schema checks, immutable exact directed/parallel/evidence-preserving revisions, atomic expected-revision/count guards, approval journals and no-resend positive read recovery. This does not certify arbitrary upstream or mixed-source graph equivalence, production/RBAC, other server versions or non-Windows publication. Google shortcut export, authenticated/social/video routes and article/semantic interpretation remain open. Received records never establish source/server/paper/author/compiler/execution authority.'
r7['suggestedScope']='Continue Google shortcut, social/video/authenticated connectors, cross-source semantic behavior and wider database/platform qualification. Preserve explicit endpoint/credential/write/approval/recovery authority, exact immutable evidence, bounded protocols, honest missing capabilities and zero query-time acquisition.'
r7['falkorPublishCheckpoint']={'index':paths[-1],'newContractCases':{'wire':6,'offlinePlansAndPayloads':4,'transportSchemaJournal':9,'hostCli':6},'finalSelectedTotals':source['nativeTotals'],'nativeGroups':20,'actualServer':'FalkorDB6.0.2 graphModule60002','actualPlatform':'Windows','actualTlsAndUnicodePassword':True,'forcedConcurrentCas':True,'lostStdoutAndUnknownCommitReadRecovery':True,'readRecoveryAfterLaterActiveRevision':True,'actualCountAndSchemaConflictRefusals':True,'foreignGraphNoAdoptionAndNativeWatchConflict':True,'sameBinaryNeo4jCompatibilityRevalidated':True,'guardedPublicationPlatforms':['windows'],'sourceAuthority':False,'serverAttested':False,'fullUpstreamContractCertified':False,'wholeNativeRerun':False}
assert not any(c['id']=='native_falkor_reviewed_snapshot_publish' for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'native_falkor_reviewed_snapshot_publish','behavior':'Explicit reviewed Windows native saved-base-graph publication through bounded native RESP2 and fixed parameterized FalkorDB6.0.2 Cypher. Pinned TCP/TLS/static roots/hostname, create-only private-pipe OS credentials, independent host grants, fresh graph WATCH/EXISTS/MULTI-one-initialize/EXEC plus5 indexed properties/3 operational unique constraints, exact original row JSON/evidence/ranges/directions/parallel/unresolved preservation, immutable scoped revisions with atomic expected-revision/known-property/actual-count guards, durable guarded approval/intent journal and no-resend positive read recovery. Actual native plain/TLS/Unicode, response-loss-after-commit, forced concurrent updates, historical recovery, foreign-graph/real-WATCH/schema/count refusals and lost stdout pass on an owned external6.0.2 VM. Same final binary revalidates Neo4j.','evidence':paths,'remainingReview':'Live credentials and guarded journals remain Windows-only. Module60002 is the finite tested profile; other versions are refused. Hash chains are integrity metadata, not authentication or global journal coordination. Matching publisher builds are required for pending plans; client cancellation cannot guarantee server cancellation. Other platforms/server versions, production/RBAC, large-capacity and full arbitrary upstream/mixed-source equivalence remain unqualified. Google/authenticated/social/video/semantic connectors, widerR1-R9/main/release and earlier platform failures remain open.','fullContractCertified':False})
for cap in value['reconciledCapabilities']:
 if cap['id']=='native_postgres_received_catalog':cap['remainingReview']='Bounded PostgreSQL selected-catalog capture/frozen queries have actual Windows PostgreSQL18.6/psql18.6 evidence. Public HTTP, dedicated arxiv and reviewed native Neo4j/FalkorDB publication are separately checkpointed. Full pinned upstream schema/definition equivalence, other server/client versions, non-Windows and mixed semantic/source integration remain unverified. Google shortcut export and authenticated/social/video/semantic connectors remain open.'
 elif cap['id']=='native_neo4j_reviewed_snapshot_publish':cap['remainingReview']='Guarded publication/credentials remain Windows-only. Journal integrity is not authentication or cross-path global coordination; matching builds are needed for pending plans and client cancellation cannot guarantee server cancellation. The final FalkorDB connector binary also revalidates the actual owned Neo4j Community2026.09.0 profile. Enterprise RBAC, other Neo4j/JVM/server/platform profiles, full arbitrary upstream/mixed-source equivalence and unattended deployment remain unverified. Google/authenticated/social/video/semantic connectors, widerR1-R9/main/release and earlier platform failures remain open.'
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode());shutil.copyfile(Path(__file__).parent/'ci-falkor-guide.md',root/'docs/native-falkor-publish.md')
status=root/'docs/STATUS.md';text=status.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert text.count(marker)==1
paragraph='''[Reviewed native FalkorDB publication](validation/integration-2026-10-19/falkor-publish/index.json)
adds a separate explicit Windows host workflow for saved native base graphs.
Closed offline plans grant no authority. The native bounded RESP2/TCP/TLS client
uses pinned sockets, verified hostnames/static roots and private-pipe OS
credentials. Fresh-graph WATCH/EXEC initialization, five indexed properties and
three operational unique constraints precede reviewed publication. Immutable
revisions preserve all original row JSON, byte evidence, directions, parallel
observations and unresolved targets; atomic expected-revision/count guards,
durable pre-send intents and positive fixed-read recovery prevent silent
overwrite or automatic replay of unknown results. See the
[host workflow](native-falkor-publish.md).

The owned **FalkorDB6.0.2/module60002** VM passes actual native TLS, wrong-root
and hostname refusals, Unicode credentials, exact row roundtrips, two forced
concurrent updates with one commit, lost stdout/commit response, historical
recovery, count/schema conflicts, foreign graph preservation and actual native
WATCH/EXEC interference. All five fixture credentials and the owned VM are
removed. The same final binary revalidates the full owned Neo4j connector live
fixture; its five credentials and Java server are also removed.

There are **25 new cases**. The final **20 selected native groups** record
**580 passed / 1 unchanged media OS1314 failure / 8 existing ignored gates**.
All-target Clippy with warnings denied and format pass; **724 native inputs**
and each live binary remain unchanged during their recorded runs. Mandatory
integrated checks are retained in the indexed evidence. Earlier incremental
results are nonadditive and recorded failures remain visible. This is not a
whole-native or other-platform pass.

Only module60002 is admitted. Production/RBAC, other database/JVM/server
versions, non-Windows and full mixed-source/upstream equivalence remain
unqualified. Google/authenticated/social/video/article/semantic connectors,
widerR1-R9, unattended production, main and release remain open.

'''
status.write_text(text.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';text=architecture.read_text(encoding='utf-8');text+='\nReviewed FalkorDB publication is a separate Windows host connector with pure\nRESP2 framing, closed offline plans and fixed typed parameter encoding. Literal\nsockets/TLS/OS credentials and independent grants stay outside repository data.\nWATCH/EXEC admits one fresh graph root before separately journaled operational\nschema steps; immutable revisions use atomic expected-revision/property/count\nguards and retain exact directed parallel evidence. Durable pre-execution\nintents require no-resend positive fixed-read recovery after unknown results.\nSee the [host workflow](native-falkor-publish.md) and\n[indexed validation](validation/integration-2026-10-19/falkor-publish/index.json).\n';architecture.write_text(text,encoding='utf-8',newline='\r\n')
neo4j=root/'docs/native-neo4j-publish.md';text=neo4j.read_text(encoding='utf-8');text=text.replace('Bolt, FalkorDB and non-Windows publication remain separate work.','Bolt and non-Windows publication remain separate work. FalkorDB now has its own\n[separately qualified native workflow](native-falkor-publish.md).');neo4j.write_text(text,encoding='utf-8',newline='\r\n')
readme=root/'README.md';text=readme.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert text.count(needle)==1;text=text.replace(needle,'Explicit reviewed [Neo4j](docs/native-neo4j-publish.md) and\n[FalkorDB](docs/native-falkor-publish.md) publication preserve saved native graph\nrows and evidence through separate host approval, pinned TLS/OS credentials,\nimmutable revisions and read-only recovery of unknown writes. Offline plans\ngrant no authority; live publication currently supports Windows.\n\n'+needle);readme.write_text(text,encoding='utf-8',newline='\r\n')
proof={'historicalInventoryLedgerAndSummaryUnchanged':True,'oldTrackerSha256':sha(old_raw),'currentTrackerSha256':sha(tracker.read_bytes()),'mainOrReleaseVerified':False};(base/'documentation-proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps(proof))
