from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';out=base/'function-body-completion/worker-red-r1';out.mkdir(parents=True,exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_inline_body_types_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs']
worker=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts)
old=b"'Body context has errors after expansion'";assert worker.count(old)==1
worker=worker.replace(old,b"'Body context has errors after expansion: '+JSON.stringify({afterDiagnostics,projected:source.get(file)})")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py')
cases=[
 'function helper():number{var row={n:3};return row.n;}const result=helper();export const observed=result;',
 'function helper():number{throw new Error("owned");}try{helper();}catch(e){}export const observed=1;',
 'const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper():number{return opaque();}const result=helper();export const observed=result;',
 'function helper():number|undefined{exit:while(true){try{return 1;}finally{break exit;}}}const result=helper();export const observed=result;',
 'function helper():number{try{return 1;}finally{return 2;}}const result=helper();export const observed=result;',
 'const events:number[]=[];function helper(){var row={n:3};events.push(row.n);}helper();export const observed=events;',
]
results=[]
for i,source in enumerate(cases):
 request=json.loads((base/'function-body-inline/worker-r1/request.json').read_text());request['contents']['main.ts']=source
 request['intent']['startByte']=source.index('function helper');request['intent']['nameStartByte']=source.index('helper(');request['intent']['nameEndByte']=request['intent']['nameStartByte']+6
 request['intent']['endByte']=source.index('const result=') if 'const result=' in source else source.index('try{helper();}') if i==1 else source.index('helper();export')
 config=json.loads(request['contents']['tsconfig.json']);config['compilerOptions'].update({'allowUnreachableCode':False,'allowUnusedLabels':False,'noUnusedLocals':True,'noUnusedParameters':True,'noFallthroughCasesInSwitch':True});request['contents']['tsconfig.json']=json.dumps(config)
 request['sourceBindings']={p:hashlib.sha256(t.encode()).hexdigest() for p,t in request['contents'].items()};data=json.dumps(request).encode();(out/f'request-{i}.json').write_bytes(data)
 p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],cwd=out,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr)
 results.append({'case':i,'exitCode':p.returncode,'error':p.stderr.decode(errors='replace')[:12000]})
result={'cases':results,'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'diagnosticOnlyWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
