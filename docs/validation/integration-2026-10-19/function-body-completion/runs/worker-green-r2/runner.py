from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018';out=base/'function-body-completion/worker-green-r2';out.mkdir(exist_ok=False)
worker=(out.parent/'worker-green-r1/worker.cjs').read_bytes()
def replace(old,new):
 global worker
 old=old.encode();assert worker.count(old)==1,old;worker=worker.replace(old,new.encode())
replace("let replacement=(wrap?'{'+line:'')+'let '+names.result+';'+line+'{'+line;", "if(!selected.type&&!resultHint)names.returnType=[fresh(),fresh(),fresh()];let replacement=(wrap?'{'+line:'')+'let '+names.result+';'+line+'{'+line;if(names.returnType){const open=selected.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.OpenParenToken);assert(open,'Body return witness parameter delimiter');const docs=(selected.jsDoc||[]).map(n=>n.getText(sf)).join(line);replacement+='if (void 0 !== void 0) {'+line+'var '+names.returnType[0]+' = '+docs+(docs?line:'')+'function'+text.slice(open.getStart(sf),selected.end)+';void '+names.returnType[0]+';'+line+'}'+line;}")
replace("result='('+result+' as ('+text.slice(selected.type.pos,selected.type.end)+'))'", "result='('+result+' as unknown as ('+text.slice(selected.type.pos,selected.type.end)+'))'")
replace("'/** @type {'+resultHint+'} */ ('+result+')':'('+result+' as ('+resultHint+'))'", "'/** @type {'+resultHint+'} */ (/** @type {unknown} */ ('+result+'))':'('+result+' as unknown as ('+resultHint+'))'")
replace("else replacement+=", "else replacement+=") if False else None
replace("replacement+=text.slice(statement.getStart(sf),call.getStart(sf))+result", "if(names.returnType){const [seed,args,returned]=names.returnType,type='typeof '+seed+' extends (...'+args+': any[]) => infer '+returned+' ? '+returned+' : never';result=/\\.[cm]?jsx?$/.test(file)?'/** @type {'+type+'} */ (/** @type {unknown} */ ('+result+'))':'('+result+' as unknown as ('+type+'))';}replacement+=text.slice(statement.getStart(sf),call.getStart(sf))+result")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py');requests=[json.loads(p.read_bytes()) for p in sorted((out.parent/'worker-red-r1').glob('request-*.json'))]
extra=[('ts','function helper(n:number){return {value:n};}const result=helper(3);export const observed=result.value;'),('ts','function helper(){return ()=>3;}const result=helper();export const observed=result();'),('js','/** @param {number} n */\nfunction helper(n){return {value:n};}const result=helper(3);export const observed=result.value;'),('js','/** @returns {number|string} */\nfunction helper(){return 3;}const result=helper();export const observed=result;'),('ts','function helper(){class Local{n=3;}return new Local();}const result=helper();export const observed=result.n;'),('ts','function helper(){type Local={n:number};const row:Local={n:3};return row;}const result=helper();export const observed=result.n;')]
for language,source in extra:
 request=json.loads((base/'function-body-inline/worker-r1/request.json').read_text());request['contents'].pop('main.ts');request['contents']['main.'+language]=source;request['intent']['path']='main.'+language
 request['intent']['startByte']=source.index('function helper');request['intent']['nameStartByte']=source.index('helper(');request['intent']['nameEndByte']=request['intent']['nameStartByte']+6;request['intent']['endByte']=source.index('const result=')
 config=json.loads(request['contents']['tsconfig.json']);config['files']=['main.'+language];config['compilerOptions'].update({'allowUnreachableCode':False,'allowUnusedLabels':False,'noUnusedLocals':True,'noUnusedParameters':True,'noFallthroughCasesInSwitch':True,'allowJs':True,'checkJs':True});request['contents']['tsconfig.json']=json.dumps(config);requests.append(request)
results=[]
for i,request in enumerate(requests):
 request['sourceBindings']={p:hashlib.sha256(t.encode()).hexdigest() for p,t in request['contents'].items()};data=json.dumps(request).encode();(out/f'request-{i}.json').write_bytes(data)
 p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],cwd=out,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr);record={'case':i,'exitCode':p.returncode,'error':p.stderr.decode(errors='replace')[:12000]};results.append(record)
 if p.returncode==0:
  r=json.loads(p.stdout);file=request['intent']['path'];source=request['contents'][file].encode()
  for e in sorted(r['bodyInline']['edits'],key=lambda e:e['startByte'],reverse=True):source=source[:e['startByte']]+e['text'].encode()+source[e['endByte']:]
  (out/f'after-{i}.{file.split(".")[-1]}').write_bytes(source)
  original_types={t['span']['text']:t['type'] for t in r['bodyInline']['before']['types'] if t['span']['text']=='result'};after_types={t['span']['text']:t['type'] for t in r['bodyInline']['after']['types'] if t['span']['text']=='result'};record['callerTypesExact']=original_types==after_types;record['beforeCallerTypes']=original_types;record['afterCallerTypes']=after_types
result={'cases':results,'allChecksPassed':all(x['exitCode']==0 and x.get('callerTypesExact') for x in results),'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'prototypeWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
