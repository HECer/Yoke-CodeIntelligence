from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-completion';out=raw/'literal-box-r1';out.mkdir(exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_inline_body_types_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs'];worker=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts)
old=b"witness='if (void 0 !== void 0)";assert worker.count(old)==1;worker=worker.replace(old,b"const invocation=names.returnType[0]+'('+names.arguments.join(',')+')',box=/\\.[cm]?jsx?$/.test(file)?'/** @type {const} */ ({[\"value\"]: '+invocation+'})':'({[\"value\"]: '+invocation+'} as const)';witness='if (void 0 !== void 0)")
old=b"+';var '+names.returnType[1]+' = '+names.returnType[0]+'('+names.arguments.join(',')+');void '";assert worker.count(old)==1;worker=worker.replace(old,b"+';var '+names.returnType[1]+' = '+box+';void '")
old=b"const type='typeof '+names.returnType[1];";assert worker.count(old)==1;worker=worker.replace(old,b"const type='(typeof '+names.returnType[1]+')[\"value\"]';")
for phase in ['before','after']:
 old=("'Body context has errors "+phase+" expansion'").encode();worker=worker.replace(old,("'Body context has errors "+phase+" expansion: '+JSON.stringify("+phase+"Diagnostics)").encode())
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py')
requests=[json.loads((raw/f'doc-bindings-r1/request-{i}.json').read_bytes()) for i in range(9)]
source='function helper(){return 3 as const;}const result=helper();export const observed=result+1;';r=json.loads((raw/'fixtures-r2/request-0.json').read_bytes());r['contents']['main.ts']=source;r['intent'].update({'path':'main.ts','startByte':0,'endByte':source.index('const result='),'nameStartByte':source.index('helper('),'nameEndByte':source.index('helper(')+6});requests.append(r)
results=[]
for i,r in enumerate(requests):
 r['sourceBindings']={p:hashlib.sha256(t.encode()).hexdigest() for p,t in r['contents'].items()};data=json.dumps(r).encode();(out/f'request-{i}.json').write_bytes(data);p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=out);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr);row={'case':i,'exitCode':p.returncode,'error':p.stderr.decode()};results.append(row)
 if p.returncode==0:
  reply=json.loads(p.stdout)['bodyInline'];before=[t['type'] for t in reply['before']['types'] if t['span']['text']=='result'];after=[t['type'] for t in reply['after']['types'] if t['span']['text']=='result'];row.update({'beforeResultTypes':before,'afterResultTypes':after,'callerTypesExact':before==after})
  source=r['contents'][r['intent']['path']].encode()
  for e in sorted(reply['edits'],key=lambda e:e['startByte'],reverse=True):source=source[:e['startByte']]+e['text'].encode()+source[e['endByte']:]
  (out/f'after-{i}.{r["intent"]["path"].split(".")[-1]}').write_bytes(source)
result={'cases':results,'allChecksPassed':all(x['exitCode']==0 and x.get('callerTypesExact') for x in results),'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'prototypeWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
