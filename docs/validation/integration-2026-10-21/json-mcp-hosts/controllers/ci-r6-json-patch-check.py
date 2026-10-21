from pathlib import Path
import hashlib,json,os,re,subprocess,sys,time
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');target=w/'target';deps=(target/'debug/deps').resolve()
out=w/'validation/integration-20261018/r6-json-mcp'/sys.argv[1];out.mkdir(parents=True,exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def inventory():
    ns=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock'],cwd=w).decode().split('\0')
    return {n:sha(w/n) for n in sorted(set(ns)) if n and (w/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
cargo=str(Path.home()/'.cargo/bin/cargo.exe');records=[]
for name in ['host_mcp_config','host_config']:
    assert inventory()==before;existing={p.name.casefold() for p in deps.iterdir() if p.is_file()}
    command=[cargo,'test','--workspace','--locked','-j','2','--target-dir',str(target),'--test',name,'--','--test-threads=1'];start=time.monotonic()
    print('R6 PATCH CHECK',name,flush=True)
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=w,env=env,stdout=log,stderr=subprocess.STDOUT)
    raw=(out/(name+'.log')).read_text(encoding='utf-8',errors='replace');totals=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)
    record={'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start,'sourceUnchanged':inventory()==before,'totals':totals,'deletedNewArtifacts':[]}
    for path in re.findall(r'Running [^\r\n]*\(([^\r\n]+\.exe)\)',raw):
        exe=(w/Path(path)).resolve();assert exe.parent==deps
        for artifact in [exe,exe.with_suffix('.pdb')]:
            if artifact.is_file() and artifact.name.casefold() not in existing:
                record['deletedNewArtifacts'].append({'path':str(artifact),'sha256':sha(artifact)});artifact.unlink()
    records.append(record);print(json.dumps(record),flush=True)
    if p.returncode or not record['sourceUnchanged']:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
result={'checks':records,'sourceUnchanged':before==after,'allChecksPassed':len(records)==2 and all(r['exitCode']==0 for r in records) and before==after,
    'scope':'pure JSON MCP ownership patch and legacy host configuration tests','clientProfilesImplemented':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='checks'}),flush=True)
sys.exit(0 if result['allChecksPassed'] else 1)
