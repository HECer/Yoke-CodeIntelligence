from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';raw=base/'function-body-completion';out=raw/'call-types-r2';out.mkdir(exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_inline_body_types_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs'];worker=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts)
def replace(old,new):
 global worker
 old=old.encode();assert worker.count(old)==1,old;worker=worker.replace(old,new.encode())
replace('names.returnType=[fresh(),fresh(),fresh()]','names.returnType=[fresh(),fresh()]')
replace("let replacement=(wrap?'{'+line:'')+'let '+names.result+';'+line+'{'+line;if(names.returnType)","let witness='';let replacement=(wrap?'{'+line:'')+'let '+names.result+';'+line+'{'+line;if(names.returnType)")
replace("replacement+='if (void 0 !== void 0) {'+line+'var '+names.returnType[0]+' = '+docs+(docs?line:'')+'function'+text.slice(open.getStart(sf),selected.end)+';void '+names.returnType[0]+';'+line+'}'+line;", "witness='if (void 0 !== void 0) {'+line+'var '+names.returnType[0]+' = '+docs+(docs?line:'')+'function'+text.slice(open.getStart(sf),selected.end)+';var '+names.returnType[1]+' = '+names.returnType[0]+'('+names.arguments.join(',')+');void '+names.returnType[1]+';'+line+'}'+line;")
replace("replacement+=trivia(close.getStart(sf))+line+'{'+line;", "replacement+=trivia(close.getStart(sf))+line+witness+'{'+line;")
replace("const [seed,args,returned]=names.returnType,type='typeof '+seed+' extends (...'+args+': any[]) => infer '+returned+' ? '+returned+' : never';", "const type='typeof '+names.returnType[1];")
replace("'Body context has errors after expansion'", "'Body context has errors after expansion: '+JSON.stringify(afterDiagnostics)")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py')
requests=[json.loads((raw/f'jsdoc-generic-r1/published-request-{i}.json').read_bytes()) for i in [0,1]]
header='/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}'
for source in [
 header+'export async function outer(){const result=helper(await Promise.resolve(3));return result+1;}',
 header+'\n/** @returns {Generator<number,number,number>} */\nexport function* outer(){const result=helper(yield 3);return result+1;}',
 '/** @type {string|null} */ let value="owned";\n/** @template T\n * @param {T} n\n * @param {null} later\n * @returns {T} */\nfunction helper(n,later){void later;return n;}const result=helper(value,(value=null));export const observed=result.length;',
]:
 request=json.loads(requests[0] and (raw/'jsdoc-generic-r1/published-request-0.json').read_bytes());request['contents']['main.js']=source;request['intent'].update({'startByte':source.index('function helper'),'endByte':source.index('}',source.index('function helper'))+1,'nameStartByte':source.index('helper('),'nameEndByte':source.index('helper(')+6});requests.append(request)
results=[]
for i,request in enumerate(requests):
 config=json.loads(request['contents']['tsconfig.json']);config['compilerOptions']['allowUnreachableCode']=False;config['compilerOptions']['allowUnusedLabels']=False;request['contents']['tsconfig.json']=json.dumps(config);request['sourceBindings']={p:hashlib.sha256(t.encode()).hexdigest() for p,t in request['contents'].items()};data=json.dumps(request).encode();(out/f'request-{i}.json').write_bytes(data)
 p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],cwd=out,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr);row={'case':i,'exitCode':p.returncode,'error':p.stderr.decode()};results.append(row)
 if p.returncode==0:
  r=json.loads(p.stdout)['bodyInline'];row['beforeResultTypes']=[t['type'] for t in r['before']['types'] if t['span']['text']=='result'];row['afterResultTypes']=[t['type'] for t in r['after']['types'] if t['span']['text']=='result'];row['callerTypesExact']=row['beforeResultTypes']==row['afterResultTypes']
result={'cases':results,'allChecksPassed':all(x['exitCode']==0 and x.get('callerTypesExact') for x in results),'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'prototypeWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
