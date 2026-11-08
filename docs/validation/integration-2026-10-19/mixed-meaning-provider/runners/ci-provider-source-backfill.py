from pathlib import Path
import json,hashlib,subprocess
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
base=work.parent/'Yoke-CodeIntelligence/validation/integration-20261018/mixed-meaning-provider'
sha=lambda b:hashlib.sha256(b).hexdigest();proof=[]
for phase in sorted(base.iterdir()):
    manifest=phase/'source-before.json'
    if not manifest.is_file():continue
    before=json.loads(manifest.read_bytes());after=json.loads((phase/'source-after.json').read_bytes())
    for name in ['native/src/main.rs','native/src/mixed_meaning_cli.rs']:
        if name not in before:continue
        h=before[name];target=phase/'source'/name
        options=[]
        if target.is_file():options.append(('existing raw phase copy',target.read_bytes()))
        options.append(('current raw source', (work/name).read_bytes()))
        if name=='native/src/main.rs':
            raw=subprocess.check_output(['git','show','9d6968637fdde5ac556af9eae8c07c2c07c2d3a7:'+name],cwd=work)
            options.extend([('parent git blob exact',raw),('parent git blob CRLF materialization',raw.replace(b'\r\n',b'\n').replace(b'\n',b'\r\n'))])
        matched=next(((origin,raw) for origin,raw in options if sha(raw)==h),None)
        assert matched is not None,(phase.name,name,h)
        origin,raw=matched;assert after[name]==h
        target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
        proof.append({'phase':phase.name,'path':name,'bytes':len(raw),'sha256':h,'beforeAndAfterMatched':True,'origin':origin})
(base/'source-filter-backfill.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'exactVerifiedCopies':len(proof),'unmatched':0}),flush=True)
