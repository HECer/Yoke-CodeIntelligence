from pathlib import Path
import json,copy
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\docs\parity\native-reconciliation-2026-10-15.json')
raw=p.read_bytes();data=json.loads(raw);old=copy.deepcopy(data);found=[]
def visit(v):
 if isinstance(v,dict):
  if v.get('id')=='R8':found.append(v)
  else:
   for child in v.values():visit(child)
 elif isinstance(v,list):
  for child in v:visit(child)
visit(data);assert len(found)==1
r=found[0]
r['missingBehavior']=r['missingBehavior'].replace(', unattended App history-job integration','')
r['suggestedScope']='Implement receiver-side independent source verification and source-bound reasoning under explicit host grants; qualify a configured live App installation/deployment separately. The explicit App history driver already authenticates worker completion and captured Git head, signs the original cumulative plan, drives bounded batches and fences recovery with host-supplied queue/history revisions. No background service is installed. Preserve existing finite remote per-file verification, approval/network/deployment/model boundaries and immutable evidence; do not infer proprietary hosted parity.'
r['appHistoryDevelopment']={'contract':'native/docs/app-history-integration.md','source':'native/src/github_app_worker_history.rs','tests':['native/tests/github_app_history_bridge.rs','native/tests/github_app_history_cli.rs'],'scope':'Explicit worker-completion/history execution, signed original cumulative plan, exact queue/history revisions, existing-only read-only status, typed retention, no silent generic-job adoption/recreation. Each durable phase survives later refusal.','boundary':'No automatic background service, fresh remote history attestation, semantic reasoning, production App deployment or full contract certification.'}
out=json.dumps(data,ensure_ascii=False,indent=2,sort_keys=True)+'\n'
# Only the R8 behavioral development metadata changes; inventory objects remain exact.
check=copy.deepcopy(data)
def restore(v):
 if isinstance(v,dict):
  if v.get('id')=='R8':
   v.pop('appHistoryDevelopment');v['missingBehavior']=next_old['missingBehavior'];v['suggestedScope']=next_old['suggestedScope']
  else:
   for child in v.values():restore(child)
 elif isinstance(v,list):
  for child in v:restore(child)
old_rows=[]
def get_old(v):
 if isinstance(v,dict):
  if v.get('id')=='R8':old_rows.append(v)
  else:
   for child in v.values():get_old(child)
 elif isinstance(v,list):
  for child in v:get_old(child)
get_old(old);next_old=old_rows[0];restore(check);assert check==old
p.write_text(out,encoding='utf-8',newline='\n')
print('Only R8 development metadata changed; inventory and all other rows preserved.')
