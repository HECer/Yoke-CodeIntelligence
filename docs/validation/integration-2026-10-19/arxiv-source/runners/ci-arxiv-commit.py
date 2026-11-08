from pathlib import Path
import subprocess,json,hashlib,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=root/'validation/integration-20261018/arxiv-source';parent='280947eb70a8b596c848863029d6e39c15704698'
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
final=read('final/result.json');quality=read('quality-r2/result.json');assert final['sourceUnchanged'] and quality['sourceUnchanged'] and all(c['exitCode']==0 for c in quality['checks'])
before=read('final/source-before.json');assert before==read('final/source-after.json')==read('quality-r2/source-before.json')==read('quality-r2/source-after.json')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
log=(base/'final/native.log').read_text(encoding='utf-8');groups=[list(map(int,m)) for m in re.findall(r'test result: .*?([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored',log)];assert len(groups)==15,groups
for name in ['arxiv_source.rs','arxiv_source_http.rs','arxiv_source_cli.rs']:
 section=log.split('Running tests\\'+name,1)[1].split('Running ',1)[0];assert 'test result: ok.' in section and '... FAILED' not in section,name
assert not re.search(r'test arxiv_rate::.*?FAILED',log)
failed=re.findall(r'^test (\S+) \.\.\. FAILED$',log,re.M);allowed={'media_cannot_bypass_source_scope_links_ignore_or_protected_paths','github_app::digest::history_boundary_tests::history_owned_process_tests::cancellation_and_deadline_stop_owned_git_and_restore_the_same_store'};assert set(failed)<=allowed,failed
for name in ['native/tests/media.rs','native/src/github_app_digest_history_process_tests.rs','native/src/process.rs','native/src/process_windows_job.rs']:
 old=subprocess.check_output(['git','show',parent+':'+name],cwd=work);assert old.replace(b'\r\n',b'\n')==(work/name).read_bytes().replace(b'\r\n',b'\n'),name
live=read('live-final/result.json');assert live['nativeMetadataCapture'] and live['nativePdfCapture'] and live['sourceUnchanged'] and live['inputs']==len(before)
def git(where,*args):return subprocess.check_output(['git',*args],cwd=where).decode().strip()
assert git(root,'rev-parse','HEAD')==parent and not git(root,'status','--porcelain')
assert git(work,'branch','--show-current')=='work/r7-arxiv-acquisition-20261019' and not git(work,'diff','--cached','--name-only')
names=sorted(set(git(work,'diff','--name-only').splitlines()+git(work,'ls-files','--others','--exclude-standard').splitlines()));assert all(n.startswith(('native/src/','native/tests/','native/docs/')) or n=='native/README.md' for n in names),names
assert git(work,'diff','--','Cargo.toml','Cargo.lock','native/Cargo.toml')==''
out=base/'final-source';out.mkdir(exist_ok=False)
for name in names:
 target=out/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes((work/name).read_bytes())
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--check'],cwd=work,check=True)
subprocess.run(['git','add','--',*names],cwd=work,check=True)
subprocess.run(['git','commit','-m','feat(native): acquire coordinated arxiv metadata and PDF sources'],cwd=work,check=True,stdout=subprocess.PIPE)
source=git(work,'rev-parse','HEAD');assert not git(work,'status','--porcelain')
subprocess.run(['git','merge','--no-ff','work/r7-arxiv-acquisition-20261019','-m','Merge native coordinated arxiv source checkpoint'],cwd=root,check=True,stdout=subprocess.PIPE)
result={'sourceCommit':source,'runtimeCommit':git(root,'rev-parse','HEAD'),'parentCommit':parent,'paths':names,'nativeGroups':groups,'nativeTotals':{k:sum(g[i] for g in groups) for i,k in enumerate(['passed','failed','ignored'])},'failedCases':failed,'newCases':16,'wholeNativeRerun':False};(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
