from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';out=base/'function-body-types/guard-options-r1';out.mkdir(exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_inline_body_types_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs'];original=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts);assert original.count(b'if (false)')==1
sources=["const events:number[]=[];function helper(){var row={n:3};events.push(row.n);}helper();export const observed=events;", "function helper():number{var row={n:3};return row.n;}const result=helper();export const observed=result;"]
results=[]
for mode,worker in [('literal',original),('binary',original.replace(b'if (false)',b'if (void 0 !== void 0)'))]:
 path=out/(mode+'.cjs');path.write_bytes(worker)
 for i,source in enumerate(sources):
  request=json.loads((base/'function-body-inline/worker-r1/request.json').read_text());request['contents']['main.ts']=source;request['intent']['startByte']=source.index('function helper');request['intent']['nameStartByte']=source.index('helper(');request['intent']['nameEndByte']=request['intent']['nameStartByte']+6;request['intent']['endByte']=source.index('}helper();')+1 if i==0 else source.index('const result=');
  config=json.loads(request['contents']['tsconfig.json']);config['compilerOptions']['allowUnreachableCode']=False;request['contents']['tsconfig.json']=json.dumps(config);request['sourceBindings']={p:hashlib.sha256(t.encode()).hexdigest() for p,t in request['contents'].items()};raw=json.dumps(request).encode();(out/f'{mode}-request-{i}.json').write_bytes(raw)
  p=subprocess.run(['node',str(path),hashlib.sha256(raw).hexdigest()],input=raw,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=out);(out/f'{mode}-stdout-{i}.json').write_bytes(p.stdout);(out/f'{mode}-stderr-{i}.log').write_bytes(p.stderr);results.append({'guard':mode,'case':i,'exitCode':p.returncode,'error':p.stderr.decode(errors='replace')})
shutil.copyfile(__file__,out/'runner.py');r={'cases':results,'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False};(out/'result.json').write_text(json.dumps(r,indent=2)+'\n',encoding='utf-8');print(json.dumps(r))
