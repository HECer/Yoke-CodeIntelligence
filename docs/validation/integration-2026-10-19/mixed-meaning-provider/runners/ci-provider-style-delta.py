from pathlib import Path
import json,hashlib
base=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\mixed-meaning-provider')
read=lambda p:json.loads((base/p).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
a=read('final/source-before.json');b=read('final-r2/source-before.json');assert a.keys()==b.keys()
changed=[p for p in a if a[p]!=b[p]];name='native/src/mixed_meaning_provider.rs';assert changed==[name]
old=(base/'final/source'/name).read_bytes();new=(base/'final-r2/source'/name).read_bytes()
assert sha(old)==a[name] and sha(new)==b[name]
s=old.decode().replace('\r\n','\n')
edits=[('''    if let Some(usage) = &reply.usage {
        if usage.validate().is_ok() {
            input(attempt.response_usage.as_ref() == Some(usage))?;
        }
    }''','''    if let Some(usage) = &reply.usage
        && usage.validate().is_ok()
    {
        input(attempt.response_usage.as_ref() == Some(usage))?;
    }'''),('''            if usage_valid {
                if let Some(usage) = &reply.usage {
                    ledger.usage(&job.attempt_id, "response", usage.clone())?;
                }
            }''','''            if usage_valid && let Some(usage) = &reply.usage {
                ledger.usage(&job.attempt_id, "response", usage.clone())?;
            }''')]
for before,after in edits:assert s.count(before)==1;s=s.replace(before,after)
assert s==new.decode().replace('\r\n','\n')
result={'changedPaths':changed,'beforeSha256':a[name],'afterSha256':b[name],
    'exactlyTwoCollapsedIfConditions':True,'allOtherFrozenInputsRawEqual':True}
(base/'style-only-delta.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
