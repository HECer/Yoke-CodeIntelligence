from pathlib import Path
import json,subprocess,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';out=base/'function-var-hoisting/worker-r1';out.mkdir(exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs'];(out/'worker.cjs').write_bytes(b'\n'.join((w/'native/src'/p).read_bytes() for p in parts));shutil.copyfile(__file__,out/'runner.py')
cases=[
'function helper(n:number):number{var x=n+1;return x;}const result=helper(2);export const observed=result;',
'function helper(n=1,f=()=>n):number[]{var n=2;return [n,f()];}const result=helper();export const observed=result;',
'function helper(n:number):number{var n:number;n++;return n;}const result=helper(2);export const observed=result;',
'function helper():number{var x:number=1;var x:number;x++;return x;}const result=helper();export const observed=result;',
'function helper():number[]{const fs:Array<()=>number>=[];for(var i=0;i<3;i++)fs.push(()=>i);return fs.map(f=>f());}const result=helper();export const observed=result;',
'function helper():number[]{const fs:Array<()=>number>=[];for(var [n] of [[1],[2]])fs.push(()=>n);return fs.map(f=>f());}const result=helper();export const observed=result;',
'function helper():unknown{var {x:n=3,...rest}:{x?:number,y?:number}={y:4};return [n,rest];}const result=helper();export const observed=result;',
'function helper():unknown{try{throw 1;}catch(e){var e=2;}return e;}const result=helper();export const observed=result;',
'function helper():string[]{const fs:Array<()=>string>=[];for(var i=0;i<3;i++)fs.push(()=>i.toFixed());return fs.map(f=>f());}const result=helper();export const observed=result;'
];results=[]
for i,source in enumerate(cases):
 request=json.loads((base/'function-body-inline/worker-r1/request.json').read_text());request['contents']['main.ts']=source;request['intent']['endByte']=source.index('const result=');request['sourceBindings']['main.ts']=hashlib.sha256(source.encode()).hexdigest();raw=json.dumps(request).encode();(out/f'request-{i}.json').write_bytes(raw)
 p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(raw).hexdigest()],input=raw,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=out);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr)
 results.append({'case':i,'exitCode':p.returncode,'error':p.stderr.decode(errors='replace')})
 if p.returncode==0:
  r=json.loads(p.stdout);after=source.encode()
  for e in sorted(r['bodyInline']['edits'],key=lambda e:e['startByte'],reverse=True):after=after[:e['startByte']]+e['text'].encode()+after[e['endByte']:]
  (out/f'after-{i}.ts').write_bytes(after)
result={'cases':results,'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
