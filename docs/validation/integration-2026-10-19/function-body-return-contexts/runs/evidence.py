from pathlib import Path
import subprocess,json,hashlib,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-body-return-contexts';base='c834b5077f8ef9abc40f7d56da78e248f652ab25';source=sys.argv[1]
run=lambda *args,cwd=w:subprocess.check_output(args,cwd=cwd)
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
for directory in [w,root]:
 assert run('git','rev-parse','HEAD',cwd=directory).decode().strip()==source
 assert not run('git','status','--porcelain',cwd=directory).strip()
guard=read(raw/'guard-r2/result.json');ret=read(raw/'retained-r6/result.json');meta=read(raw/'metadata/proof.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret,read(raw/'metadata/result.json')])
native=read(raw/'guard-r2/source-after.json');retained=read(raw/'retained-r6/source-after.json');tracker='docs/parity/native-reconciliation-2026-10-15.json'
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
cli_check=read(raw/'native-green-r2/result.json')
assert cli_check['cliSha256']==binary_hash and cli_check['exitCode']==0 and cli_check['sourceUnchanged'] and not cli_check['analyzedCodeExecuted']
assert cli_check['response']['results'][0]['sourceExecuted'] is False
assert cli_check['response']['results'][0]['actions'][0]['kind']=='native.inline.function.body'

qualification={'sourceCommit':source,'baseIntegration':base,'guardPhase':'guard-r2','guardUnits':next(x['totals'] for x in guard['commands'] if x['name']=='units'),'guardIntegrations':next(x['totals'] for x in guard['commands'] if x['name']=='native'),'guardGroups':8,'nativeInputs':guard['inputs'],'retainedPhase':'retained-r6','retainedInputs':ret['frozenInputs'],'newIntegrationFunctions':2,'newUnitFunctions':1,'nativePositiveScenarios':31,'consumerTypeShadowRefusalScenarios':1,'bodyCliGateScenarios':12,'newCliGateScenarios':3,'currentCliExecutableSha256':binary_hash,'countsAreNonadditive':True,'metadataOnlyAfterFinalChecks':meta,'unchangedSources':unchanged,'wholeNativeRerun':False,'fullWindowsCertified':False,'otherPlatformsCertified':False,'fullParityCertified':False,'mainMerged':False,'releasePerformed':False}
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
node=Path(shutil.which('node')).resolve();compiler=(w/'node_modules/typescript/lib/typescript.js').resolve()
(raw/'tools.json').write_text(json.dumps({'node':{'path':str(node),'sha256':sha(node.read_bytes()),'version':run(str(node),'--version').decode().strip()},'typescript':{'path':str(compiler),'sha256':sha(compiler.read_bytes()),'version':read(compiler.parent.parent/'package.json')['version']},'python':sys.version,'rustc':run(str(Path.home()/'.cargo/bin/rustc.exe'),'--version').decode().strip()},indent=2)+'\n',encoding='utf-8')
dest=w/'docs/validation/integration-2026-10-19/function-body-return-contexts';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 r=read(p);phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':r.get('allChecksPassed'),'nativeAdmission':r.get('nativeAdmission'),'sourceUnchanged':r.get('sourceUnchanged'),'inputs':r.get('inputs',r.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-classic-body-return-contexts-runtime-names','sourceCommit':source,'baseIntegration':base,'scope':['Original source return annotations retain contextual result assignment types','Fixed void-comma barriers preserve anonymous returned function/class runtime names','Native source/binding/type and owned runtime/CLI gates; full STATUS/R1-R9 remains active'],'guardPhase':'guard-r2','retainedPhase':'retained-r6','countsAreNonadditive':True,'phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
readme=f'''# Return contextual types and anonymous runtime names

Source `{source}` extends the optional `inlineCompiler` classic-body action.
Original TypeScript return annotations also type the generated result storage,
with a fixed undefined alternative for normal completion. Function-valued return
properties, methods, tuples and callbacks retain contextual parameter types.
Anonymous returned function/class expressions use a fixed void-comma barrier,
preserving empty
runtime names for arrows, function expressions and classes. Named
expressions retain their names. Evaluation order, nested returns, finally
completion and source bytes retain the preceding body contract.

| Frozen scope | Actual result |
| --- | --- |
| guard-r2, 809 unchanged native inputs | Twelve body units; 115 integrations passed / zero failed / two existing ignored, eight targets |
| Format/build/all-target Clippy with warnings denied | Passed |
| retained-r6, 803 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final checks | Python 17; all 809 native and 802 other retained inputs exact; only three R1 fields changed |

Two added integration functions cover thirty-one owned positive scenarios and one
explicit consumer type-shadow refusal. They include object properties, methods,
tuples, source aliases, contextual arrows/functions, anonymous TS/JS functions
and classes, named functions, finally override/cancellation, Unicode/CRLF, retained
exports, calls before declarations, four never-typed call forms and seven erased-
wrapper/JavaScript cases. One new unit checks nineteen source classifications and
expired deadlines. Native original-byte/binding checks admit
every copied annotation, with exact surviving caller type strings. A consumer
scope changing a return type binding refuses before plan/derived publication.

Twelve existing body CLI scenarios include three additional strict TS/JS gates:
independent tsc, verification, approval, application and foreign-preserving recovery.
Extended actual-host admission tests reject coherent forged return annotations
and removed name barriers despite matching forged result hashes. Raw compiler
observations are neither normalized nor relaxed. No new wire fields or blanket
identifier exceptions are introduced. Runtime comparisons execute owned literal
fixtures only; preparation executes no analyzed source.

The original owned CLI refused the contextual object callback after projection.
The sixteen-case worker red phase finds eight projected compiler errors (TS7006)
and seven actual runtime name changes, despite exact reported result type strings.
One baseline case already passes. The same sixteen green probes have exact raw
result types and owned runtime observations. The worker snapshot only adds diagnostic
detail; these synthetic requests claim no native admission. The final Rust/CLI
regression supplies independent native admission and ordinary application gates.
The standalone current CLI also admits the exact original red fixture without
changing its source. Its executable SHA-256 matches the final build.

The initial broad guard records 114 integration passes and one TS7027 failure
with eleven passing units. Wrapping ordinary never calls in a comma caused the
generated break to appear unreachable. A return-type assertion prototype still
fails on all four never variants. The corrected source derives anonymous-value
classification through erased wrappers and leaves other expressions intact. The
final thirty-one-case worker probe retains exact types and owned runtime values.
These development runs are distinct from the final source freeze.

Counts overlap and are not added. Earlier evidence remains immutable. Primary
materialization checks {len(material)} paths: {materialization['raw']} raw and
{materialization['utf8CrlfOnly']} UTF-8 CRLF-only matches. The protected PostgreSQL
fixture is 2779 bytes and byte exact in both checkouts. No primary runtime rerun,
full native/Windows/current-head hosted/upstream acceptance, main merge or release
is claimed. Predicates, argument/parameter/annotation contexts, broader expression/
invocation/declaration/language/consumer semantics and other STATUS/R1-R9 work
remain open. Existing platform/deployment qualifications remain separate.

The [index](index.json) binds {len(files)} raw artifacts including original red,
green, current-source regression, retained tests, metadata and source hashes.
'''
(dest/'README.md').write_text(readme,encoding='utf-8',newline='\n')
parent=dest.parent/'README.md';original=parent.read_bytes();nl=b'\r\n' if b'\r\n' in original else b'\n';at=original.index(nl+nl)+len(nl+nl)
entry=('Return contextual types and anonymous runtime names have [separate evidence](function-body-return-contexts/README.md):\noriginal source annotations and fixed name barriers; 115 scoped integration\npasses and twelve units. Counts overlap; full STATUS/Windows acceptance remains open.\n\n').replace('\n',nl.decode()).encode('utf-8')
parent.write_bytes(original[:at]+entry+original[at:]);assert parent.read_bytes().replace(entry,b'',1)==original
print(json.dumps({'sourceCommit':source,'artifacts':len(files),'indexBytes':(dest/'index.json').stat().st_size,'indexSha256':sha((dest/'index.json').read_bytes()),'materialization':{k:v for k,v in materialization.items() if k!='files'}}))
