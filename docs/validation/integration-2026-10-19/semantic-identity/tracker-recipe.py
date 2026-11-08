from pathlib import Path
import json,copy,hashlib
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=w/'docs/parity/native-reconciliation-2026-10-15.json'
out=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/semantic-identity/tracker';out.mkdir(exist_ok=False)
raw=p.read_bytes();before=json.loads(raw);after=copy.deepcopy(before)
def find(o):
    if isinstance(o,dict):
        if o.get('id')=='R2':return o
        for v in o.values():
            r=find(v)
            if r is not None:return r
    if isinstance(o,list):
        for v in o:
            r=find(v)
            if r is not None:return r
r=find(after);old=find(before);assert r and old
r['title']='Source-bound meaning graphs, checked semantic identity and build state'
r['missingBehavior']='A separate explicit native semantic-identity policy now combines complete selected native claim/source/receipt replay, deterministic normalization/MinHash/LSH/Jaro candidates, a separately authorized durable model adjudication job, complete per-pair support/coverage checks and positive-clique quotient projection. Frozen node/degree/directed-trace queries preserve all original concepts, parallel/reverse/self observations, ordered role slots, source evidence and original billing without changing parent records. Native selections share one exact workspace/canonical snapshot. Other source/history/workspace identity, broader normalization/survivor rules, further interpretation/outcome surfaces, independent semantic quality and full upstream contract equivalence remain open. Existing aliases/groups/native joins/portable projections and mixed source contracts keep their original formats and authority boundaries. Coherent data-only forgeries and model/provider truth are not attested.'
r['suggestedScope']='Qualify additional source/history/workspace identity scopes, wider normalization/survivor policies and actual model semantics with their own complete source/response/accounting replay. Preserve the released alias/group/join/data-only/mixed contracts and avoid replacing unavailable capabilities with empty complete results. Use the new explicit semantic-identity job only for its finite native policy: labels and scores propose candidates; model decisions remain unverified, all positive components require complete same-entity cliques, original observations remain intact and no source/execution authority follows. Continue full platform/upstream acceptance and remaining R1-R9 work.'
r['semanticIdentityDevelopment']={'behavior':'Separate explicit native joint source/receipt-backed normalization/MinHash64/LSH16x4/Jaro candidates; one host-authorized single-use ledger/job/payload/provider attempt; immutable bound response and original billing; complete decision coverage with exact concept-support IDs; positive-clique quotient retaining every original observation and ordered role slot; provider-free checked load/status/jobs and frozen node/degree/directed-trace queries, including after Store loss. Typed retention bounds new metadata and parent replay before legacy materialization and follows validated new-job/artifact roots only.','contract':'native/docs/meaning-identity.md','evidence':'docs/validation/integration-2026-10-19/semantic-identity/index.json','sourceParent':'7a61aa782462614843fcd879dfffa31e9dacd382','recordKind':'native_meaning_identity_job_v1','operation':'adjudicate_meaning_identity_v1','algorithm':'native_nfkc_context_lower_minhash64_lsh16x4_jaro_v1','mergePolicy':'complete_same_entity_clique_or_refuse','scope':'selected_native_meaning_graphs_one_exact_workspace_canonical_snapshot','releasedParentFormatsUnchanged':True,'originalClaimsModified':False,'semanticIdentityEstablished':False,'canonicalSourceAuthority':False,'executionObserved':False,'providerFilesystemIsolationEnforced':False,'liveSemanticQualityQualified':False,'fullContractCertified':False}
check=copy.deepcopy(after);rr=find(check)
for key in ['title','missingBehavior','suggestedScope']:rr[key]=old[key]
del rr['semanticIdentityDevelopment'];assert check==before
p.write_text(json.dumps(after,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
(out/'before.json').write_bytes(raw);(out/'after.json').write_bytes(p.read_bytes())
(out/'proof.json').write_text(json.dumps({'onlyChangedR2Fields':['title','missingBehavior','suggestedScope','semanticIdentityDevelopment'],'allOtherDataExact':True,'inventoryAndCertificationsUnchanged':True,'beforeSha256':hashlib.sha256(raw).hexdigest(),'afterSha256':hashlib.sha256(p.read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf-8')
print('Only four R2 development fields changed; all other data exact.')
