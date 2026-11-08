from pathlib import Path
import subprocess,json,hashlib,time,shutil,re,os
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
root=work.parent/'Yoke-CodeIntelligence'
out=root/'validation/integration-20261018/app-history-integration/native-job-output-r6'
out.mkdir(parents=True,exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest()
def inventory():
 names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')
 return {n:sha((work/n).read_bytes()) for n in sorted(set(names)) if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
assert before==json.loads((out.parent/'final-r2/source-before.json').read_text())
shutil.copyfile(__file__,out/'runner.py')
env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
cargo=str(Path.home()/'.cargo/bin/cargo.exe')
commands=[('build',[cargo,'rustc','--profile','test','-p','yoke-code-intelligence','--locked','-j','2','--target-dir',str(root/'target'),'--test','digest_history_jobs','--','-C','extra-filename=-r8-acquisition'])]
runs=[];binary=None
for name,cmd in commands:
 print('START APP HISTORY ALTERNATE OUTPUT',name,flush=True);start=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:r=subprocess.run(cmd,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
 runs.append({'name':name,'command':cmd,'exitCode':r.returncode,'seconds':time.monotonic()-start})
 if r.returncode:break
else:
 exe=root/'target/debug/deps/digest_history_jobs-r8-acquisition.exe';assert exe.is_file()
 binary={'path':str(exe),'bytes':exe.stat().st_size,'beforeSha256':sha(exe.read_bytes())}
 cmd=[str(exe),'--test-threads=1'];start=time.monotonic()
 print('START APP HISTORY ALTERNATE OUTPUT actual native harness',flush=True)
 with (out/'native.log').open('wb') as log:r=subprocess.run(cmd,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
 groups=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', (out/'native.log').read_text(errors='replace'),re.M)
 runs.append({'name':'native','command':cmd,'exitCode':r.returncode,'seconds':time.monotonic()-start,'groups':len(groups),'totals':{k:sum(int(g[i]) for g in groups) for i,k in enumerate(['passed','failed','ignored'])}})
 binary['afterSha256']=sha(exe.read_bytes());assert binary['beforeSha256']==binary['afterSha256']
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
result={'sourceUnchanged':before==after,'inputs':len(before),'commands':runs,'binary':binary,'allChecksPassed':len(runs)==2 and all(r['exitCode']==0 for r in runs),'standardCargoTest':False,'compilerDifference':'Explicit test profile; only target output suffix -C extra-filename=-r8-acquisition; same unchanged source, dependency lock, normal test harness and assertions. Original target output is held by crashed MSVC linker PID22940 (termination denied).','wholeNativeRerun':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
assert result['sourceUnchanged'] and result['allChecksPassed']
