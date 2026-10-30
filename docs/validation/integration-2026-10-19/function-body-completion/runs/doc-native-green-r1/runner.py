from pathlib import Path
import json,hashlib,subprocess,tempfile,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/function-body-completion'/sys.argv[1];out.mkdir(exist_ok=False)
fixture=Path(tempfile.mkdtemp(prefix='ci-body-completion-doc-owned-'));host=Path(tempfile.mkdtemp(prefix='ci-body-completion-doc-host-'))
source='export const marker=1;/** @returns {string|number} */ function helper(){return {value:3};}const result=helper();export const observed={marker,result};'
config={'compilerOptions':{'strict':True,'target':'ES2022','module':'ESNext','moduleResolution':'Bundler','allowJs':True,'checkJs':True,'noEmit':True,'allowUnreachableCode':False,'allowUnusedLabels':False},'files':['main.js']}
(fixture/'main.js').write_text(source,encoding='utf-8');(fixture/'tsconfig.json').write_text(json.dumps(config),encoding='utf-8')
profile={'inlineCompiler':{'node':str(Path(shutil.which('node')).resolve()),'typescript':str((w/'node_modules/typescript/lib/typescript.js').resolve()),'trusted':True}}
(host/'host.json').write_text(json.dumps(profile),encoding='utf-8');requests=[{'operation':'refactor.prepare','args':{'operation':'inline','arguments':{'relativePath':'main.js','namePath':'helper','keepDefinition':False}}}];(host/'requests.json').write_text(json.dumps(requests),encoding='utf-8')
exe=root/'target/debug/yoke.exe';command=[str(exe),'--root',str(fixture),'session','--id','completion-doc-owned','--host-profile',str(host/'host.json'),'--requests',str(host/'requests.json')]
p=subprocess.run(command,cwd=w,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(out/'stdout.json').write_bytes(p.stdout);(out/'stderr.log').write_bytes(p.stderr)
(out/'source.js').write_text(source,encoding='utf-8');(out/'config.json').write_text(json.dumps(config,indent=2)+'\n',encoding='utf-8');(out/'requests.json').write_text(json.dumps(requests,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py')
result={'command':command,'exitCode':p.returncode,'cliSha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'sourceUnchanged':(fixture/'main.js').read_text(encoding='utf-8')==source,'analyzedCodeExecuted':False,'ownedLiteralFixture':True,'response':json.loads(p.stdout) if p.stdout else None}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
