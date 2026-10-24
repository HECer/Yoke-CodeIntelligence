from pathlib import Path
import subprocess,json,hashlib,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-body-argument-contexts';base='1fcf8b828a411952b4487e814876d4d31a504e03';source=sys.argv[1]
run=lambda *args,cwd=w:subprocess.check_output(args,cwd=cwd)
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
for directory in [w,root]:
 assert run('git','rev-parse','HEAD',cwd=directory).decode().strip()==source
 assert not run('git','status','--porcelain',cwd=directory).strip()
guard=read(raw/'guard-r2/result.json');ret=read(raw/'retained-r2/result.json');meta=read(raw/'metadata/proof.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret,read(raw/'metadata/result.json')])
native=read(raw/'guard-r2/source-after.json');retained=read(raw/'retained-r2/source-after.json');tracker='docs/parity/native-reconciliation-2026-10-15.json'
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
cli_check=read(raw/'native-scalar-green-r2/result.json')
assert cli_check['cliSha256']==binary_hash and cli_check['exitCode']==0 and cli_check['sourceUnchanged'] and not cli_check['analyzedCodeExecuted']
assert cli_check['response']['results'][0]['sourceExecuted'] is False
assert cli_check['response']['results'][0]['actions'][0]['kind']=='native.inline.function.body'

qualification={'sourceCommit':source,'baseIntegration':base,'guardPhase':'guard-r2','guardUnits':next(x['totals'] for x in guard['commands'] if x['name']=='units'),'guardIntegrations':next(x['totals'] for x in guard['commands'] if x['name']=='native'),'guardGroups':8,'nativeInputs':guard['inputs'],'retainedPhase':'retained-r2','retainedInputs':ret['frozenInputs'],'newIntegrationFunctions':1,'newUnitFunctions':2,'nativePositiveScenarios':20,'bodyCliGateScenarios':15,'newCliGateScenarios':3,'argumentRecipes':['direct_original_bindings','source_typed_pure_rest_collector'],'currentCliExecutableSha256':binary_hash,'countsAreNonadditive':True,'metadataOnlyAfterFinalChecks':meta,'unchangedSources':unchanged,'wholeNativeRerun':False,'fullWindowsCertified':False,'otherPlatformsCertified':False,'fullParityCertified':False,'mainMerged':False,'releasePerformed':False}
(raw/'qualification.json').write_text(json.dumps(qualification,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
node=Path(shutil.which('node')).resolve();compiler=(w/'node_modules/typescript/lib/typescript.js').resolve()
(raw/'tools.json').write_text(json.dumps({'node':{'path':str(node),'sha256':sha(node.read_bytes()),'version':run(str(node),'--version').decode().strip()},'typescript':{'path':str(compiler),'sha256':sha(compiler.read_bytes()),'version':read(compiler.parent.parent/'package.json')['version']},'python':sys.version,'rustc':run(str(Path.home()/'.cargo/bin/rustc.exe'),'--version').decode().strip()},indent=2)+'\n',encoding='utf-8')
dest=w/'docs/validation/integration-2026-10-19/function-body-argument-contexts';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 r=read(p);phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':r.get('allChecksPassed'),'nativeAdmission':r.get('nativeAdmission'),'sourceUnchanged':r.get('sourceUnchanged'),'inputs':r.get('inputs',r.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-classic-body-argument-contexts-evaluation','sourceCommit':source,'baseIntegration':base,'scope':['Source-derived argument callable contexts and fixed pure rest collection','Original arguments evaluated once before defaults/patterns/body with dense own indexing','Direct literal/identifier recipe preserves existing scalar inference','Native binding/type/application gates; full STATUS/R1-R9 remains active'],'guardPhase':'guard-r2','retainedPhase':'retained-r2','countsAreNonadditive':True,'phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
readme=f'''# Argument contexts and once-only evaluation

Source `{source}` extends the optional `inlineCompiler` classic-body action.
Context-sensitive arguments retain callable parameter typing through a fixed pure
rest collector. The original argument list is passed once, before any original
parameter default, pattern or body initialization. Raw fresh arrays are read only
at their own dense numeric positions. An array destructure would invoke a mutable
iterator; that substitution is prohibited. Direct literal and identifier bindings
retain existing scalar/generic inference. Both closed recipes are reconstructed
independently from original source and share the original freshness, source,
binding/type, capacity/deadline and guarded application contracts.

| Frozen scope | Actual result |
| --- | --- |
| guard-r2, 811 unchanged native inputs | Fourteen body units; 116 integrations passed / zero failed / two existing ignored, eight targets |
| Format/build/all-target Clippy with warnings denied | Passed |
| retained-r2, 805 unchanged inputs | Python 17, typecheck, TypeScript 260 passed / three existing skips |
| Metadata after final checks | Python 17; all 811 native and 804 other retained inputs exact; only three R1 fields changed |

One added integration function covers twenty owned positive scenarios, including
contextual callback/object/tuple/rest/pattern arguments, generic function-value
literal types, anonymous arrow/function/class names, never-typed values that
actually return, default order, hostile iterators and direct scalar/var cases.
Fifteen body CLI gates include three additional strict TS/JS cases, independent
tsc, verification, approval, application and foreign-preserving recovery. Two
added units and extended actual-host tests reject forged UTF-8/documentation
bounds, template tags, names, recipes, executing the source carrier instead of
the collector, empty collector results, raw destructuring and hidden JSON fields.
Counts overlap; they are not added. Runtime oracles execute owned literal
fixtures only. Preparation executes no analyzed source or model.

The source callable signature and original-signature fallback remain under fixed
`void 0 !== void 0`. Source defaults and the fixed throwing signature body never
execute. Only the generated rest collector executes in an applied program; it
returns its raw rest array. The existing native parameter initialization consumes
fresh arguments afterward, retaining default closures, var environments and
finally behavior. No ambient Parameters helper or arbitrary printed compiler type
expression is inserted. Conditional tuple inference and keywords have fixed syntax;
JSDoc parameter/template pieces have original-comment/UTF-8/length bounds. The
host compiler supplies that bounded JSDoc enumeration and free-binding authority;
raw observations and surviving caller/argument type comparisons remain intact.

Development probes are distinct from current-source admission. Six initial
context cases produce TS7006 (plus a tuple assignment error), and two anonymous
argument controls change empty names to a generated name. Conditional parameter
prototypes still lose a generic callback literal type. Signature prototypes fix
doc ownership/scope, and an indexed collector retains the owned iterator/default
observations. The destructuring control changes result 3 to 99 and records an
extra iterator event despite identical type strings. This is retained as an
intentional red, not a passing native check. Prototype TypeScript-generic
signatures do not certify actual TypeScript-generic body frames, which remain
unsupported. A tagged-return alternative is also retained as a prototype only.

The first native feature run passes fifteen owned cases. The first broad guard
has fourteen unit passes, 114 integration passes and two failures. One old helper
rejects an original var inside an inert source-function copy; the same actual
native plan has a verified result hash and equal owned runtime result 3, with two
additional native var/escaped-var controls. The second failure is genuine generic
scalar widening (3 to number) from tuple capture. The final direct scalar recipe
keeps the old var test unchanged and preserves the scalar type. The final
source worker has twenty exact named-result type/runtime matches; these
synthetic requests claim no native admission. The current standalone CLI admits
the original scalar regression, leaves source bytes unchanged and matches the
final executable hash. Prior failed/pass phases are not added to final totals.
An additional mixed generic/callback counterprobe passes its reported result type
and owned runtime. A separate caller `@type {3}`/generic-factory probe has valid
original diagnostics and projected TS2322 at the generated result variable. The
caller annotation remains before the scaffold; ownership/expected-context work
is still open. Neither probe is a native acceptance upgrade.

Primary materialization checks {len(material)} paths: {materialization['raw']} raw
and {materialization['utf8CrlfOnly']} UTF-8 CRLF-only matches. The protected PostgreSQL
fixture is 2779 bytes and byte exact in both checkouts. No primary runtime rerun,
whole-native/Windows/current-head hosted/upstream certification, main merge or
release is claimed. Caller JSDoc ownership and mixed generic/expected-result argument contexts, actual
TypeScript generic frames, other parameter/annotation/predicate/diagnostic and
wider invocation/expression/declaration/language/consumer semantics remain open,
alongside the other STATUS/R1-R9 requirements and platform/deployment qualifications.

The [index](index.json) binds {len(files)} raw artifacts: original red/green and
control observations, available runners/workers, current-source regression logs,
metadata proofs, tool hashes and source materialization. Earlier archives remain
immutable.
'''
(dest/'README.md').write_text(readme,encoding='utf-8',newline='\n')
parent=dest.parent/'README.md';original=parent.read_bytes();nl=b'\r\n' if b'\r\n' in original else b'\n';at=original.index(nl+nl)+len(nl+nl)
entry=('Argument callable contexts and once-only evaluation have [separate evidence](function-body-argument-contexts/README.md):\nsource signatures, fixed collection and direct scalar bindings;116 scoped\nintegration passes and fourteen units. Counts overlap; full STATUS/Windows\nacceptance remains open.\n\n').replace('\n',nl.decode()).encode('utf-8')
parent.write_bytes(original[:at]+entry+original[at:]);assert parent.read_bytes().replace(entry,b'',1)==original
print(json.dumps({'sourceCommit':source,'artifacts':len(files),'indexBytes':(dest/'index.json').stat().st_size,'indexSha256':sha((dest/'index.json').read_bytes()),'materialization':{k:v for k,v in materialization.items() if k!='files'}}))
