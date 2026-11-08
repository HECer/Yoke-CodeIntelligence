from pathlib import Path
import json,hashlib,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
base=work.parent/'Yoke-CodeIntelligence/validation/integration-20261018/python-empty-suite'
names=['native/README.md','native/docs/semantic-refactor-intents.md','native/docs/symbol-deletion.md',
    'native/src/changes.rs','native/src/changes_symbol_delete.rs','native/src/protocol.rs',
    'native/src/validation.rs','native/src/mcp_http_rejection_tests.rs','native/tests/python_empty_suite.rs']
sha=lambda b:hashlib.sha256(b).hexdigest()
for phase in ['r1','r2','live-r1','final']:
    out=base/phase;before=json.loads((out/'source-before.json').read_bytes());entries=[]
    for name in names:
        raw=(work/name).read_bytes()
        match=sha(raw)==before.get(name)
        if match:
            path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True)
            if path.exists():assert path.read_bytes()==raw
            else:shutil.copyfile(work/name,path)
        entries.append({'path':name,'expectedSha256':before.get(name),'backfilledRawFromCurrentAfterExactHashEquality':match,
            'historicalHashStillRetained':True})
    (out/'source-backfill.json').write_text(json.dumps(entries,indent=2)+'\n',encoding='utf-8')
print('Historical sources copied only where their full recorded SHA-256 matches; other historical sources remain hash-only.')
