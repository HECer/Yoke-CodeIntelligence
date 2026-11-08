from pathlib import Path
import json,hashlib,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'docs/validation/integration-2026-10-19/neo4j-publish';index=json.loads((out/'index.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();old=json.loads(tracker.read_bytes());value=json.loads(tracker.read_bytes());t=index['nativeTotals'];paths=[*index['paths'],'docs/validation/integration-2026-10-19/neo4j-publish/index.json'];r7=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R7')
for path in paths:
 if path not in r7['evidence']:r7['evidence'].append(path)
r7['missingBehavior']='Bounded selected PostgreSQL catalogs, anonymous HTTP text/raw-HTML/PDF, dedicated shared-rate-coordinated arxiv.org metadata/PDF, and reviewed Windows native saved-base-graph Neo4j upsert now have actual evidence. Neo4j supports explicit schema/namespace bootstrap, pinned HTTPS/OS credentials, immutable parallel/directed/evidence-preserving revisions, atomic CAS/count guards, approval journals and read-only unknown-result recovery; this does not certify arbitrary upstream graph equivalence, Enterprise RBAC, other server versions or non-Windows publication. Google shortcut export, authenticated/social/video routes, article/semantic interpretation and FalkorDB upsert remain open, along with broader mixed-source and upstream qualification. Received records do not establish source/server/paper/author/compiler/execution authority.'
r7['suggestedScope']='Continue FalkorDB, Google shortcut, social/video/authenticated connectors, cross-source semantic behavior and wider Neo4j/platform qualification. Preserve explicit host endpoint/credential/write/approval/recovery authority, exact immutable evidence, bounded protocols, honest missing capabilities and zero query-time acquisition.'
r7['neo4jPublishCheckpoint']={'index':paths[-1],'newContractCases':index['newContractCases'],'finalSelectedTotals':t,'nativeGroups':17,'actualServer':'Neo4j Community2026.09.0','actualPlatform':'Windows','actualHttpsAndUnicodePassword':True,'forcedConcurrentCas':True,'lostStdoutAndUnknownCommitReadRecovery':True,'readRecoveryAfterLaterActiveRevision':True,'actualCountAndSchemaConflictRefusals':True,'guardedPublicationPlatforms':['windows'],'jvmVendorQualificationClaimed':False,'sourceAuthority':False,'serverAttested':False,'fullUpstreamContractCertified':False,'wholeNativeRerun':False}
assert not any(c['id']=='native_neo4j_reviewed_snapshot_publish' for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'native_neo4j_reviewed_snapshot_publish','behavior':'Explicit reviewed Windows native saved-base-graph publication through fixed parameterized QueryAPIv2/Cypher5. Separate schema/namespace bootstrap, pinned endpoint/TLS/OS credentials, lowerable whole-payload quotas, exact original rows/evidence/ranges/parallel directions/unresolved targets, immutable scoped revisions with atomic expected-revision/count guards, durable pre-send approval/intent journal, and no-replay positive read recovery. Imported plans/records grant no host authority. Actual native HTTPS/Unicode/password, forced concurrent updates, response loss after commit, historical recovery, count/schema conflicts and lost stdout are qualified on an owned Community2026.09.0/JBR25 fixture.','evidence':paths,'remainingReview':'Guarded publication and credential operations are Windows-only. Journal hash integrity is not authentication or global coordination across different host paths; matching publisher builds are required for existing plans. Client cancellation does not guarantee server cancellation. Enterprise RBAC, other Neo4j/JVM/server/platform profiles, full arbitrary upstream/mixed-source graph equivalence and unattended deployment remain unverified. FalkorDB/Google/authenticated/social/video/semantic connectors and widerR1-R9/main/release remain open. Earlier whole-native/platform failures remain.','fullContractCertified':False})
pg=next(c for c in value['reconciledCapabilities'] if c['id']=='native_postgres_received_catalog');pg['remainingReview']='Bounded PostgreSQL selected-catalog capture/frozen queries have actual Windows PostgreSQL18.6/psql18.6 evidence, revalidated with the held-member process guard. Public HTTP, dedicated arxiv and bounded reviewed native Neo4j publication are separately checkpointed. Full pinned upstream schema/definition equivalence, other server/client versions, non-Windows qualification and mixed semantic/source integration remain unverified. Google shortcut export, authenticated/social/video and FalkorDB upsert remain open; local readers/generated Cypher are not those connectors.'
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph=f'''[Reviewed native Neo4j publication](validation/integration-2026-10-19/neo4j-publish/index.json)
now adds an explicit Windows host workflow for saved native base graphs. Offline
plans grant no capabilities. Separate approval, network, credential, schema,
write and journal gates protect fixed parameterized Query API v2/Cypher 5.
Immutable scoped revisions retain original entity/evidence/observation JSON,
byte ranges, directions, parallel observations and unresolved targets. Atomic
expected-revision/count guards and durable pre-send intents prevent silent
overwrite or automatic replay of unknown writes. Recovery uses only fixed reads.
See the [host workflow](native-neo4j-publish.md).

The owned Neo4j Community **2026.09.0** fixture passes actual native HTTPS,
static-root/hostname refusals, Unicode private-pipe credentials, exact row
roundtrips, two forced concurrent updates with exactly one commit, lost stdout,
response loss after actual commit, historical recovery and count/schema conflict
refusals. All five fixture credentials and the Java server are removed. JBR25.0.3
is functional test evidence, not JVM-vendor qualification. The exact final
production binary is unchanged across a test-only cfg-import compiler fix.

There are **21 new cases**. Final 17 selected native groups record **{t['passed']}
passed / {t['failed']} failed / {t['ignored']} existing ignored gates**; the unchanged media
symlink setup failure remains OS1314. All-target Clippy, format, mandatory
typecheck, **260 TypeScript tests / 3 skips** and **17 Python tests** pass with
**{index['integratedInputs']} integrated inputs unchanged**. Native/live input proofs cover
{index['nativeInputs']} files. Earlier incremental runs are nonadditive and all recorded
failures remain in the evidence. This is not a whole-native pass.

Live publication/credentials remain Windows-only. Hash-chain metadata is not
authentication or global journal coordination; client cancellation cannot
guarantee server cancellation. Other Neo4j versions/JVM vendors, Enterprise RBAC,
non-Windows and full upstream/mixed-source equivalence remain unqualified.
FalkorDB, Google/authenticated/social/video/semantic connectors, widerR1-R9,
unattended production, main and release remain open.

'''
status=root/'docs/STATUS.md';text=status.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert text.count(marker)==1;status.write_text(text.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';text=architecture.read_text(encoding='utf-8');text+='\nReviewed native Neo4j publication is an explicit Windows host connector over\nfixed parameterized QueryAPIv2/Cypher5. Closed offline plans and immutable saved\nbase-graph rows grant no host or source authority. Separate schema bootstrap,\nnetwork/OS-credential/write/approval/journal gates, scoped immutable revisions,\natomic expected-revision/count guards and durable pre-send intents preserve\nparallel observations, unresolved targets and exact evidence. Unknown writes\nrequire positive fixed-read recovery; they are never automatically replayed.\nSee the [host workflow](native-neo4j-publish.md) and\n[indexed qualification](validation/integration-2026-10-19/neo4j-publish/index.json).\n';architecture.write_text(text,encoding='utf-8',newline='\r\n')
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();p=index['priorEvidenceReadme'];assert len(prefix)==p['bytes'] and hashlib.sha256(prefix).hexdigest()==p['sha256'];append=f'''\n## Reviewed native Neo4j publication checkpoint\n\nSource `{index['sourceCommit']}`, runtime merge `{index['runtimeCommit']}`.\n[Indexed evidence](neo4j-publish/index.json) retains21newcases, selected\n555/1/8 with the unchanged mediaOS1314failure, actualCommunity2026.09.0\nHTTPS/Unicode/parallel-row/CAS/count/schema/unknown-commit/lost-stdout checks,\nall5fixturecredential removals and server cleanup, and mandatory integrated\nchecks. The cfg-import bridge preserves the exact production binary. Raw\nfailures remain; no whole-native, other-platform/upstream/JVM-vendor/main/release\ncertification follows. Previous README bytes are preserved.\n'''.encode();readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
proof={'historicalInventoryLedgerAndSummaryUnchanged':True,'oldTrackerSha256':hashlib.sha256((out/'frozen-tracker.json').read_bytes()).hexdigest(),'currentTrackerSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'priorReadmeBytesPreserved':p,'mainOrReleaseVerified':False};(out/'documentation-proof.json').write_bytes((json.dumps(proof,indent=2)+'\n').encode());shutil.copyfile(Path(__file__),out/'runners'/Path(__file__).name)
index['artifacts']=[]
for file in sorted(out.rglob('*')):
 if file.is_file() and file.name!='index.json':raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest(),'historicalLedgerUnchanged':True,'readmePrefixPreserved':p}))
