from pathlib import Path
import subprocess,json,hashlib,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/windows-process-exit';parent='7237a6c54b217df7be61afa8044786a1c7e75406';read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
checks=read('r4/result.json');final=read('final/result.json');assert checks['allChecksPassed'] and checks['sourceUnchanged'] and final['sourceUnchanged'];before=read('final/source-before.json');assert before==read('final/source-after.json')==read('r4/source-before.json')==read('r4/source-after.json')
names=['native/src/github_app_digest_history_process_tests.rs','native/src/process_windows_job.rs','native/src/process_windows_job_tests.rs'];baseline=read('baseline/source-before.json');assert set(before)==set(baseline) and {n for n in before if before[n]!=baseline[n]}==set(names)
for n,h in before.items():assert sha((work/n).read_bytes())==h,n
assert 'owned_job_probe' not in (work/'native/src/process_windows_job.rs').read_text(encoding='utf-8')
log=(base/'final/native.log').read_text(encoding='utf-8');groups=[list(map(int,m)) for m in re.findall(r'test result: .*?([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored',log)];totals=[sum(g[i] for g in groups) for i in range(3)];assert len(groups)==15 and totals==[534,1,8],(groups,totals)
failed=re.findall(r'^test (\S+) \.\.\. FAILED$',log,re.M);assert failed==['media_cannot_bypass_source_scope_links_ignore_or_protected_paths'] and 'code: 1314' in log
old=subprocess.check_output(['git','show',parent+':native/tests/media.rs'],cwd=work);assert old.replace(b'\r\n',b'\n')==(work/'native/tests/media.rs').read_bytes().replace(b'\r\n',b'\n')
for name in ['live-pg-tls','live-pdf']:
 live=read(name+'/result.json');assert live['sourceUnchanged'] and live['inputs']==len(before)
assert read('live-pg-tls/result.json')['tlsScramValidated'] and read('live-pg-tls/result.json')['ownedFixtureCredentialsRemoved']==3 and read('live-pdf/result.json')['nativeFrozenPdfWorker']
def git(where,*args):return subprocess.check_output(['git',*args],cwd=where).decode().strip()
assert git(root,'rev-parse','HEAD')==parent and not git(root,'status','--porcelain');assert set(git(work,'diff','--name-only').splitlines())==set(names) and not git(work,'ls-files','--others','--exclude-standard') and not git(work,'diff','--cached','--name-only')
out=base/'final-source';out.mkdir(exist_ok=False)
for name in names:
 target=out/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes((work/name).read_bytes())
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--check'],cwd=work,check=True);subprocess.run(['git','add','--',*names],cwd=work,check=True);subprocess.run(['git','commit','-m','fix(native): wait for held Windows job members before cleanup success'],cwd=work,check=True,stdout=subprocess.PIPE)
source=git(work,'rev-parse','HEAD');assert not git(work,'status','--porcelain');subprocess.run(['git','merge','--no-ff','work/r8-windows-process-exit-proof-20261019','-m','Merge bounded Windows owned member exit proof and regression evidence'],cwd=root,check=True,stdout=subprocess.PIPE)
proof={'sourceCommit':source,'runtimeCommit':git(root,'rev-parse','HEAD'),'parentCommit':parent,'paths':names,'runtimeCodeChanged':True,'newCases':2,'nativeGroups':groups,'nativeTotals':dict(zip(['passed','failed','ignored'],totals)),'failedCases':failed,'existingExitTimeAndSqlRestorationAssertionsRetained':True,'maxObservedMembersPerCleanup':4096,'pidBasedTermination':False,'sameBinaryPriorCaseRepeats':8,'temporaryTracePriorCaseRepeats':4,'kernelCauseCertified':False,'wholeNativeRerun':False};(base/'source-integration.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps(proof))
