from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';out=base/'function-body-completion/worker-green-r1';out.mkdir(exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_inline_body_types_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs'];worker=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts)
old=b"const returns=n=>";assert worker.count(old)==1;worker=worker.replace(old,b"let returnCount=0;const returns=n=>")
old=b"if(ts.isReturnStatement(n)){const at=";assert worker.count(old)==1;worker=worker.replace(old,b"if(ts.isReturnStatement(n)){returnCount++;const at=")
old=b"body=body.slice(0,-1)+line+names.result+' = void 0;'+line+'}';replacement+=names.exit+': '+body+line+'}'+line+'}'+line;";assert worker.count(old)==1
worker=worker.replace(old,b"replacement+=(returnCount?names.exit+': ':'')+'{'+line+'if (void 0 === void 0) '+body+line+names.result+' = void 0;'+line+'}'+line+'}'+line+'}'+line;")
old=b"'Body context has errors after expansion'";assert worker.count(old)==1;worker=worker.replace(old,b"'Body context has errors after expansion: '+JSON.stringify({afterDiagnostics,projected:source.get(file)})")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py');results=[]
for p in sorted((out.parent/'worker-red-r1').glob('request-*.json')):
 data=p.read_bytes();shutil.copyfile(p,out/p.name);i=int(p.stem.split('-')[-1]);r=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],cwd=out,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/f'stdout-{i}.json').write_bytes(r.stdout);(out/f'stderr-{i}.log').write_bytes(r.stderr)
 results.append({'case':i,'exitCode':r.returncode,'error':r.stderr.decode(errors='replace')[:12000]})
 if r.returncode==0:
  reply=json.loads(r.stdout);source=json.loads(data)['contents']['main.ts'].encode()
  for e in sorted(reply['bodyInline']['edits'],key=lambda x:x['startByte'],reverse=True):source=source[:e['startByte']]+e['text'].encode()+source[e['endByte']:]
  (out/f'after-{i}.ts').write_bytes(source)
result={'cases':results,'allChecksPassed':all(x['exitCode']==0 for x in results),'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'prototypeWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
