from pathlib import Path
import subprocess,json,hashlib,os,time,sys
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=root/'validation/integration-20261018/mixed-meaning-provider/integrated'
out.mkdir(exist_ok=False)
def inventory():
    names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=root).decode().split('\0')
    return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and (root/n).is_file() and (n.startswith(('native/src/','native/tests/','native/examples/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','native/Cargo.toml','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json','.github/workflows/rust.yml','.gitattributes'])}
before=inventory()
(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
env=os.environ.copy()
env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
results=[]
for name,cmd in [('format',[str(Path.home()/'.cargo/bin/cargo.exe'),'fmt','--all','--check']),('python-contracts',[sys.executable,'-m','unittest','discover','-s','scripts/tests','-v']),('typecheck',['npm.cmd','run','typecheck']),('tests',['npm.cmd','test'])]:
    print('START MIXED MEANING PROVIDER INTEGRATED '+name,flush=True);start=time.monotonic()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    results.append({'name':name,'command':cmd,'exitCode':p.returncode,'seconds':round(time.monotonic()-start,2)})
after=inventory()
(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8')
result={'sourceUnchanged':before==after,'frozenInputs':len(before),'commands':results,'allChecksPassed':all(r['exitCode']==0 for r in results),'wholeNativeRerun':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
assert result['allChecksPassed'] and result['sourceUnchanged']
