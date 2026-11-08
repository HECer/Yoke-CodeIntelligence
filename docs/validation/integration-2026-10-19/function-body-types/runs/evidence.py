from pathlib import Path
import subprocess,json,hashlib,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-body-types';base='7f51e6cc2b0ad7d7e7378f9345b2457b987008a1';source='3f8fe565fd4fc88d9ca8266bbf605e698688e4a8'
run=lambda *args,cwd=w:subprocess.check_output(args,cwd=cwd)
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
for directory in [w,root]:
 assert run('git','rev-parse','HEAD',cwd=directory).decode().strip()==source
 assert not run('git','status','--porcelain',cwd=directory).strip()
guard=read(raw/'guard-r1/result.json');ret=read(raw/'retained-r1/result.json');meta=read(raw/'metadata/proof.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret,read(raw/'metadata/result.json')])
native=read(raw/'guard-r1/source-after.json');retained=read(raw/'retained-r1/source-after.json');tracker='docs/parity/native-reconciliation-2026-10-15.json'
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
unchanged={}
for name in ['native/src/analysis.rs','native/src/model.rs','native/src/changes.rs','native/src/changes_plan.rs','native/src/agent_repl.rs','native/src/project_session.rs','native/src/compiler_inline_worker.cjs','Cargo.toml','Cargo.lock','native/Cargo.toml',pg]:
 a=(w/name).read_bytes();b=run('git','show',base+':'+name);assert a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n'),name;unchanged[name]={'currentSha256':sha(a),'baseGitSha256':sha(b),'match':'raw' if a==b else 'crlf-only'}
binary=root/'target/debug/yoke.exe';binary_hash=sha(binary.read_bytes())
qualification={'sourceCommit':source,'baseIntegration':base,'guardPhase':'guard-r1','guardUnits':next(x['totals'] for x in guard['commands'] if x['name']=='units'),'guardIntegrations':next(x['totals'] for x in guard['commands'] if x['name']=='native'),'guardGroups':8,'nativeInputs':guard['inputs'],'retainedPhase':'retained-r1','retainedInputs':ret['frozenInputs'],'newIntegrationFunctions':2,'newUnitFunctions':2,'inferredIntegrationScenarios':22,'strictDiagnosticIntegrationScenarios':2,'bodyCliGateScenarios':5,'newInferredCliGateScenarios':1,'currentCliExecutableSha256':binary_hash,'countsAreNonadditive':True,'metadataOnlyAfterFinalChecks':meta,'unchangedSources':unchanged,'wholeNativeRerun':False,'fullWindowsCertified':False,'otherPlatformsCertified':False,'fullParityCertified':False,'mainMerged':False,'releasePerformed':False,'remainingObservedRefusal':'Returning-body owned worker fixture with allowUnreachableCode:false refuses under both literal and binary guard. Precise diagnostic cause not localized.'}
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
node=Path(shutil.which('node')).resolve();compiler=(w/'node_modules/typescript/lib/typescript.js').resolve()
(raw/'tools.json').write_text(json.dumps({'node':{'path':str(node),'sha256':sha(node.read_bytes()),'version':run(str(node),'--version').decode().strip()},'typescript':{'path':str(compiler),'sha256':sha(compiler.read_bytes()),'version':read(compiler.parent.parent/'package.json')['version']},'python':sys.version,'rustc':run(str(Path.home()/'.cargo/bin/rustc.exe'),'--version').decode().strip()},indent=2)+'\n',encoding='utf-8')
dest=w/'docs/validation/integration-2026-10-19/function-body-types';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 r=read(p);phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':r.get('allChecksPassed'),'nativeAdmission':r.get('nativeAdmission'),'sourceUnchanged':r.get('sourceUnchanged'),'inputs':r.get('inputs',r.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-classic-body-source-inferred-var-types','sourceCommit':source,'baseIntegration':base,'scope':['Independent original-byte type carriers from declarations and loop heads','Fixed pure-false guard; copied expressions and reads never execute','Destructured declaration/type/binding coverage and source-ordered control frames','Owned runtime/CLI/native contracts; broader STATUS/R1-R9 remains active'],'guardPhase':'guard-r1','retainedPhase':'retained-r1','countsAreNonadditive':True,'phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
readme=f'''# Source-derived inferred var types

Source `{source}` extends the optional `inlineCompiler` classic-body action.
Native reconstruction derives fresh type carriers from original declarations or
loop heads at their lexical sites. Fixed `typeof` syntax preserves admitted object,
array, readonly tuple, union and function storage types without inserting arbitrary
compiler type expressions. The fixed guard `void 0 !== void 0` keeps copied
initializers, iterable expressions and reads inert. The carrier var binding remains
function/module scoped; actual body storage is lexical and fresh per expansion.

| Frozen scope | Actual result |
| --- | --- |
| guard-r1, 809 unchanged native inputs | Nine body units; 111 integration passes / zero failures / two existing ignored, across eight targets |
| Final format/build/all-target Clippy with warnings denied | Passed |
| retained-r1, 803 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final checks | Python 17; all 809 native and 802 other retained inputs exact; only three allowed R1 tracker fields changed |

These scopes overlap and are not added. No source or test code changed after the
final guard freeze. The two ignored Session tests were not selected in this run;
their earlier actual live result remains at the preceding var-hoisting checkpoint.

Two added integration functions cover 22 inferred-storage scenarios and two cases
with `allowUnreachableCode:false`, `noUnusedLocals`, `noUnusedParameters` and
`noFallthroughCasesInSwitch` enabled. Coverage includes labels/continue, unbraced
controls, nested/adjacent insertions, actual once-only initializer/iterable calls,
destructuring/default/shorthand/prototype keys, JS checkJs, const assertions and
escaped UTF-8/CRLF input. Two new native unit functions and extended admission tests
cover malformed groups/names/deadlines, omitted destructured types, forged raw type
rows and a coherent forged true guard. Five actual body CLI gates include one new
inferred loop case, independent tsc, approval/application and foreign-preserving
recovery. Runtime comparisons execute only owned literal fixtures. Preparation
executes no analyzed source.

The final native and worker plans independently reconstruct exact source pieces,
quoted decoded property keys and ordered insertion/return frames. Copied member,
library and local bindings remain checked; raw compiler observations and graph IDs
are retained. Source sites, carrier names, label depth, bytes, edits, copies and
compiler observations share bounded budgets and refuse rather than truncate.

Development phases are distinct, nonadditive evidence. The initial worker probes
show actual implicit-any and const-token refusals; the worker green and owned VM
comparisons are synthetic-snapshot evidence without native admission. Feature-r1
has nine passes and one obsolete test assertion failure: a blanket ban on var also
rejected the intended cold carriers after actual native/runtime comparison passed.
Feature-r2 passes ten functions before the final strict-diagnostic additions.
Guard-options-r1 compares a literal false guard against the pure binary guard:
the nonreturning strict-option fixture passes only the latter; the returning-body
fixture still refuses under both. Its precise diagnostic cause is not localized.
The final guard passes all 11 body integration functions. Earlier compile/unit/
feature counts are not added to final totals. Targeted deletion of two verified
unused old build cache files is recorded separately; the CLI hash was unchanged.

Primary materialization checks {len(material)} paths: {materialization['raw']} raw
matches and {materialization['utf8CrlfOnly']} UTF-8 CRLF-only matches. The protected
PostgreSQL fixture is 2779 bytes and raw exact in both checkouts. No primary runtime
rerun, complete native/Windows/current-head hosted/upstream acceptance, main merge
or release is claimed. Inferred return/parameter/annotation contexts, other strict
diagnostic combinations and wider expression/invocation/declaration/language/
consumer forms remain open, alongside the remaining STATUS/R1-R9 work. Existing
OS1314/macOS/runtime and production App qualifications remain separate.

The [index](index.json) binds {len(files)} raw artifacts, including all passing and
failed development phases. Original runners, requests, logs, hashes, metadata and
materialization records are retained under `runs/`. Earlier evidence is immutable.
'''
(dest/'README.md').write_text(readme,encoding='utf-8',newline='\n')
parent=dest.parent/'README.md';original=parent.read_bytes();nl=b'\r\n' if b'\r\n' in original else b'\n';at=original.index(nl+nl)+len(nl+nl)
entry=('Source-derived inferred var types have [separate evidence](function-body-types/README.md):\noriginal-byte cold carriers, destructured type coverage and ordered control frames;\n111 scoped integration passes and nine body units. Counts overlap; full STATUS/\nWindows acceptance remains open.\n\n').replace('\n',nl.decode()).encode('utf-8')
parent.write_bytes(original[:at]+entry+original[at:]);assert parent.read_bytes().replace(entry,b'',1)==original
print(json.dumps({'sourceCommit':source,'artifacts':len(files),'indexBytes':(dest/'index.json').stat().st_size,'indexSha256':sha((dest/'index.json').read_bytes()),'materialization':{k:v for k,v in materialization.items() if k!='files'}}))
