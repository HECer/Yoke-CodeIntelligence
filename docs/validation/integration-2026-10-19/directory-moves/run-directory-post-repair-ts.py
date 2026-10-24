from pathlib import Path
import subprocess,time,json,hashlib
root=Path.cwd();out=root/'validation/integration-20261018/directory-moves-development/post-repair-typescript';out.mkdir(exist_ok=False);names=json.loads((root/'validation/integration-20261018/directory-moves-final/source-before.json').read_bytes());before={n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in names};(out/'source-before.json').write_bytes((json.dumps(before,indent=2)+'\n').encode());results=[]
for name,cmd in [('typecheck',['npm.cmd','run','typecheck']),('tests',['npm.cmd','test'])]:
 start=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 value={'gate':name,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd};results.append(value);print(json.dumps(value),flush=True)
after={n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in names};(out/'source-after.json').write_bytes((json.dumps(after,indent=2)+'\n').encode());(out/'summary.json').write_bytes((json.dumps({'checks':results,'sourceUnchanged':before==after},indent=2)+'\n').encode());assert before==after;raise SystemExit(1 if any(x['exitCode'] for x in results) else 0)
