from pathlib import Path
import subprocess,json,hashlib,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-var-hoisting';base='51e3b3509f7a6c7208eda5374aa3e70900d5abb0'
run=lambda *args,cwd=w:subprocess.check_output(args,cwd=cwd)
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
source=run('git','rev-parse','HEAD').decode().strip();assert source=='3d6983eb7c7926c46865b5b5a5170d2138b6f424'
assert run('git','rev-parse','HEAD',cwd=root).decode().strip()==source
assert not run('git','status','--porcelain').strip() and not run('git','status','--porcelain',cwd=root).strip()
guard=read(raw/'guard-r3/result.json');review=read(raw/'review-r1/result.json');ret=read(raw/'retained-r3/result.json');meta=read(raw/'metadata/proof.json');repair=read(raw/'fixture-repair/proof.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,review,ret,read(raw/'metadata/result.json')])
native=read(raw/'review-r1/source-after.json');retained=read(raw/'retained-r3/source-after.json');tracker='docs/parity/native-reconciliation-2026-10-15.json'
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==(meta['trackerAfterSha256'] if n==tracker else h) for n,h in retained.items())
pg='native/tests/fixtures/postgres-catalog-response.jsonl'
for directory in [w,root]:assert len((directory/pg).read_bytes())==2779 and sha((directory/pg).read_bytes())=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
paths=sorted(set(native)|set(retained)|{'docs/STATUS.md','docs/REMAINING_WORK.md','docs/ARCHITECTURE.md','README.md','SECURITY.md'})
material=[]
for name in paths:
 a=(w/name).read_bytes();b=(root/name).read_bytes()
 if a==b:kind='raw'
 else:
  assert name!=pg and b'\0' not in a and b'\0' not in b,name
  a.decode('utf-8');b.decode('utf-8');assert a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n'),name
  kind='utf8-crlf-only'
 material.append({'path':name,'worktreeSha256':sha(a),'primarySha256':sha(b),'match':kind})
materialization={'sourceCommit':source,'paths':len(material),'raw':sum(x['match']=='raw' for x in material),'utf8CrlfOnly':sum(x['match']=='utf8-crlf-only' for x in material),'primaryRuntimeRerun':False,'protectedPostgresFixtureRawExact':True,'files':material}
(raw/'source-materialization.json').write_text(json.dumps(materialization,indent=2)+'\n',encoding='utf-8')
(raw/'guard-r1/preflight-note.json').write_text(json.dumps({'status':'runner_preflight_failed','cargoStarted':False,'reason':'Requested nonexistent compiler_move_repairs and compiler_move_missing integration target files; assertions stopped before Cargo.','sourceBeforeRetained':True,'runtimeValidationClaim':False},indent=2)+'\n',encoding='utf-8')
unchanged={}
for name in ['native/src/analysis.rs','native/src/model.rs','native/src/changes.rs','native/src/changes_plan.rs','native/src/agent_repl.rs','native/src/project_session.rs','native/src/compiler_inline_worker.cjs','Cargo.toml','Cargo.lock','native/Cargo.toml',pg]:
 a=(w/name).read_bytes();b=run('git','show',base+':'+name);assert a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n'),name;unchanged[name]={'currentSha256':sha(a),'baseGitSha256':sha(b),'match':'raw' if a==b else 'crlf-only'}
