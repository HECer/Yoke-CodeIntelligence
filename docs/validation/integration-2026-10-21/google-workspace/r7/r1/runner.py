"""Scoped R7 checks, only after the complete frozen baseline is terminal."""
from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,sys,time
primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=primary.parent/'Yoke-CI-r7-google'
baseline=primary/'validation/integration-20261018/full-native-default-20261021-r1/result.json'
assert baseline.is_file(), 'Baseline is still running; no concurrent Cargo'
b=json.loads(baseline.read_text()); assert b['allRegisteredGroupsAttempted'] and b['sourceUnchanged']
phase=sys.argv[1]
out=work/'validation/google-workspace'/phase; out.mkdir(parents=True,exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def inventory():
    names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','package.json','package-lock.json','src','test','workers','scripts'],cwd=work).decode().split('\0')
    return {n:sha(work/n) for n in sorted(set(names)) if n and (work/n).is_file()}
before=inventory(); (out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
shutil.copyfile(__file__,out/'runner.py')
env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
cargo=str(Path.home()/'.cargo/bin/cargo.exe');target=primary/'target';deps=(target/'debug/deps').resolve()
commands=[('fmt',[cargo,'fmt','--all','--','--check']),
('units',[cargo,'test','--workspace','--locked','-j','2','--target-dir',str(target),'--lib','google_workspace::','--','--test-threads=1'])]
for name in ['google_workspace_cli','google_workspace_process','source_acquisition_cli','source_acquisition_http','source_acquisition','arxiv_source']:
    commands.append((name,[cargo,'test','--workspace','--locked','-j','2','--target-dir',str(target),'--test',name,'--','--test-threads=1']))
commands.append(('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','-j','2','--target-dir',str(target),'--','-D','warnings']))
records=[]
for name,command in commands:
    assert inventory()==before, 'R7 source changed during validation'
    assert shutil.disk_usage(target).free>700*1024*1024
    print('R7 CHECK',name,flush=True); start=time.monotonic()
    existing={p.name.casefold() for p in deps.iterdir() if p.is_file()}
    with (out/(name+'.log')).open('wb') as log:
        process=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
    raw=(out/(name+'.log')).read_text(encoding='utf-8',errors='replace')
    totals=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)
    record={'name':name,'command':command,'exitCode':process.returncode,'seconds':time.monotonic()-start,
        'sourceUnchanged':inventory()==before,'totals':{key:sum(int(g[i]) for g in totals) for i,key in enumerate(['passed','failed','ignored'])},'deletedNewTestArtifacts':[]}
    # Only remove newly created test executables explicitly named in Cargo output,
    # and their new PDB; preserve every preexisting artifact and the CLI executable.
    for raw_path in re.findall(r'Running [^\r\n]*\(([^\r\n]+\.exe)\)',raw):
        executable=Path(raw_path).resolve()
        assert executable.parent==deps
        for artifact in [executable,executable.with_suffix('.pdb')]:
            if artifact.is_file() and artifact.name.casefold() not in existing:
                info={'path':str(artifact),'bytes':artifact.stat().st_size,'sha256':sha(artifact)}
                artifact.unlink();record['deletedNewTestArtifacts'].append(info)
    records.append(record);print(json.dumps(record),flush=True)
    (out/'progress.json').write_text(json.dumps(records,indent=2)+'\n')
    if process.returncode or not record['sourceUnchanged']:break
after=inventory(); (out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
result={'scope':'R7 Google export and existing source acquisition regressions','sourceUnchanged':before==after,
    'allChecksPassed':len(records)==len(commands) and all(r['exitCode']==0 and r['sourceUnchanged'] for r in records),
    'checks':records,'fullWindowsCertified':False,'liveGoogleExportValidated':False,'resourceKeyTransportAvailable':False}
if (target/'debug/yoke.exe').is_file():result['cliSha256']=sha(target/'debug/yoke.exe')
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
sys.exit(0 if result['allChecksPassed'] else 1)
