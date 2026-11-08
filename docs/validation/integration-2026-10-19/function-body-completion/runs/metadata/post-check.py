from pathlib import Path
import hashlib,json,subprocess,shutil,sys,time,re
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-completion';out=raw/'metadata'
read=lambda p:json.loads(p.read_text());sha=lambda b:hashlib.sha256(b).hexdigest()
proof=read(out/'proof.json');native=read(raw/'guard-r4/source-after.json');ret=read(raw/'retained-r5/source-after.json');tracker='docs/parity/native-reconciliation-2026-10-15.json'
def check():
 assert all(sha((w/n).read_bytes())==h for n,h in native.items())
 assert all(sha((w/n).read_bytes())==(proof['trackerAfterSha256'] if n==tracker else h) for n,h in ret.items())
check();start=time.monotonic();command=[sys.executable,'-m','unittest','discover','-s','scripts/tests','-v']
with (out/'python-contracts.log').open('wb') as log:p=subprocess.run(command,cwd=w,stdout=log,stderr=subprocess.STDOUT)
check();text=(out/'python-contracts.log').read_text();count=int(re.search(r'Ran (\d+) tests?',text).group(1))
shutil.copyfile(__file__,out/'post-check.py')
result={'allChecksPassed':p.returncode==0,'sourceUnchanged':True,'command':command,'exitCode':p.returncode,'pythonContracts':count,'seconds':time.monotonic()-start,'nativeInputsExact':len(native),'otherRetainedInputsExact':len(ret)-1,'trackerAfterSha256':proof['trackerAfterSha256'],'wholeNativeRerun':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result));assert p.returncode==0 and count==17
