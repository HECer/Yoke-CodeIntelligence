from pathlib import Path
import subprocess,json,hashlib,shutil,re,sys
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
root=work.parent/'Yoke-CodeIntelligence'
raw=root/'validation/integration-20261018/compiler-inline'
sha=lambda b:hashlib.sha256(b).hexdigest()
base='83614e989a299eaec5e3809d9105c7dff4fee78a'
protected='native/tests/fixtures/postgres-catalog-response.jsonl'
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def run(*args,cwd=work):return subprocess.check_output(args,cwd=cwd)
if sys.argv[1]=='materialize':
 native=read(raw/'final-r2/source-after.json');retained=read(raw/'retained-r1/source-after.json')
 names=sorted(set(native)|set(retained));entries=[]
 for name in names:
  a=(work/name).read_bytes();b=(root/name).read_bytes()
  mode='raw' if a==b else 'crlf-only' if a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n') else 'mismatch'
  assert mode!='mismatch',name
  entries.append({'path':name,'worktreeSha256':sha(a),'integrationSha256':sha(b),'match':mode})
 pg=(root/protected).read_bytes();assert len(pg)==2779 and sha(pg)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
 proof={'sourceCommit':run('git','rev-parse','HEAD').decode().strip(),'integrationCommit':run('git','rev-parse','HEAD',cwd=root).decode().strip(),'inputs':len(entries),'rawMatches':sum(e['match']=='raw' for e in entries),'crlfOnly':sum(e['match']=='crlf-only' for e in entries),'runtimeRerunInIntegration':False,'protectedPostgres':{'bytes':len(pg),'sha256':sha(pg)},'files':entries}
 assert proof['sourceCommit']==proof['integrationCommit']
 (raw/'materialization.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in proof.items() if k!='files'}));sys.exit()
assert sys.argv[1]=='archive'
final=read(raw/'final-r2/result.json');retained=read(raw/'retained-r1/result.json')
assert final['allChecksPassed'] and final['sourceUnchanged'] and retained['allChecksPassed'] and retained['sourceUnchanged']
source_commit=run('git','rev-parse','HEAD').decode().strip()
before=read(raw/'final-r2/source-after.json')
assert all(sha((work/n).read_bytes())==s for n,s in before.items())
retained_inputs=read(raw/'retained-r1/source-after.json')
changed=[n for n,s in retained_inputs.items() if sha((work/n).read_bytes())!=s]
assert changed==['docs/parity/native-reconciliation-2026-10-15.json']
metadata_proof=read(raw/'metadata-key-order-proof.json')
assert sha((work/changed[0]).read_bytes())==metadata_proof['currentSha256']
assert json.loads((work/changed[0]).read_bytes())==read(raw/'tracker-before-key-order-only.json')
native_log=(raw/'final-r2/native.log').read_text(encoding='utf-8')
new_integration=re.findall(r'^fn ([a-zA-Z0-9_]+)\(', (work/'native/tests/compiler_inline.rs').read_text(encoding='utf-8'),re.M)
helpers={'d','compiler','fixture','args','planned','unpublished'}
new_integration=[n for n in new_integration if n not in helpers]
assert len(new_integration)==16 and all(f'test {n} ... ok' in native_log for n in new_integration)
unit=['compiler_inline::tests::copied_syntax_checks_preserve_precedence_statements_literals_and_operators','compiler_inline::tests::hostile_effects_cannot_widen_paths_ranges_references_or_removal']
assert all(f'test {n} ... ok' in native_log for n in unit)
unchanged={}
for name in ['native/src/analysis.rs','native/src/model.rs','native/src/lsp.rs','native/src/lsp_refactor_intents.rs','native/src/changes.rs','native/tests/projects.rs','native/tests/feedback_cli.rs','native/tests/refactor_intents.rs','native/tests/session_refactor.rs',protected]:
 current=(work/name).read_bytes();prior=run('git','show',base+':'+name)
 assert current.replace(b'\r\n',b'\n')==prior.replace(b'\r\n',b'\n'),name
 unchanged[name]={'currentRawSha256':sha(current),'baseGitSha256':sha(prior),'match':'raw' if current==prior else 'crlf-only'}
qualification={'baseIntegration':base,'sourceCommit':source_commit,'newIntegrationCases':new_integration,'newUnitCases':unit,'newCasesPassed':18,'newCasesFailed':0,'actualCompiler':'TypeScript 5.8.3','actualNode':'24.21.0','languageSiblingMatrixScenarios':12,'jsxComponentShadowingScenarios':2,'ownedNonTsLspFixtureSeparatelyLabeled':True,'nativeTotals':final['commands'][0]['totals'],'nativeGroups':final['commands'][0]['groups'],'nativeFrozenInputs':final['inputs'],'retainedFrozenInputs':retained['frozenInputs'],'wholeNativeRerun':False,'otherPlatformCertification':False,'fullParityCertified':False,'releasePerformed':False,'unchangedSources':unchanged}
qualification['postCheckDocumentationKeyOrderOnly']=True
qualification['allOtherRetainedInputsCurrentExact']=True
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8')
shutil.copyfile(__file__,raw/'evidence.py')
dest=work/'docs/validation/integration-2026-10-19/compiler-inline';dest.mkdir(parents=True,exist_ok=False)
shutil.copytree(raw,dest/'runs')
files=[]
for p in sorted((dest/'runs').rglob('*')):
 if p.is_file(): files.append({'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())})
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 d=read(p);assert d['sourceUnchanged'];phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':d['allChecksPassed'],'inputs':d.get('inputs',d.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-immutable-compiler-inline','sourceCommit':source_commit,'baseIntegration':base,'scope':['Explicit host TS5.8.3 function-valued variable inlining on immutable captured files','Independent initializer/reference/removal/result admission before guarded plan','Existing session/CLI/facade and independent tsc/apply/recovery gates','Scoped Windows development evidence; no universal/whole-native/other-platform/release certification'],'finalPhase':'final-r2','retainedPhase':'retained-r1','phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(files),'indexSha256':sha((dest/'index.json').read_bytes()),'qualification':{k:v for k,v in qualification.items() if k not in ['unchangedSources','newIntegrationCases','newUnitCases']}}))
