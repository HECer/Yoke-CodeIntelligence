from pathlib import Path
import subprocess,json,hashlib,time,shutil,re,os,sys
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
root=work.parent/'Yoke-CodeIntelligence'
phase=sys.argv[1]
out=root/'validation/integration-20261018/plan-feedback'/phase
out.mkdir(parents=True,exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest()
def inventory():
    names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
    return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
before=inventory()
(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
    if any(s in name for s in ['plan_feedback','knowledge_feedback','feedback_cli','retention.rs','outcome-feedback.md']):
        target=out/'source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,target)
shutil.copyfile(__file__,out/'runner.py')
cargo=str(Path.home()/'.cargo/bin/cargo.exe')
common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')]
command=[cargo,'test',*common,'--no-fail-fast']
for name in sys.argv[2:] or ['plan_feedback']:
    command+=['--lib'] if name=='lib' else ['--test',name]
command+=['--','--test-threads=1']
env=os.environ.copy()
env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
runs=[]
commands=[('native',command)]
if phase=='final':
    commands += [('format',[cargo,'fmt','--all','--check']),('build',[cargo,'build',*common,'--bin','yoke']),('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings'])]
for name,command in commands:
    print('START R9',phase,name,flush=True);start=time.monotonic()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
    record={'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start};runs.append(record)
    if name=='native':
        log=(out/'native.log').read_text(encoding='utf-8',errors='replace')
        groups=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log,re.M)
        record['totals']={k:sum(int(g[i]) for g in groups) for i,k in enumerate(['passed','failed','ignored'])}
        record['groups']=len(groups)
        record['failures']=re.findall(r'^test (\S+) \.\.\. FAILED$',log,re.M)
    # Keep a known failed native prerequisite visible while independently
    # completing format/build/Clippy; the aggregate remains failed.
    if p.returncode and not (phase=='final' and name=='native'):break
after=inventory()
(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8')
result={'sourceUnchanged':before==after,'inputs':len(before),'commands':runs,'allChecksPassed':all(x['exitCode']==0 for x in runs),'wholeNativeRerun':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
