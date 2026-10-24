from pathlib import Path
import subprocess,json,hashlib,re,tomllib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=root/'validation/integration-20261018/source-acquisition';parent='f26f6b7e03d5bd011353cbeeb5203c9121da8c70'
read=lambda name:json.loads((base/name).read_bytes())
final=read('final/result.json');assert final['sourceUnchanged'] and final['inputs']==688
log=(base/'final/native.log').read_text(encoding='utf-8');groups=[list(map(int,m)) for m in re.findall(r'test result: .*?([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored',log)];assert len(groups)==12 and [sum(g[i] for g in groups) for i in range(3)]==[516,1,8],groups
assert log.count('... FAILED')==1 and 'code: 1314' in log and 'media_cannot_bypass_source_scope_links_ignore_or_protected_paths ... FAILED' in log
legacy=subprocess.check_output(['git','show',parent+':native/tests/media.rs'],cwd=work);current=(work/'native/tests/media.rs').read_bytes();assert legacy.replace(b'\r\n',b'\n')==current.replace(b'\r\n',b'\n')
quality=read('quality-r2/result.json');assert quality['allChecksPassed'] and quality['sourceUnchanged']
before=read('final/source-before.json');assert before==read('final/source-after.json')==read('quality-r2/source-before.json')==read('quality-r2/source-after.json')
for name,h in before.items():assert hashlib.sha256((work/name).read_bytes()).hexdigest()==h,name
live=read('live-r2/result.json');assert live['sourceUnchanged'] and live['inputs']==688 and len(live['results'])==3 and live['results'][1]['nativePdfText'] and live['results'][2]['artifactPreserved'] and live['results'][2]['identityReported']
old=tomllib.loads(subprocess.check_output(['git','show',parent+':Cargo.lock'],cwd=work).decode());new=tomllib.loads((work/'Cargo.lock').read_text(encoding='utf-8'));own=next(x for x in new['package'] if x['name']=='yoke-code-intelligence');assert own['dependencies'].count('rustls-native-certs')==1;own['dependencies'].remove('rustls-native-certs');assert new==old
(base/'dependency-lock-proof.json').write_text(json.dumps({'parentCommit':parent,'onlyChange':'existing pinned rustls-native-certs0.8.4 added to own direct dependencies','allPackageVersionsSourcesChecksumsAndOtherDependenciesUnchanged':True},indent=2)+'\n',encoding='utf-8')
def git(where,*args):return subprocess.check_output(['git',*args],cwd=where).decode().strip()
assert git(root,'rev-parse','HEAD')==parent and not git(root,'status','--porcelain')
assert git(work,'branch','--show-current')=='work/r7-url-acquisition-20261019' and not git(work,'diff','--cached','--name-only')
names=sorted(set(git(work,'diff','--name-only').splitlines()+git(work,'ls-files','--others','--exclude-standard').splitlines()));assert all(n=='Cargo.lock' or n=='native/Cargo.toml' or n.startswith(('native/src/','native/tests/','native/docs/')) for n in names),names
out=base/'final-source';out.mkdir(exist_ok=False)
for name in names:
    target=out/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes((work/name).read_bytes())
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--check'],cwd=work,check=True)
subprocess.run(['git','add','--',*names],cwd=work,check=True)
subprocess.run(['git','commit','-m','feat(native): acquire frozen HTTP sources and inspect PDF bytes'],cwd=work,check=True,stdout=subprocess.PIPE)
source=git(work,'rev-parse','HEAD');assert not git(work,'status','--porcelain')
subprocess.run(['git','merge','--no-ff','work/r7-url-acquisition-20261019','-m','Merge native received HTTP source checkpoint'],cwd=root,check=True,stdout=subprocess.PIPE)
result={'sourceCommit':source,'runtimeCommit':git(root,'rev-parse','HEAD'),'parentCommit':parent,'paths':names,'nativeGroups':groups,'nativeTotals':{k:sum(g[i] for g in groups) for i,k in enumerate(['passed','failed','ignored'])},'wholeNativeRerun':False};(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
