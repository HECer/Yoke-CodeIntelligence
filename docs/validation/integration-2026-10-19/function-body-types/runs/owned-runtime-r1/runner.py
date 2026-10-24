from pathlib import Path
import ast,json,subprocess,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-types';prior=raw/'worker-green-r1';out=raw/'owned-runtime-r1';out.mkdir(exist_ok=False)
tree=ast.parse((prior/'runner.py').read_text());assignment=next(n for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='cases' for t in n.targets));owned=ast.literal_eval(assignment.value)
cases=[]
for i,source in enumerate(owned):
 request=json.loads((prior/f'request-{i}.json').read_text());assert request['contents']['main.ts']==source
 cases.append({'case':i,'before':source,'after':(prior/f'after-{i}.ts').read_text()})
(out/'cases.json').write_text(json.dumps(cases,indent=2)+'\n',encoding='utf-8')
script="""const fs=require('node:fs'),vm=require('node:vm'),ts=require(process.argv[2]);
const cases=JSON.parse(fs.readFileSync(process.argv[3],'utf8'));
function observe(s){const js=ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const context={exports:{}};vm.runInNewContext('(function(exports){'+js+'\\n})(exports);',context,{timeout:1000});return JSON.stringify(context.exports.observed);}
const results=cases.map(c=>{const before=observe(c.before),after=observe(c.after);if(before!==after)throw Error('owned fixture '+c.case+' changed: '+before+' => '+after);return {case:c.case,observation:before};});process.stdout.write(JSON.stringify(results));
"""
(out/'oracle.cjs').write_text(script,encoding='utf-8');shutil.copyfile(__file__,out/'runner.py')
command=['node',str(out/'oracle.cjs'),str((w/'node_modules/typescript/lib/typescript.js').resolve()),str(out/'cases.json')]
p=subprocess.run(command,cwd=out,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/'stdout.json').write_bytes(p.stdout);(out/'stderr.log').write_bytes(p.stderr)
result={'allChecksPassed':p.returncode==0,'exitCode':p.returncode,'ownedFixtureCount':len(cases),'nativeAdmission':False,'productionPreparationExecutedSource':False,'ownedFixtureExecution':True,'command':command};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result));assert p.returncode==0
