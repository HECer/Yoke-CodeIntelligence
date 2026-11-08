"""Capture an actual immutable default-report control before R3 source edits."""
from pathlib import Path
import hashlib,json,subprocess,tempfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');exe=w/'target/debug/yoke.exe'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()=='52f60c2097356f41f0573f99d47977b204cfa39f'
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
assert hashlib.sha256(exe.read_bytes()).hexdigest()=='989c2bbce725aa22dbfd2efdfd5bb341994e5dffb29d56515fa3068eb96fedaf'
out=w/'validation/integration-20261018/r3-source-categories/legacy-before-r1';out.mkdir(parents=True,exist_ok=False)
owned=Path(tempfile.mkdtemp(prefix='yoke-owned-r3-policy-')).resolve()
files={'src/core.rs':'pub fn core(){ leaf(); }\nfn leaf(){}\npub fn Path(){core();}\n',
 'src/consumer.rs':'pub fn consumer(){core();Path();missing();}\n',
 'web/client.ts':'export function client(){ return 3; }\n',
 'docs/design.md':'# Owned design\n\nCore and consumer retain separate byte ranges.\n'}
for n,s in files.items():
    path=owned/n;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(s.encode())
commands=[];reports=[]
for label,tail in [('index',['index']),('report-1',['graph','analyze']),('report-2',['graph','analyze'])]:
    command=[str(exe),'--root',str(owned),'--timeout','30000','--max-bytes','1000000',*tail]
    p=subprocess.run(command,cwd=w,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=45,creationflags=0x08000000)
    (out/(label+'.stdout')).write_bytes(p.stdout);(out/(label+'.stderr')).write_bytes(p.stderr)
    commands.append({'label':label,'command':command,'exitCode':p.returncode});assert p.returncode==0
    if label.startswith('report'):reports.append(json.loads(p.stdout))
assert reports[0]==reports[1]
assert all((owned/n).read_bytes()==s.encode() for n,s in files.items())
result={'sourceCommit':'52f60c2097356f41f0573f99d47977b204cfa39f','cliSha256':hashlib.sha256(exe.read_bytes()).hexdigest(),
 'ownedRoot':str(owned),'sourceFiles':{n:hashlib.sha256(s.encode()).hexdigest() for n,s in files.items()},
 'commands':commands,'defaultReport':reports[0],'sameReportRepeated':True,'sourceUnchanged':True,
 'purpose':'Re-run unchanged checked saved scope after R3 build and require identical legacy JSON, artifact and Markdown identities.'}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'ownedRoot':str(owned),'reportKeys':list(reports[0]),'repeatIdentical':True}))
