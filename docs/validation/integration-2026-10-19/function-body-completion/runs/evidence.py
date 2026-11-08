from pathlib import Path
import subprocess,json,hashlib,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-body-completion';base='06347aa272ba76045f10431c3c5af87e869b882b';source=sys.argv[1]
run=lambda *args,cwd=w:subprocess.check_output(args,cwd=cwd)
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
for directory in [w,root]:
 assert run('git','rev-parse','HEAD',cwd=directory).decode().strip()==source
 assert not run('git','status','--porcelain',cwd=directory).strip()
guard=read(raw/'guard-r4/result.json');ret=read(raw/'retained-r5/result.json');meta=read(raw/'metadata/proof.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret,read(raw/'metadata/result.json')])
native=read(raw/'guard-r4/source-after.json');retained=read(raw/'retained-r5/source-after.json');tracker='docs/parity/native-reconciliation-2026-10-15.json'
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
cli_check=read(raw/'doc-native-green-r2/result.json')
assert cli_check['cliSha256']==binary_hash and cli_check['exitCode']==0 and cli_check['sourceUnchanged'] and not cli_check['analyzedCodeExecuted']
assert cli_check['response']['results'][0]['sourceExecuted'] is False
assert cli_check['response']['results'][0]['actions'][0]['kind']=='native.inline.function.body'

qualification={'sourceCommit':source,'baseIntegration':base,'guardPhase':'guard-r4','guardUnits':next(x['totals'] for x in guard['commands'] if x['name']=='units'),'guardIntegrations':next(x['totals'] for x in guard['commands'] if x['name']=='native'),'guardGroups':8,'nativeInputs':guard['inputs'],'retainedPhase':'retained-r5','retainedInputs':ret['frozenInputs'],'newIntegrationFunctions':2,'newUnitFunctions':2,'strictCompletionIntegrationScenarios':26,'genericDocumentationPositiveScenarios':13,'documentationShadowRefusalScenarios':1,'bodyCliGateScenarios':9,'newStrictCliGateScenarios':4,'currentCliExecutableSha256':binary_hash,'countsAreNonadditive':True,'metadataOnlyAfterFinalChecks':meta,'unchangedSources':unchanged,'wholeNativeRerun':False,'fullWindowsCertified':False,'otherPlatformsCertified':False,'fullParityCertified':False,'mainMerged':False,'releasePerformed':False}
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
node=Path(shutil.which('node')).resolve();compiler=(w/'node_modules/typescript/lib/typescript.js').resolve()
(raw/'tools.json').write_text(json.dumps({'node':{'path':str(node),'sha256':sha(node.read_bytes()),'version':run(str(node),'--version').decode().strip()},'typescript':{'path':str(compiler),'sha256':sha(compiler.read_bytes()),'version':read(compiler.parent.parent/'package.json')['version']},'python':sys.version,'rustc':run(str(Path.home()/'.cargo/bin/rustc.exe'),'--version').decode().strip()},indent=2)+'\n',encoding='utf-8')
dest=w/'docs/validation/integration-2026-10-19/function-body-completion';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 r=read(p);phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':r.get('allChecksPassed'),'nativeAdmission':r.get('nativeAdmission'),'sourceUnchanged':r.get('sourceUnchanged'),'inputs':r.get('inputs',r.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-classic-body-completion-call-types','sourceCommit':source,'baseIntegration':base,'scope':['Independent original-byte body completion and called-result type carriers','Runtime-true original-body guard; false-guarded source clone/call with evaluated args only','JSDoc template/free-binding/typedef/neighbor ownership and exact surviving caller types','Owned runtime/CLI/native contracts; broader STATUS/R1-R9 remains active'],'guardPhase':'guard-r4','retainedPhase':'retained-r5','countsAreNonadditive':True,'phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
readme=f'''# Body completion and call-specific inferred result types

Source `{source}` extends the optional `inlineCompiler` classic-body action.
The original body block runs under fixed `void 0 === void 0`; normal completion
clears a cancelled return outside it, and only actual outer returns need a label.
Enabled unreachable-code/unused-label diagnostics remain active. Original/fixed
result assertions use a fixed intermediate unknown assertion for generated
undefined paths, including throwing and never-typed calls that actually return.

Other inferred results use fixed typeof syntax over a fresh value carrier. Its
initializer calls an original-parameter/body function carrier with already
evaluated fresh argument temporaries. Both initializers and the read remain under
`void 0 !== void 0`; neither function clone nor call executes. The var bindings
remain function/module scoped and initially undefined. JSDoc template/literal
instantiation, async/generator caller arguments and stateful argument order retain
checked types without reevaluating source expressions. Const/contextual literal results use a fixed readonly wrapper and indexed typeof;
ordinary contexts retain widening inference. Module declarations stay at their
original site; bounded original function-tag pieces preserve aliases when copied.
Generated const type tokens require exact parsed assertion positions. No arbitrary
compiler type text, global ReturnType helper or conditional type reconstruction
is inserted.

| Frozen scope | Actual result |
| --- | --- |
| guard-r4, 809 unchanged native inputs | Eleven body units; 113 integration passes / zero failures / two existing ignored, across eight targets |
| Final format/build/all-target Clippy with warnings denied | Passed |
| retained-r5, 803 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final checks | Python 17; all 809 native and 802 other retained inputs exact; only three allowed R1 tracker fields changed |

Scopes overlap and are not added. Two added integration functions cover 26 strict
completion/type cases, thirteen generic/documentation positives and one explicit local
type-shadow refusal. Two new unit functions cover leading/trailing comments, BOM,
Unicode/newlines, exact source copies, deadlines and bounds. Existing actual-host
admission tests now reject forged typeof queries, carrier argument maps, removed
documentation neutralization, guards, names and source/result hashes. Nine body
CLI cases include four new strict TS/JS cases, independent tsc, approval/application
and foreign-preserving recovery. Async runtime comparison awaits only owned actual
Promise observations with a one-second bound; runtime comparisons execute only
owned literal fixtures. Preparation executes no analyzed source.

Removing a documented definition emits a fixed empty statement at its original
declaration span, keeping comment/typedef bytes while preventing silent annotation
of neighboring functions. Native source-derived ownership excludes trailing docs,
including line breaks inside comments, and handles BOM explicitly. The compiler
checks type/value free bindings in JSDoc type expressions/template defaults at the
consumer scope; owned documentation declarations retain their copied scope and
fixed import-type qualifiers retain module meaning. Native original-byte frames,
fresh names, every reported copied binding and surviving caller types remain
checked. Raw observations and original graph/entity identities are not rewritten.

Development evidence is distinct and nonadditive. Initial probes pinpoint TS7027
at the synthetic reset and TS7028 at unused generated labels. An initial guard
prototype still produced TS2352 on never/undefined paths. Feature-r1 has 11 passes
and one invalid-original JSDoc fixture, corroborated as TS7006. Fixtures-r2 finds
one additional invalid JS-catch original (TS18046), corrected with instanceof;
fixtures-r3 selects that correction. Guard-r1 has nine unit passes and one actual
BOM ownership failure, subsequently fixed. Guard-r2 passes ten units and 112
integrations on its prior source freeze, before the final called-result design.
It is not a current-source result. Guard-r3 has 112 passes and one literal
widening failure (3 to number), with ten passing units. Initial always-boxed and
contextual-boxed probes still lose a typedef alias; the final original-tag piece
plan fixes the alias while retaining exact raw before/after type text.

The published-vs-current worker comparison then exposed loss of JSDoc generic
instantiation in the intermediate conditional-result design. The final call-specific
carrier supersedes it. Prototype async failure also localized orphaned documentation
reattachment; keep-definition probes isolate it and fixed empty statements preserve
neighbor/typedef types. Eighteen parse-only ownership cases and an actual owned CLI
red are retained. An initial inline local-typedef counterexample did not actually
shadow; the corrected newline-separated probe records original/consumer declaration
positions and the intended refusal. A runner preflight stops before any worker.
Synthetic probes explicitly claim no native admission or analyzed execution.
Earlier pass/fail counts are not added to final totals. The two ignored Session
tests were not selected in this run; earlier live results remain in their archive.

Primary materialization checks {len(material)} paths: {materialization['raw']} raw
matches and {materialization['utf8CrlfOnly']} UTF-8 CRLF-only matches. The protected
PostgreSQL fixture is 2779 bytes and raw exact in both checkouts. No primary runtime
rerun, complete native/Windows/current-head hosted/upstream acceptance, main merge
or release is claimed. Further return/predicate/parameter/annotation contexts,
diagnostic combinations and wider expression/invocation/declaration/language/
consumer forms remain open, alongside STATUS/R1-R9. Existing OS1314/macOS/runtime
and production App qualifications remain separate.

The [index](index.json) binds {len(files)} raw artifacts, preserving failed and
passing phases, workers, requests, outputs, available runners, logs, metadata and
materialization records. Earlier evidence is immutable.
'''
(dest/'README.md').write_text(readme,encoding='utf-8',newline='\n')
parent=dest.parent/'README.md';original=parent.read_bytes();nl=b'\r\n' if b'\r\n' in original else b'\n';at=original.index(nl+nl)+len(nl+nl)
entry=('Body completion and call-specific result types have [separate evidence](function-body-completion/README.md):\nsource-derived carriers, guarded control flow and JSDoc ownership; 113 scoped\nintegration passes and eleven units. Counts overlap; full STATUS/Windows acceptance\nremains open.\n\n').replace('\n',nl.decode()).encode('utf-8')
parent.write_bytes(original[:at]+entry+original[at:]);assert parent.read_bytes().replace(entry,b'',1)==original
print(json.dumps({'sourceCommit':source,'artifacts':len(files),'indexBytes':(dest/'index.json').stat().st_size,'indexSha256':sha((dest/'index.json').read_bytes()),'materialization':{k:v for k,v in materialization.items() if k!='files'}}))
