from pathlib import Path
import subprocess,json,hashlib,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=root/'validation/integration-20261018/postgres-catalog'
read=lambda name:json.loads((base/name).read_bytes())
final=read('r8/result.json');assert final['sourceUnchanged'] and final['inputs']==681
log=(base/'r8/native.log').read_text(encoding='utf-8');groups=[list(map(int,m)) for m in re.findall(r'test result: .*?([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored',log)];totals=[sum(g[i] for g in groups) for i in range(3)]
assert len(groups)==9 and totals==[479,1,11],totals
assert log.count('... FAILED')==1 and 'code: 1314' in log and 'existing_output_file_directory_and_hardlink_are_never_overwritten ... FAILED' in log
quality=read('quality-r2/result.json');assert quality['allChecksPassed'] and quality['sourceUnchanged']
before=read('r8/source-before.json');assert before==read('r8/source-after.json')==read('quality-r2/source-before.json')==read('quality-r2/source-after.json')
for name,h in before.items():assert hashlib.sha256((work/name).read_bytes()).hexdigest()==h,name
for stage in ['tls-final-r3','quoted-endpoint-final-r2','live-deadline']:
    result=read(stage+'/result.json');assert result['sourceUnchanged'] and result['inputs']==681
tls=read('tls-final-r3/result.json');assert all(tls[k] for k in ['tlsScramValidated','unicodePasswordViaPrivateStdin','skippedScramRefused','liveRowQuotaRefused','lostStdoutPreservedArtifactAndIdentity','tlsHostnameMismatchRefused','ambientConfigurationIgnored','serverStopped']) and tls['ownedFixtureCredentialsRemoved']==3
weird=read('quoted-endpoint-final-r2/result.json');assert weird['quotedUnicodeDatabase'] and weird['quotedUnicodeRole'] and weird['visibleOwnerDefinitionsWithoutExecution'] and weird['serverStopped']
def git(where,*args):return subprocess.check_output(['git',*args],cwd=where).decode().strip()
assert git(root,'rev-parse','HEAD')=='4146434bb3fd7fde348d2de2480fddb91ed192aa'
assert not git(root,'status','--porcelain')
assert git(work,'branch','--show-current')=='work/r7-postgres-catalog-20261019'
assert not git(work,'diff','--cached','--name-only')
tracked=git(work,'diff','--name-only').splitlines();untracked=git(work,'ls-files','--others','--exclude-standard').splitlines();names=sorted(set(tracked+untracked))
assert all(n=='.gitattributes' or n.startswith(('native/src/','native/tests/','native/docs/')) for n in names),names
frozen=base/'final-source';frozen.mkdir(exist_ok=False)
for name in names:
    target=frozen/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes((work/name).read_bytes())
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--check'],cwd=work,check=True)
subprocess.run(['git','add','--',*names],cwd=work,check=True)
assert hashlib.sha256(subprocess.check_output(['git','show',':native/tests/fixtures/postgres-catalog-response.jsonl'],cwd=work)).hexdigest()=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
subprocess.run(['git','commit','-m','feat(native): capture and replay bounded PostgreSQL catalogs'],cwd=work,check=True)
source=git(work,'rev-parse','HEAD');assert not git(work,'status','--porcelain')
subprocess.run(['git','merge','--no-ff','work/r7-postgres-catalog-20261019','-m','Merge native PostgreSQL catalog checkpoint'],cwd=root,check=True)
result={'sourceCommit':source,'runtimeCommit':git(root,'rev-parse','HEAD'),'parentCommit':'4146434bb3fd7fde348d2de2480fddb91ed192aa','paths':names,'nativeGroups':groups,'nativeTotals':{'passed':479,'failed':1,'ignored':11},'wholeNativeRerun':False}
(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
