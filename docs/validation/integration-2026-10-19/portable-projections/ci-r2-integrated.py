from pathlib import Path
import subprocess,json,hashlib,os,time,sys
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
base=root/'validation/integration-20261018/portable-projections';dev=base/'r4';out=base/'integrated';out.mkdir(exist_ok=False)
assert json.loads((dev/'freeze.json').read_bytes())['allNewCapabilityChecksPassed']
scoped=json.loads((dev/'source-before.json').read_bytes());assert scoped==json.loads((dev/'source-after.json').read_bytes())
def sha(b):return hashlib.sha256(b).hexdigest()
entries=[]
for name,h in scoped.items():
    old=(work/name).read_bytes();current=(root/name).read_bytes();assert sha(old)==h,name
    assert old==current or old.replace(b'\r\n',b'\n')==current.replace(b'\r\n',b'\n'),name
    entries.append({'path':name,'scopedSha256':h,'integratedSha256':sha(current),'materialization':'raw_equal' if old==current else 'crlf_only_checkout_materialization'})
(out/'scoped-source-materialization.json').write_text(json.dumps(entries,indent=2)+'\n')
def inventory():
    names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=root).decode().split('\0')
    return {n:sha((root/n).read_bytes()) for n in sorted(set(names)) if n and (root/n).is_file() and (n.startswith(('native/src/','native/tests/','native/examples/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','native/Cargo.toml','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json','.github/workflows/rust.yml'])}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
results=[]
for name,cmd in [('format',[str(Path.home()/'.cargo/bin/cargo.exe'),'fmt','--all','--check']),('python-contracts',['python','-m','unittest','discover','-s','scripts/tests','-v']),('typecheck',['npm.cmd','run','typecheck']),('tests',['npm.cmd','test'])]:
    print('START '+name,flush=True);start=time.time()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    results.append({'name':name,'command':cmd,'exitCode':p.returncode,'seconds':round(time.time()-start,2)});print(json.dumps(results[-1]),flush=True)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
summary={'sourceUnchanged':before==after,'frozenInputs':len(before),'nativeInputsMatched':len(entries),'commands':results,'allChecksPassed':all(x['exitCode']==0 for x in results),'runtimeCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip(),'wholeNativeRerun':False}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
assert before==after and summary['allChecksPassed']
