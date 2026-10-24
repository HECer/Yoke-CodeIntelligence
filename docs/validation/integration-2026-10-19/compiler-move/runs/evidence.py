from pathlib import Path
import subprocess,json,hashlib,shutil,sys,re
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/compiler-move'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
def run(*args,cwd=work):return subprocess.check_output(args,cwd=cwd)
source_commit=run('git','rev-parse','HEAD').decode().strip();native=read(raw/'final-r2/source-after.json');retained=read(raw/'retained-r1/source-after.json')
pg='native/tests/fixtures/postgres-catalog-response.jsonl'
if sys.argv[1]=='materialize':
 entries=[]
 for name in sorted(set(native)|set(retained)):
  a=(work/name).read_bytes();b=(root/name).read_bytes();mode='raw' if a==b else 'crlf-only' if a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n') else 'mismatch';assert mode!='mismatch',name;entries.append({'path':name,'worktreeSha256':sha(a),'integrationSha256':sha(b),'match':mode})
 assert run('git','rev-parse','HEAD',cwd=root).decode().strip()==source_commit
 proof={'sourceCommit':source_commit,'integrationCommit':source_commit,'inputs':len(entries),'rawMatches':sum(e['match']=='raw' for e in entries),'crlfOnly':sum(e['match']=='crlf-only' for e in entries),'runtimeRerunInIntegration':False,'files':entries}
 (raw/'materialization.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in proof.items() if k!='files'}));sys.exit()
assert sys.argv[1]=='archive'
for name,h in {**native,**retained}.items():assert sha((work/name).read_bytes())==h,name
checks=read(raw/'combined-checks.json');ret=read(raw/'retained-r1/result.json');assert ret['allChecksPassed'] and ret['sourceUnchanged'];meta=read(raw/'metadata-proof.json');assert sha((work/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())==meta['trackerSha256']
assert len((work/pg).read_bytes())==2779 and sha((work/pg).read_bytes())=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
base='d36a6d0459e0b38352a04be30ce1f284e63ac768';unchanged={}
for name in ['native/src/analysis.rs','native/src/model.rs','native/src/lsp.rs','native/src/lsp_refactor_intents.rs','native/src/semantic_move_intents.rs','native/src/semantic_refactor.rs','native/src/changes.rs','native/src/agent_repl.rs','native/tests/session_refactor.rs','native/tests/compiler_inline.rs',pg]:
 a=(work/name).read_bytes();b=run('git','show',base+':'+name);assert a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n'),name;unchanged[name]={'currentSha256':sha(a),'baseGitSha256':sha(b),'match':'raw' if a==b else 'crlf-only'}
log=(raw/'final-r2/native.log').read_text(encoding='utf-8');new=re.findall(r'^fn (\w+)\(', (work/'native/tests/compiler_move.rs').read_text(encoding='utf-8'),re.M);helpers={'d','compiler','fixture','args','planned','source_unchanged','unpublished'};new=[n for n in new if n not in helpers];assert len(new)==13 and all(f'test {name} ... ok' in log for name in new)
units=re.findall(r'^test (compiler_move::tests::\S+) \.\.\. ok$',(raw/'final-r2/native.log').read_text(encoding='utf-8'),re.M);assert len(units)==3
qualification={'baseIntegration':base,'sourceCommit':source_commit,'newIntegrationCases':new,'newUnitCases':units,'newCasesPassed':16,'additionalStdlibCliProbePassed':True,'compiler':'TypeScript 5.8.3','declarationEpochScenarios':14,'encodingLanguageScenarios':4,'jsxComponentScenarios':2,'combinedNativeTotals':checks['combinedNonoverlappingTotals'],'combinedGroups':checks['combinedGroups'],'nativeFrozenInputs':checks['nativeInputs'],'retainedFrozenInputs':ret['frozenInputs'],'libraryRuntimeInputHashesMatched':checks['libraryRuntimeInputHashesMatched'],'wholeNativeRerun':False,'otherPlatformsCertified':False,'fullParityCertified':False,'releasePerformed':False,'unchangedSources':unchanged}
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
probe=Path(r'C:\Users\Lomi\AppData\Local\YokeValidation\typescript-5.8.3-lsp-5.1.3\node_modules\typescript\lib\typescript.js');(raw/'exploratory-probe-compiler.json').write_text(json.dumps({'path':str(probe),'bytes':probe.stat().st_size,'sha256':sha(probe.read_bytes()),'probeIsExploratoryNotProductionWorkerProof':True},indent=2)+'\n',encoding='utf-8')
dest=work/'docs/validation/integration-2026-10-19/compiler-move';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 result=read(p);assert result['sourceUnchanged'];phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':result['allChecksPassed'],'inputs':result.get('inputs',result.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-immutable-compiler-move','sourceCommit':source_commit,'baseIntegration':base,'scope':['Actual explicitly host-selected TS5.8.3 Move to existing captured ES module','Native original-byte/syntax/static-binding/import-export/result admission','Explicit AST provenance without invented graph identity','Existing verify/approval/apply/conditional recovery gates','Scoped Windows development evidence; full STATUS remains incomplete'],'finalPhase':'final-r2','libraryPhase':'final-r2','retainedPhase':'retained-r1','phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
summary={k:v for k,v in qualification.items() if k not in ['unchangedSources','newIntegrationCases','newUnitCases']};print(json.dumps({'artifacts':len(files),'indexSha256':sha((dest/'index.json').read_bytes()),'qualification':summary}))
