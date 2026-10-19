from pathlib import Path
import json,hashlib,subprocess,datetime,shutil,re
root=Path.cwd();src=root/'validation/integration-20261018/directory-moves-final';dev=root/'validation/integration-20261018/directory-moves-development'
summary=json.loads((src/'summary.json').read_text(encoding='utf-8'));before=json.loads((src/'source-before.json').read_text(encoding='utf-8'))
assert summary['sourceUnchanged'] and summary['wholeNativeRun'] and summary['allTestableNativeTargetsRan'];assert before==json.loads((src/'source-after.json').read_text(encoding='utf-8'))
def initial_bytes(n):
 p=dev/'pre-repair-full-source'/n
 return p.read_bytes() if p.is_file() else (root/n).read_bytes()
for n,h in before.items():assert hashlib.sha256(initial_bytes(n)).hexdigest()==h,n
assert all(c['exitCode']==0 for c in summary['checks'] if c['gate']!='whole-native')
repair_dir=dev/'rename-admission-repair';repair=json.loads((repair_dir/'summary.json').read_bytes());assert repair['sourceUnchanged'] and len(repair['gates'])==4
ts_repair=json.loads((dev/'post-repair-typescript/summary.json').read_bytes());assert ts_repair['sourceUnchanged'] and all(c['exitCode']==0 for c in ts_repair['checks'])
assert all(c['exitCode']==0 for c in repair['gates'] if c['gate']!='scoped-native')
repair_log=(repair_dir/'scoped-native.log').read_text(encoding='utf-8',errors='replace');repair_failures=[{'testName':m[1],'windowsError1314':'code: 1314' in m[2]} for m in re.finditer(r'---- (.*?) stdout ----(.*?)(?=\n---- |\nfailures:|\Z)',repair_log,re.S)];assert all(f['windowsError1314'] for f in repair_failures)
assert 'test directory_missing_excluded_and_readonly_sources_have_no_file_rename_authority ... ok' in repair_log
assert 'test captured_file_rename_cannot_gain_directory_authority_from_a_folder_capable_provider ... ok' in repair_log
repair_before=json.loads((repair_dir/'source-before.json').read_bytes());assert repair_before==json.loads((repair_dir/'source-after.json').read_bytes());work=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence-r1-directory');bindings=[];differences=[]
for n,h in repair_before.items():
 b=(work/n).read_bytes();assert hashlib.sha256(b).hexdigest()==h,n;old=initial_bytes(n);same=old.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n')
 if not same:differences.append(n)
 bindings.append({'path':n,'frozenFullSha256':before[n],'scopedRepairSha256':h,'sameNormalizedSource':same})
assert differences==['native/src/agent_repl.rs','native/tests/facade_directory_move.rs'],differences
out=root/'docs/validation/integration-2026-10-19/directory-moves';out.mkdir(exist_ok=False)
shutil.copytree(src,out/'final');shutil.copytree(dev,out/'development')
for name in ['run-directory-final.py','preserve-directory-moves.py','run-directory-post-repair-ts.py']:shutil.copyfile(root/'validation/integration-20261018'/name,out/name)
temp=Path.home()/'AppData/Local/Temp'
for name in ['ci-directory-r1.py','ci-directory-r2.py','ci-directory-r3.py','ci-directory-r4.py','ci-directory-clippy-r5.py','ci-directory-alias-red.py','ci-directory-versions.py','ci-directory-repair-r6.py','ci-directory-freeze-complete.py']:
 if (temp/name).is_file():shutil.copyfile(temp/name,out/name)
new=['native/src/changes_directory.rs','native/src/changes_directory_tests.rs','native/src/semantic_directory_move.rs','native/tests/directory_move_live.rs','native/tests/facade_directory_move.rs','native/tests/fixtures/directory-move-lsp.mjs'];new_files=[]
for n in new:
 target=out/'new-source'/n;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(initial_bytes(n));new_files.append({'source':n,'artifact':target.relative_to(out).as_posix(),'sha256':hashlib.sha256(initial_bytes(n)).hexdigest(),'scope':'initial_frozen_full_run'})