binary=root/'target/debug/yoke.exe';binary_hash=sha(binary.read_bytes())
log=(raw/'review-r1/live.log').read_text(encoding='utf-8');assert log.count('sha256='+binary_hash)==2
qualification={'sourceCommit':source,'baseIntegration':base,'guardPhase':'guard-r3','guardUnits':next(x['totals'] for x in guard['commands'] if x['name']=='units'),'guardIntegrations':next(x['totals'] for x in guard['commands'] if x['name']=='native'),'guardGroups':8,'reviewPhase':'review-r1','reviewUnits':next(x['totals'] for x in review['commands'] if x['name']=='native'),'liveSessionTests':next(x['totals'] for x in review['commands'] if x['name']=='live'),'reviewNativeInputs':review['inputs'],'retainedPhase':'retained-r3','retainedInputs':ret['frozenInputs'],'newIntegrationFunctions':2,'newUnitFunctions':3,'newVarIntegrationScenarios':21,'newOwnedJsUnitScenarios':11,'bodyCliGateScenarios':4,'newVarCliGateScenarios':2,'fixtureRepair':repair,'currentCliExecutableSha256':binary_hash,'guardCliExecutableSha256':repair['mainBinaryBeforeReviewSha256'],'binariesAreDifferentBuilds':binary_hash!=repair['mainBinaryBeforeReviewSha256'],'countsAreNonadditive':True,'metadataOnlyAfterFinalReview':meta,'unchangedSources':unchanged,'wholeNativeRerun':False,'fullWindowsCertified':False,'otherPlatformsCertified':False,'fullParityCertified':False,'mainMerged':False,'releasePerformed':False}
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
node=Path(shutil.which('node')).resolve();compiler=(w/'node_modules/typescript/lib/typescript.js').resolve()
(raw/'tools.json').write_text(json.dumps({'node':{'path':str(node),'sha256':sha(node.read_bytes()),'version':run(str(node),'--version').decode().strip()},'typescript':{'path':str(compiler),'sha256':sha(compiler.read_bytes()),'version':read(compiler.parent.parent/'package.json')['version']},'python':sys.version,'rustc':run(str(Path.home()/'.cargo/bin/rustc.exe'),'--version').decode().strip(),'liveSessionTools':read(raw/'review-r1/live-tools.json')},indent=2)+'\n',encoding='utf-8')
dest=w/'docs/validation/integration-2026-10-19/function-var-hoisting';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 r=read(p);phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':r.get('allChecksPassed'),'nativeAdmission':r.get('nativeAdmission'),'sourceUnchanged':r.get('sourceUnchanged'),'inputs':r.get('inputs',r.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-classic-body-function-var-hoisting','sourceCommit':source,'baseIntegration':base,'scope':['Independent original-byte function-owned var hoisting and initializer reconstruction','Parameter/default environments, duplicate/Unicode/loop/catch storage and assignment correspondence','Exact declaration/type/value/property witnesses without rewriting raw compiler observations','Owned runtime/CLI/native contracts; broader STATUS/R1-R9 remains active'],'guardPhase':'guard-r3','reviewPhase':'review-r1','retainedPhase':'retained-r3','countsAreNonadditive':True,'phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
readme=f'''# Function-owned var hoisting evidence

Source `{source}` extends the optional `inlineCompiler` body action.
Native source reconstruction preserves original-position initializers, function
storage shared by loops, parameter-default environments, duplicate/escaped names,
destructuring, nested scopes and distinct catch assignment targets.

| Frozen scope | Actual result |
| --- | --- |
| guard-r3 on 807 unchanged native inputs | Seven body units; 109 integration passes / zero failures / two existing ignored, across eight targets |
| review-r1 on 807 unchanged inputs | Seven body units; two separately selected actual live session tests, zero failures or ignored |
| Final format/build/all-target Clippy with warnings denied | Passed |
| retained-r3 on 801 unchanged inputs | Python17, typecheck, TypeScript260 passed / three existing skips |
| Metadata after final review | Python17; all807 native and800 other retained inputs exact; only three allowed R1 tracker fields changed |

The scopes overlap and are not added. Between guard and review, two unintentionally
recoded test comment literals were restored to their original UTF-8 spelling,
with original array formatting. The production prefix and other806 guard inputs
are exact. `fixture-repair/proof.json` checks the complete expected text; the
initial whitespace-only proof failure from rustfmt removing an optional trailing
comma is preserved. The CLI builds have different recorded hashes, and are not
claimed identical. No source or test code changed after the final review freeze.

Two added integration functions cover21 var scenarios. Three added unit functions
exercise original pattern-role boundaries, decoded names, budgets and coherent
forged type/assignment/parameter/property witnesses. An existing native frame unit
adds11 owned JavaScript scenarios. Four body CLI gates include two new var cases
and independent tsc, approval/application and foreign-preserving recovery.
The separate live Session cases execute8 TS/JS fixture scenarios using explicitly
selected TypeScript5.8.3/language-server5.1.3, with compiler-only verification and
the existing independent gates. Preparation executes no analyzed source; runtime
comparisons execute only owned literal fixtures.

Original compiler observations remain raw. Only comparison IDs for independently
derived same-function var groups are canonicalized. Initializer resolution is a
separate original checker witness; a simple catch parameter can own the assignment
without owning the function hoist. Assignment-pattern property syntax may target
only its own exact checked source key. Arbitrary compiler type expressions are
not inserted; original annotations and fixed primitive hints retain admitted
types. Other inferred forms must pass projected diagnostics or visibly refuse.

Failed development phases remain indexed: actual loop-closure typing failures,
native destructuring-property correspondence, an invalid original TypeScript
function/var fixture, the test Debug compilation error and the runner preflight
that started no Cargo. Worker prototypes use synthetic snapshots and explicitly
claim no native admission or analyzed execution. Existing eval/finally protection
and variable-inline/move/session regression cases remain in the broad pass.

The protected PostgreSQL fixture is2779 bytes and raw exact in both checkouts.
Primary materialization checks{len(material)} paths: {materialization['raw']} raw
matches and{materialization['utf8CrlfOnly']} UTF-8 CRLF-only matches. No primary
checkout runtime rerun or complete native/Windows/current-head hosted/upstream
acceptance follows. Wider inferred types, expression/invocation/declaration/
language/consumer forms and full STATUS/R1-R9 work remain active. Existing OS1314,
macOS/runtime and production App qualifications remain separate. Main is unmerged
and no release was performed.

The [index](index.json) binds{len(files)} raw artifacts. The complete raw runner,
sources, requests, logs, hashes, metadata and materialization records are under
`runs/`; immutable earlier checkpoints remain in their original directories.
'''
(dest/'README.md').write_text(readme,encoding='utf-8',newline='\n')
parent=dest.parent/'README.md';original=parent.read_bytes();nl=b'\r\n' if b'\r\n' in original else b'\n';at=original.index(nl+nl)+len(nl+nl)
entry=('Function-owned var hoisting now has [separate evidence](function-var-hoisting/README.md):\noriginal-byte reconstruction, parameter/catch/loop bindings and type witnesses;\n109 scoped integration passes, seven units and two separately selected live\nsession passes. Counts overlap; full STATUS/Windows acceptance remains open.\n\n').replace('\n',nl.decode()).encode('utf-8')
parent.write_bytes(original[:at]+entry+original[at:]);assert parent.read_bytes().replace(entry,b'',1)==original
print(json.dumps({'sourceCommit':source,'artifacts':len(files),'indexBytes':(dest/'index.json').stat().st_size,'indexSha256':sha((dest/'index.json').read_bytes()),'materialization':{k:v for k,v in materialization.items() if k!='files'}}))
