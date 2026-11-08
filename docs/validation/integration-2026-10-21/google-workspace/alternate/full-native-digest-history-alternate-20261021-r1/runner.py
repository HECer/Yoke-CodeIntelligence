"""Recover only a locked test output; preserve held processes and original paths."""
from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,time
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach'; base=root/'validation/integration-20261018/full-native-default-20261021-r1'
final=json.loads((base/'result.json').read_text()); assert final['allRegisteredGroupsAttempted'] and final['sourceUnchanged']
before=json.loads((base/'source-before.json').read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def unchanged():return all((work/n).is_file() and sha(work/n)==h for n,h in before.items())
assert unchanged() and not subprocess.check_output(['git','status','--porcelain'],cwd=work).strip()
out=root/'validation/integration-20261018/full-native-digest-history-alternate-20261021-r1';out.mkdir(exist_ok=False)
shutil.copyfile(__file__,out/'runner.py')
exe=root/'target/debug/deps/digest_history_jobs-full-native-20261021.exe';assert not exe.exists()
env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
env['CARGO_MANIFEST_DIR']=str(work/'native');env['CARGO_MANIFEST_PATH']=str(work/'native/Cargo.toml')
cmd=[str(Path.home()/'.cargo/bin/cargo.exe'),'rustc','--profile','test','--locked','-j','2','--target-dir',str(root/'target'),'--test','digest_history_jobs','--','-C','extra-filename=-full-native-20261021']
records=[]
for name,command,cwd in [('compile',cmd,work),('tests',[str(exe),'--test-threads=1'],work/'native')]:
    print('FULL NATIVE LOCK RECOVERY',name,flush=True);start=time.monotonic()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=cwd,env=env,stdout=log,stderr=subprocess.STDOUT)
    raw=(out/(name+'.log')).read_text(errors='replace')
    records.append({'name':name,'command':command,'cwd':str(cwd),'exitCode':p.returncode,'seconds':time.monotonic()-start,
      'totals':re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)})
    if p.returncode:break
result={'sourceCommit':final['sourceCommit'],'sourceUnchanged':unchanged(),'commands':records,
 'standardCargoTest':False,'difference':'Only test artifact output suffix; same test profile, frozen sources, lock, test harness, assertions and package cwd. Held original output/processes untouched.',
 'allChecksPassed':len(records)==2 and all(r['exitCode']==0 for r in records) and unchanged(),'fullWindowsCertified':False}
if exe.is_file():result['executable']={'path':str(exe),'sha256':sha(exe),'bytes':exe.stat().st_size}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
assert result['allChecksPassed']
