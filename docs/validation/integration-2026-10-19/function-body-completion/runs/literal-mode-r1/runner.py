from pathlib import Path
import json,subprocess,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-completion';out=raw/'literal-mode-r1';out.mkdir(exist_ok=False)
worker=(raw/'literal-box-r1/worker.cjs').read_bytes();old=b"const invocation=names.returnType[0]";assert worker.count(old)==1
worker=worker.replace(old,b"const unit=t=>!!t&&(!!(t.flags&(ts.TypeFlags.Literal|ts.TypeFlags.UniqueESSymbol))||t.isUnion()&&!t.aliasSymbol&&t.types.every(unit)),boxed=unit(checker.getTypeAtLocation(call))&&((ts.isVariableDeclaration(parent)&&!!(parent.parent.flags&ts.NodeFlags.Const))||unit(checker.getContextualType(call)));if(boxed)names.literalBox=true;const invocation=names.returnType[0]")
old=b"+';var '+names.returnType[1]+' = '+box+';void '";assert worker.count(old)==1;worker=worker.replace(old,b"+';var '+names.returnType[1]+' = '+(boxed?box:invocation)+';void '")
old=b"const type='(typeof '+names.returnType[1]+')[\"value\"]';";assert worker.count(old)==1;worker=worker.replace(old,b"const type=names.literalBox?'(typeof '+names.returnType[1]+')[\"value\"]':'typeof '+names.returnType[1];")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py');requests=[json.loads((raw/f'literal-box-r1/request-{i}.json').read_bytes()) for i in range(10)]
header='/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}'
for source in [header+'let result=helper(3);export const observed=result+1;',header+'export function outer(){return helper(3);}export const observed=outer();',header+'\n/** @returns {3} */\nexport function outer(){return helper(3);}export const observed=outer();']:
 r=json.loads((raw/'literal-box-r1/request-0.json').read_bytes());r['contents']['main.js']=source;r['intent'].update({'startByte':source.index('function helper'),'endByte':source.index('}',source.index('function helper'))+1,'nameStartByte':source.index('helper('),'nameEndByte':source.index('helper(')+6});requests.append(r)
results=[]
for i,r in enumerate(requests):
 r['sourceBindings']={p:hashlib.sha256(t.encode()).hexdigest() for p,t in r['contents'].items()};data=json.dumps(r).encode();(out/f'request-{i}.json').write_bytes(data);p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=out);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr);row={'case':i,'exitCode':p.returncode,'error':p.stderr.decode()};results.append(row)
 if p.returncode==0:
  reply=json.loads(p.stdout)['bodyInline'];before=[t['type'] for t in reply['before']['types'] if t['span']['text'] in ['result','outer']];after=[t['type'] for t in reply['after']['types'] if t['span']['text'] in ['result','outer']];row.update({'beforeTypes':before,'afterTypes':after,'callerTypesExact':before==after,'literalBox':reply['frames'][0]['names'].get('literalBox',False)})
result={'cases':results,'allChecksPassed':all(x['exitCode']==0 and x.get('callerTypesExact') for x in results),'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'prototypeWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
