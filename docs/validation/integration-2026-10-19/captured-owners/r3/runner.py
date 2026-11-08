from pathlib import Path
import subprocess,hashlib,json,time,os,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/captured-owners/r3';out.mkdir(parents=True,exist_ok=False)
names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock'],cwd=root).decode().split('\0')))
def inventory():return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in names if n and (root/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
for name in ['native/src/git_owners.rs','native/src/git_owners_tests.rs','native/src/git_owners_cli.rs','native/src/git_areas.rs','native/src/git_workflows.rs','native/src/main.rs','native/tests/git_owners.rs','native/tests/support/git_impact_fixture.rs','native/docs/git-owners.md']:
    dest=out/'source'/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes((root/name).read_bytes())
shutil.copyfile(__file__,out/'runner.py')
old=subprocess.check_output(['git','show','bf648cb:native/src/git_workflows.rs'],cwd=root).decode();current=(root/'native/src/git_workflows.rs').read_text(encoding='utf-8')
def legacy(s):return s.replace('\r\n','\n').split('fn owner_rules(',1)[1].split('\nfn changed_commit_paths(',1)[0]
assert legacy(old)==legacy(current)
(out/'legacy-owner-parser-proof.json').write_text(json.dumps({'comparison':'LF-normalized fn owner_rules body and signature suffix','parentCommit':'bf648cb8625acf98ff7849389f831783fae4e1eb','unchanged':True,'sha256':hashlib.sha256(legacy(old).encode()).hexdigest()},indent=2)+'\n')
cargo=str(Path.home()/'.cargo/bin/cargo.exe');target=str(primary/'target');base=[cargo,'test','--workspace','--locked','-j','2','--target-dir',target]
commands=[('contracts',base+['--test','git_owners','--','--test-threads=1']),('library',base+['--lib','--','--test-threads=1']),('regressions',base+['--no-fail-fast','--test','git_areas','--test','git_workflows','--','--test-threads=1']),('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','-j','2','--target-dir',target,'--','-D','warnings']),('format',[cargo,'fmt','--all','--','--check'])]
env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH'];results=[]
for name,cmd in commands:
    print('START '+name,flush=True);start=time.time()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    results.append({'name':name,'command':cmd,'exitCode':p.returncode,'seconds':round(time.time()-start,2)});(out/'results.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results[-1]),flush=True)
    if p.returncode and name=='contracts':break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');summary={'sourceUnchanged':before==after,'inputs':len(before),'plannedCommands':len(commands),'executedCommands':len(results),'allChecksPassed':len(results)==len(commands) and all(r['exitCode']==0 for r in results),'results':results};(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
