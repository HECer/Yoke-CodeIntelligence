from pathlib import Path
import json,copy,hashlib
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=w/'docs/parity/native-reconciliation-2026-10-15.json'
out=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/semantic-transcript/tracker'
out.mkdir(exist_ok=False)
raw=p.read_bytes();before=json.loads(raw);after=copy.deepcopy(before)
def find(o):
    if isinstance(o,dict):
        if o.get('id')=='R9':return o
        for v in o.values():
            r=find(v)
            if r is not None:return r
    if isinstance(o,list):
        for v in o:
            r=find(v)
            if r is not None:return r
r=find(after);old=find(before);assert r and old
r['missingBehavior']='Source-bound knowledge, saved-plan and packet-qualified Meaning outcomes retain their separate released immutable event contracts. Explicit semantic transcript candidates now use a separate host-authorized provider job over selected original imported UTF8 bytes, with single-use ledger admission, durable original response/billing, exact source/payload/response/result replay, provider-free status/jobs/checked export and typed retention. Candidates create no events. Further object surfaces, independently authenticated corroboration, live model-quality qualification, current applicability and actual application/execution evidence remain open. Labels, scores and model interpretations grant no semantic truth or authority.'
r['suggestedScope']='Continue remaining typed object surfaces, independent corroboration and actual execution evidence with exact source-space/receipt replay and separately qualified live providers. Preserve knowledge, saved-plan and Meaning event schemas, immutable history, actor/session sequence domains and transparent ranking. Semantic transcript candidates require host review and grant no automatic promotion, retry, native code identity or execution authority. Central AgentMemory remains separate.'
r['semanticTranscriptDevelopment']={'behavior':'Explicit host-selected contiguous original imported UTF8 span, including full-import context within the finite cap. Separate single-attempt protocol/job and single-use reservation; bound bills retained before invalid domain/usage/provider-error/nonzero/overrun rejection; unknown sends retain reservations. Immutable original payload/reply/candidate artifacts, exact registered source and ledger replay, private checked export, read-only status/jobs, no automatic retry or candidate-to-event promotion. Typed retention validates all job states.','contract':'native/docs/semantic-transcript-candidates.md','evidence':'docs/validation/integration-2026-10-19/semantic-transcript/index.json','sourceParent':'dae57012519f013103541c1ee0ea21bb897a6409','recordKind':'native_transcript_semantic_job_v1','operation':'extract_transcript_feedback_v1','sourceKind':'transcriptUtf8','existingFeedbackFormatsUnchanged':True,'eventsCreated':0,'sourceWriteScope':'native_parent_only','providerFilesystemIsolationEnforced':False,'interpretationTruthEstablished':False,'independentCorroborationEstablished':False,'executionObserved':False,'liveModelQualityQualified':False,'fullContractCertified':False}
check=copy.deepcopy(after);rr=find(check)
for k in ['missingBehavior','suggestedScope']:rr[k]=old[k]
del rr['semanticTranscriptDevelopment'];assert check==before
p.write_text(json.dumps(after,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
(out/'before.json').write_bytes(raw);(out/'after.json').write_bytes(p.read_bytes())
(out/'proof.json').write_text(json.dumps({'onlyChangedR9Fields':['missingBehavior','suggestedScope','semanticTranscriptDevelopment'],'allOtherDataExact':True,'inventoryAndCertificationsUnchanged':True,'beforeSha256':hashlib.sha256(raw).hexdigest(),'afterSha256':hashlib.sha256(p.read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf-8')
print('Only three R9 development fields changed; all other data exact.')
