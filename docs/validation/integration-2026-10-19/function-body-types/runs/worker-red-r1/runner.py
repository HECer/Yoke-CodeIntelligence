from pathlib import Path
import json,hashlib,subprocess,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';phase=sys.argv[1];out=base/'function-body-types'/phase;out.mkdir(parents=True,exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs']
worker=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts)
old=b"'Body context has errors after expansion'";assert worker.count(old)==1
worker=worker.replace(old,b"'Body context has errors after expansion: '+JSON.stringify(afterDiagnostics)")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py')
cases=[
 'function helper():number[]{const fs:Array<()=>number>=[];for(var row of [{n:1},{n:2}])fs.push(()=>row.n);return fs.map(f=>f());}const result=helper();export const observed=result;',
 'function helper():number[]{const fs:Array<()=>number>=[];for(var row of [[1],[2]])fs.push(()=>row[0]);return fs.map(f=>f());}const result=helper();export const observed=result;',
 'function helper():number[]{const fs:Array<()=>number>=[];for(var row of [["a",1] as const,["bb",2] as const])fs.push(()=>row[0].length);return fs.map(f=>f());}const result=helper();export const observed=result;',
 'function helper():number[]{const fs:Array<()=>number>=[];for(var fn of [(x:number)=>x+1,(x:number)=>x+2])fs.push(()=>fn(3));return fs.map(f=>f());}const result=helper();export const observed=result;',
 'function helper():number[]{const fs:Array<()=>number>=[];for(var row of [{n:1},null])fs.push(()=>row?.n??0);return fs.map(f=>f());}const result=helper();export const observed=result;',
 'function helper():number[]{const fs:Array<()=>number>=[];for(var [key,row] of new Map([[1,{n:1}],[2,{n:2}]]))fs.push(()=>row.n+key);return fs.map(f=>f());}const result=helper();export const observed=result;',
 'function helper():()=>number{var read=()=>row.n;var row={n:3};return read;}const result=helper();export const observed=result();'
]
results=[]
for i,source in enumerate(cases):
 request=json.loads((base/'function-body-inline/worker-r1/request.json').read_text());request['contents']['main.ts']=source;request['intent']['endByte']=source.index('const result=');request['sourceBindings']['main.ts']=hashlib.sha256(source.encode()).hexdigest();data=json.dumps(request).encode();(out/f'request-{i}.json').write_bytes(data)
 p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],cwd=out,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr)
 results.append({'case':i,'exitCode':p.returncode,'error':p.stderr.decode(errors='replace')[:4000]})
 if p.returncode==0:
  r=json.loads(p.stdout);after=source.encode()
  for e in sorted(r['bodyInline']['edits'],key=lambda e:e['startByte'],reverse=True):after=after[:e['startByte']]+e['text'].encode()+after[e['endByte']:]
  (out/f'after-{i}.ts').write_bytes(after)
result={'cases':results,'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'diagnosticOnlyWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