(out/'runtime-change.patch').write_bytes(subprocess.check_output(['git','-c','core.safecrlf=false','diff','272866f6c106932c7e2f14f62f460d0a2f7a2200','eaf9d5995c2370b4873bbd752f1479b0923637a0','--no-ext-diff','--binary','--','native/src','native/tests']))
raw=(src/'whole-native.log').read_text(encoding='utf-8',errors='replace');failures=[]
for m in re.finditer(r'---- (.*?) stdout ----(.*?)(?=\n---- |\nfailures:|\Z)',raw,re.S):
 failures.append({'testName':m[1],'windowsError1314':'code: 1314' in m[2],'windowsSharingError32':'code: 32' in m[2],'output':m[2].strip()})
(out/'failure-classification.json').write_text(json.dumps({'failures':failures,'allObservedFailuresAreFixturePrerequisite1314':bool(failures) and all(f['windowsError1314'] for f in failures)},indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
assert [f['testName'] for f in failures if not f['windowsError1314']]==['directory_missing_excluded_and_readonly_sources_have_no_file_rename_authority']
r4=json.loads((dev/'source-before-r4.json').read_text(encoding='utf-8'));normalized_differences=[]
for n in r4:
 old=dev/'r4-source'/n
 if old.is_file() and old.read_bytes().replace(b'\r\n',b'\n')!=initial_bytes(n).replace(b'\r\n',b'\n'):normalized_differences.append(n)
assert normalized_differences==['native/tests/facade_directory_move.rs'],normalized_differences
old=(dev/'r4-source/native/tests/facade_directory_move.rs').read_bytes().replace(b'\r\n',b'\n');new=initial_bytes('native/tests/facade_directory_move.rs').replace(b'\r\n',b'\n');assert old.replace(b'.err()\n        .expect("Unsupported folder edit must refuse")',b'.expect_err("Unsupported folder edit must refuse")')==new
p='docs/parity/native-reconciliation-2026-10-15.json';prior=json.loads(subprocess.check_output(['git','show','272866f6c106932c7e2f14f62f460d0a2f7a2200:'+p]));current=json.loads((root/p).read_text(encoding='utf-8'));inventory={k:prior[k]==current[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']};assert all(inventory.values())
(out/'post-freeze-repair-inputs.json').write_bytes((json.dumps({'allScopedInputsBound':True,'normalizedRuntimeDifferences':differences,'entries':bindings},indent=2)+'\n').encode())
index={'schemaVersion':1,'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'parentCommit':'272866f6c106932c7e2f14f62f460d0a2f7a2200','implementationCommit':'e6a4586','integratedCommit':'eaf9d5995c2370b4873bbd752f1479b0923637a0','accepted':False,'allLocalChecksPassed':summary['allChecksPassed'],'wholeNativeRun':True,'frozenInputCount':len(before),'wholeNativeTotals':summary['wholeNativeTotals'],'nativeResultGroups':len(next(c for c in summary['checks'] if c['gate']=='whole-native')['resultGroups']),'allTestableNativeTargetsRan':True,'inventoryPreserved':inventory,'newSourceFiles':new_files,'summary':summary,'postFreezeRepair':{'summary':repair,'allProductCasesPassed':True,'knownSymlinkPrerequisiteFailures':repair_failures,'sourceBinding':'post-freeze-repair-inputs.json','normalizedRuntimeDifferences':differences,'wholeNativeRerun':False,'countsAreNonadditive':True},'findings':[
 'One complete captured text directory is selected under the existing host scope and exactly one explicitly registered provider language. A matching standard-LSP folder willRenameFiles filter is required; only checked original-coordinate text changes or null are accepted. Native code appends the sealed resource effect and prepares one plan. Multiple provider languages, ignored-capture or host-provider workspace-root migration, empty-only and uncaptured/binary trees remain unsupported; complete=false and source omissions remain visible.',
 'The directory-move-v1 plan identity binds the original directory shape including empty directories, modes, stable directory and parent identities, exact captured files and mapped result. Legacy file-only serialization and IDs remain unchanged. Actual source membership, host spellings and bodies are rechecked before the journal. One native filesystem rename precedes byte repairs; cross-filesystem recursive copy is not introduced. Copied verification checks empty shape as well as source bytes. Recovery handles a renamed tree even when cancellation preceded appliedPaths publication and preserves foreign entries, bytes and directories for explicit reconciliation.',
 'Finite selection admits 1..1000 text files, at most2000 directories and at most16000000 bytes counted per physical file, before source body reads. Real boundary cases cover1000/1001 files,2000/2001 directories and16000000/16000001 bytes even with identical CAS bodies. Existing plan, snapshot, diff, LSP frame/virtual-edit and request limits remain. Cooperative checks and identity observations do not establish an OS sandbox, atomic multi-file visibility or protection from hostile same-account races.',
 'The first R2 integration run preserves one outdated directory-refusal assertion in the file facade; the revised contract requires capability negotiation and still refuses a file-only provider without publishing a plan. An actual RED reproduced Windows destination-parent case-alias acceptance; exact existing host spelling now refuses before provider startup. R3 passes425 library tests/7 ignored plus7 folder,8 file and13 saved-plan cases. R4 passes9 folder cases, including an actual unprivileged junction and zero-exit verifiers deleting/creating empty directories. These scoped counts overlap the final suite and are not added to its totals.',
 'The separate actual installed rust-analyzer1.94.1 gate returned module-declaration and use repairs for an owned dependency-free Rust folder. Actual rustc1.94.1 compiled the copied result; exact approved workspace application and recovery passed. No fixture/server/compiler fallback, dependency build script, project execution, default model or proprietary IDE acceptance is asserted. The final integrated gate is recorded separately and remains nonadditive.',
 'Initial all-target Clippy R5 preserves an err_expect diagnostic in one new test. The sole subsequent normalized R4-to-initial-full source difference changes err().expect() to expect_err(); the initial full integrated format/Clippy/test run uses that frozen source. Raw pre/post manifests, earlier failed assertions, actual RED source and live outputs remain preserved. The Windows full-suite profile serializes independent tests while retaining their internal concurrency adversaries. Remaining STATUS/R1-R9, upstream and platform certification, package/extension-host acceptance, main merge and release remain open.',
 'The complete initial full run retains one genuine file-rename admission regression in addition to ten OS1314 fixture prerequisites. Its shared relocation helper entered the directory path for jb.rename, starting a provider. A separate two-input repair makes directory selection private and true only for jb.move; jb.rename refuses before provider startup/publication even with a folder-capable server. Existing rename tests and new V8/V9 omitted/null cases pass in the scoped repair. All repair inputs are source-bound; library and R1 regression counts are nonadditive, and the scoped run retains its unchanged OS1314 file-delete setup failure. Integrated whole-native acceptance is not inferred from this repair; the original full run remains failed.'
 ],'r4ToInitialFullNormalizedSourceDifferences':normalized_differences,'artifacts':[]}
index['postFreezeRepair']['postRepairTypescript']=ts_repair
index['postFreezeRepair']['integratedRuntimeCommit']=subprocess.check_output(['git','rev-parse','HEAD']).decode().strip()
for p in sorted(out.rglob('*')):
 if p.is_file() and p!=out/'index.json':
  b=p.read_bytes();index['artifacts'].append({'path':p.relative_to(out).as_posix(),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');print(json.dumps({k:index[k] for k in ['frozenInputCount','wholeNativeTotals','nativeResultGroups','accepted']}));print(json.dumps({'artifacts':len(index['artifacts'])}))
