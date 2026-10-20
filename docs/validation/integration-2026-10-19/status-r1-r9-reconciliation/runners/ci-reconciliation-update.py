from pathlib import Path
import json,subprocess,hashlib
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence'
out=root/'validation/integration-20261018/status-r1-r9-reconciliation';out.mkdir(parents=True,exist_ok=False)
p=w/'docs/parity/native-reconciliation-2026-10-15.json';raw=p.read_bytes();old=json.loads(raw);original=json.loads(raw)
(out/'tracker-before.json').write_bytes(raw)
def find(v,id):
    if isinstance(v,dict):
        if v.get('id')==id:return v
        for x in v.values():
            r=find(x,id)
            if r is not None:return r
    elif isinstance(v,list):
        for x in v:
            r=find(x,id)
            if r is not None:return r
r2=find(old,'R2');r3=find(old,'R3');r8=find(old,'R8')
r2['missingBehavior']='Explicit semantic alias identity/de-duplication policy, broader interpretation/outcome behavior and full upstream contract equivalence remain open. Checked same-snapshot native joins and their lookup/degree/directed-trace consumers retain complete native source/receipt replay. Separate portable native group/join consumers validate supplied presentation structure without source authority. The separate mixed-source APIs now cover typed Office XML/decoded/PDF preparation, source-checked claims, explicit durable provider jobs, checked graph consumers and independently checked cross-history/workspace library composition with graph-qualified keys and unverified opt-in host links. Legacy native join schemas remain unchanged. Coherent data-only forgeries/omissions, semantic/model quality and independent interpretation truth remain unverified.'
r2['suggestedScope']='Retain native checked replay and the separate portable-data and mixed-source capability boundaries. Continue explicit semantic identity/outcome policy without promoting aliases, host assertions or serialized JSON into source or semantic authority. Reuse the implemented R4 mixed preparation/provider/query/composition contracts rather than reimplementing those capabilities.'
for name in ['native/docs/mixed-meaning-composition.md','native/src/mixed_meaning_composition.rs','docs/validation/integration-2026-10-19/mixed-meaning-composition/index.json']:
    if name not in r2['evidence']:r2['evidence'].append(name)
r3['missingBehavior']='Checked historical pair-of-Views comparison is implemented in the Rust library and explicit pinned graph compare-views CLI. Selected analytics previousSnapshotId remains refused because it does not pin both View selections. Remaining full pinned upstream analytics equivalence is an acceptance/qualification gap; this field does not describe the implemented pair comparison as missing behavior.'
r8['missingBehavior']='Configured public production deployment/live App installation, unattended App history-job integration, receiver-side independent source verification and semantic reasoning, whole-repository remote equality and universal upstream contract equivalence remain open. Existing worker per-file remote Git-blob/patch checks retain their finite checked-source contract; matching metadata is not an atomic whole-tree proof. Explicit persistent local jobs continue atomic captured-history acquisition across bounded batches and process restarts with cumulative quotas and exact revision preconditions. The independently owned client/receiver implement approval/send/durable-intake/GET-recovery and separately authorized literal metadata queries. Receipt-qualified rows remain sender claims, not receiver-verified source truth or proprietary brain parity.'
r8['suggestedScope']='Integrate explicit persistent history jobs with the durable App worker lifecycle under current grants, exact job/head/revision preconditions and cumulative quotas. Preserve idempotent recovery, separate network/write/deployment/model authority and original immutable source/receipt bindings. Existing bounded closed-PR discussion captures and digest/service handoff already have separate implemented contracts; do not reimplement them or infer hosted semantic parity.'
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==original[key]
for id in ['R1','R4','R5','R6','R7','R9']:assert find(old,id)==find(original,id)
p.write_text(json.dumps(old,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
(out/'tracker-after.json').write_bytes(p.read_bytes())
baseline=json.loads((root/'validation/integration-20261018/mixed-meaning-composition/integrated/source-after.json').read_bytes())
sha=lambda b:hashlib.sha256(b).hexdigest()
runtime={n:h for n,h in baseline.items() if n!='docs/parity/native-reconciliation-2026-10-15.json'}
assert all(sha((w/n).read_bytes())==h for n,h in runtime.items())
result={'sourceParent':'6181fcb01522de2e1d82b7aa6c22d72a1ab65f1b','unchangedRuntimeInputs':len(runtime),'changedTrackerRows':['R2','R3','R8'],'inventoryLedgerUnchanged':True,'historicalInventoryUnchanged':True,'ledgerSummaryUnchanged':True,'otherPriorityRowsUnchanged':True,'noBehaviorChange':True,'nativeOrTypescriptTestsRerun':False,'wholeGoalComplete':False}
(out/'reconciliation.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
