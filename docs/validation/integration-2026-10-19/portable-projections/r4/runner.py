import subprocess,pathlib,json,hashlib,os,time,shutil
root=pathlib.Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
primary=pathlib.Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=primary/'validation/integration-20261018/portable-projections/r4';out.mkdir(parents=True,exist_ok=False)
files=sorted(set(subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0'))|set(subprocess.check_output(['git','ls-files','--others','--exclude-standard','-z','native'],cwd=root).decode().split('\0')))
files=[x for x in files if x and (x.startswith('native/') or x in ['Cargo.toml','Cargo.lock','rust-toolchain.toml']) and (root/x).is_file()]
def inventory():return {x:hashlib.sha256((root/x).read_bytes()).hexdigest() for x in files}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
for name in ['native/src/meaning_projection.rs','native/src/meaning_projection_cli.rs','native/src/main.rs','native/src/lib.rs','native/tests/meaning_projection_portable.rs','native/tests/support/meaning_semantics_fixture.rs','native/docs/meaning-projection.md']:
    dest=out/'source'/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(root/name,dest)
shutil.copyfile(__file__,out/'runner.py')
cargo=r'C:\Users\Lomi\.cargo\bin\cargo.exe';base=[cargo,'test','--workspace','--locked','-j','2','--target-dir',str(primary/'target')]
env=os.environ.copy();env['PATH']=r'C:\Users\Lomi\AppData\Local\YokeValidation\tools\powershell-7.6.7-win-x64;C:\Users\Lomi\.cargo\bin;'+env['PATH']
commands=[('portable',base+['--test','meaning_projection_portable','--','--test-threads=1']),('regressions',base+['--no-fail-fast','--test','meaning_groups_admission','--test','meaning_join_cli','--test','meaning_cli','--test','meaning_archive_cli','--','--test-threads=1']),('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','-j','2','--target-dir',str(primary/'target'),'--','-D','warnings']),('format',[cargo,'fmt','--all','--','--check'])]
results=[]
for name,cmd in commands:
    print('START '+name,flush=True);start=time.time()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    results.append({'name':name,'command':cmd,'exitCode':p.returncode,'seconds':round(time.time()-start,2)})
    (out/'results.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results[-1]),flush=True)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
changed=[x for x in before if before[x]!=after[x]]
(out/'freeze.json').write_text(json.dumps({'inputs':len(before),'changed':changed,'allChecksPassed':all(x['exitCode']==0 for x in results),'allNewCapabilityChecksPassed':all(x['exitCode']==0 for x in results if x['name']!='regressions')},indent=2)+'\n')
print('FREEZE '+str(len(before))+' changed='+str(len(changed)),flush=True)
