from pathlib import Path
import json,re,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'docs/validation/integration-2026-10-19/arxiv-source';index=json.loads((out/'index.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();old=json.loads(tracker.read_bytes());value=json.loads(tracker.read_bytes())
assert re.search(r'\bpass\s+260\b',(out/'integrated/tests.log').read_text(encoding='utf-8'));assert 'Ran 17 tests' in (out/'integrated/python-contracts.log').read_text(encoding='utf-8')
paths=['native/src/arxiv_source.rs','native/src/arxiv_rate.rs','native/src/source_acquisition_http.rs','native/src/source_acquisition_cli.rs','native/tests/arxiv_source.rs','native/tests/arxiv_source_http.rs','native/tests/arxiv_source_cli.rs','native/docs/arxiv-source.md','docs/validation/integration-2026-10-19/arxiv-source/index.json']
boundary='Bounded selected PostgreSQL catalogs, anonymous HTTP text/raw-HTML/PDF, and dedicated shared-rate-coordinated arxiv.org received metadata/PDF now have actual Windows evidence. Received citation tags and matching base-ID labels do not establish paper/version/author/server truth, a browser DOM, semantic graph or canonical code snapshot. Google shortcut export, authenticated/social/video routes, article/semantic interpretation and Neo4j/FalkorDB upsert remain open, along with full upstream and non-Windows qualification. Local files and generated Cypher are not those remaining connectors.'
r7=next(x for x in value['prioritizedRemainingScopes'] if x['id']=='R7');r7['missingBehavior']=boundary;r7['suggestedScope']='Continue remaining Google/social/video/authenticated/database connectors and cross-source semantic behavior. Preserve host-only endpoint/credential authority, exact immutable received evidence, bounded protocols, honest missing capabilities and zero query-time acquisition.'
for path in paths:
 if path not in r7['evidence']:r7['evidence'].append(path)
r7['arxivCheckpoint']={'index':paths[-1],'newContractCases':index['newContractCases'],'finalScopedTotals':index['nativeTotals'],'nativeGroups':index['nativeResultGroups'],'actualPlatform':'Windows','actualFrontendMetadata':True,'actualPdfCapture':True,'legacyApiUsed':False,'sharedRateJournal':True,'authorIdentityAttested':False,'versionTruthAttested':False,'serverTruthAttested':False,'wholeNativeRerun':False,'fullUpstreamContractCertified':False}
next(x for x in value['residualRegister'] if x['id']=='document_external_acquisition_database')['remainingBoundary']=boundary
http=next(x for x in value['reconciledCapabilities'] if x['id']=='native_received_http_sources');http['behavior']=http['behavior'].replace('HTML remains raw; arxiv domains are refused until their dedicated rate-aware connector.','Generic HTML remains raw and generic HTTP refuses arxiv domains; the separately evidenced dedicated arxiv connector owns those requests.');http['remainingReview']=boundary
assert not any(x['id']=='native_received_arxiv_sources' for x in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'native_received_arxiv_sources','behavior':'Host-only arxiv.org abstract/PDF acquisition from a bare ID and literal SocketAddr with TLS name verification. Independent network/output/rate-state grants and one shared checksum journal serialize owned requests and start a three-second cooldown after completion. Pending/corrupt/linked/stale state refuses; explicit exact-hash pending recovery sends no request. Storeless frozen head-META inspection preserves distinct citation occurrences and exact original whole-tag byte evidence, with finite parsing/quotas and no publication/version/author/server/source authority.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False})
assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
tot=index['nativeTotals'];failures=', '.join(index['failedCases']) or 'none'
paragraph=f'''[Native received arXiv sources](../native/docs/arxiv-source.md) now add a
dedicated host-authorized abstract/PDF route with literal addresses, verified TLS
names and shared-file rate coordination. The exclusive journal spans each owned
request; completion starts a three-second cooldown. Pending/corrupt state refuses,
and explicit exact-hash pending recovery sends no request. Operators must share
the same coherent-lock file and UTC clocks across participating clients.

Frozen head metadata preserves each citation occurrence, repeated author labels
and exact original whole-tag byte evidence. Decoding does not establish a browser
DOM, paper version, author identity, server truth or a canonical code snapshot.
No default model call, project execution or query-time acquisition is added.

All **16 new contracts** pass. Final selected native evidence records
**{tot['passed']} passed / {tot['failed']} failed / {tot['ignored']} existing ignored gates** in
{index['nativeResultGroups']} groups. Failures remain in the evidence: `{failures}`.
All-target Clippy, format, **17 Python tests**, mandatory typecheck and
**260 TypeScript tests / 3 existing skips** pass with
**{index['frozenIntegratedInputs']} integrated inputs unchanged**. Actual Windows native
frontend metadata and PDF captures share completion sequences 2/4, matching
body/archive hashes and six distinct author occurrences. Arxiv PDF text extraction
is not claimed; public payloads and full abstract output remain ignored/local.

[Evidence](validation/integration-2026-10-19/arxiv-source/index.json) preserves the
initial API-unavailable RED, genuine comment-boundary panic and correction, two
test-fixture corrections, the first Clippy warning and an earlier unchanged
Git-process termination test failure. Attempts are nonadditive; later passes do
not erase earlier failures or establish their cause. Dependencies are unchanged.
Remaining Google/social/video/database and semantic connectors, wider R1–R9,
full native/platform acceptance, main and release stay open.

'''
status=root/'docs/STATUS.md';raw=status.read_text(encoding='utf-8');start=raw.index('The direct arxiv API attempt timed out; a separate public abstract-page');end=raw.index('Windows ZIP and VSIX development packages',start);raw=raw[:start]+'At the preceding HTTP checkpoint, the direct arxiv API attempt timed out and a\npublic abstract-page investigation succeeded. The dedicated native route now has\nits own separately frozen acceptance below; generic HTTP still refuses arxiv.\n\n'+paragraph+raw[end:];status.write_text(raw,encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';raw=architecture.read_text(encoding='utf-8');marker='See [HTTP source acquisition](../native/docs/source-acquisition.md).';assert raw.count(marker)==1;raw=raw.replace(marker,marker+'\nThe dedicated [arXiv source route](../native/docs/arxiv-source.md) adds explicit\nshared-file rate coordination and exact received citation-tag evidence, without\npublication/version/author authority or query-time acquisition.');architecture.write_text(raw,encoding='utf-8',newline='\r\n')
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();proof=index['priorEvidenceReadme'];assert len(prefix)==proof['bytes'] and hashlib.sha256(prefix).hexdigest()==proof['sha256'];append=f'''\n## Coordinated received arXiv source checkpoint\n\nSource `{index['sourceCommit']}`, runtime merge `{index['runtimeCommit']}`.\n[Indexed evidence](arxiv-source/index.json) retains 16 new contracts, selected\n{tot['passed']}/{tot['failed']}/{tot['ignored']} native outcomes, actual Windows metadata/PDF\ncapture with one shared journal, source-integrity proofs and mandatory integrated\nchecks. Initial failures remain nonadditive. Public payloads/full abstract output\nremain local; received tag labels are not publication, version or author truth.\nNo whole-native/platform/upstream, main or release certification follows.\n'''.encode();readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
print(json.dumps({'historicalLedgerUnchanged':True,'readmePrefixPreserved':proof,'nativeTotals':tot,'integratedInputs':index['frozenIntegratedInputs']}))
