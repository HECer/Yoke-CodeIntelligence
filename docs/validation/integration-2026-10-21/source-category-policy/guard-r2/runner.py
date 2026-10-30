from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,sys,time
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');target=w/'target';deps=(target/'debug/deps').resolve()
phase=sys.argv[1];out=w/'validation/integration-20261018/r3-source-categories'/phase;out.mkdir(parents=True,exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def inventory():
    names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','workers','src','test','scripts','Cargo.toml','Cargo.lock','package.json','package-lock.json'],cwd=w).decode().split('\0')
    return {n:sha(w/n) for n in sorted(set(names)) if n and (w/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n');shutil.copyfile(__file__,out/'runner.py')
env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
cargo=str(Path.home()/'.cargo/bin/cargo.exe')
commands=[('fmt',[cargo,'fmt','--all','--','--check'])]
for n in ['graph_analysis_policy','graph_extensions','selected_view_analysis','selected_view_analysis_review','view_comparison_cli','graphify_adapter','graph_view']:
    commands.append((n,[cargo,'test','--workspace','--locked','-j','2','--target-dir',str(target),'--test',n,'--','--test-threads=1']))
commands.extend([('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','-j','2','--target-dir',str(target),'--','-D','warnings']),('typecheck',['npm.cmd','run','typecheck']),('retained',['npm.cmd','test'])])
records=[]
for name,command in commands:
    assert inventory()==before
    assert shutil.disk_usage(target).free>700*1024*1024
    existing={p.name.casefold() for p in deps.iterdir() if p.is_file()};start=time.monotonic();print('R3 CHECK',name,flush=True)
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=w,env=env,stdout=log,stderr=subprocess.STDOUT)
    raw=(out/(name+'.log')).read_text(encoding='utf-8',errors='replace');totals=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)
    record={'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start,'sourceUnchanged':inventory()==before,
        'totals':{k:sum(int(g[i]) for g in totals) for i,k in enumerate(['passed','failed','ignored'])},'deletedNewTestArtifacts':[]}
    for path in re.findall(r'Running [^\r\n]*\(([^\r\n]+\.exe)\)',raw):
        exe=Path(path).resolve();assert exe.parent==deps
        for f in [exe,exe.with_suffix('.pdb')]:
            if f.is_file() and f.name.casefold() not in existing:
                record['deletedNewTestArtifacts'].append({'path':str(f),'sha256':sha(f),'bytes':f.stat().st_size});f.unlink()
    records.append(record);(out/'progress.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps(record),flush=True)
    if p.returncode or not record['sourceUnchanged']:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
result={'sourceUnchanged':before==after,'checks':records,'allChecksPassed':len(records)==len(commands) and all(r['exitCode']==0 and r['sourceUnchanged'] for r in records),
    'scope':'Source-category analysis policy plus existing graph/view contracts','fullStatusCertified':False}
if (target/'debug/yoke.exe').is_file():result['cliSha256']=sha(target/'debug/yoke.exe')
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='checks'}),flush=True)
sys.exit(0 if result['allChecksPassed'] else 1)
