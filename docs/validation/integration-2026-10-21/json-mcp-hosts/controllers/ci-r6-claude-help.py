from pathlib import Path
import os,subprocess,tempfile,json,hashlib
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=w/'validation/integration-20261018/r6-json-mcp/client-help-r1';out.mkdir(exist_ok=False)
exe=Path(r'C:\Users\Lomi\AppData\Local\YokeValidation\tools\claude-2.1.306-win32-x64\claude.exe')
assert hashlib.sha256(exe.read_bytes()).hexdigest()=='f6bc9de38f507c36c9a3bb0085a59a27e32156e2208252bf1707c4e3ef309114'
job=Path(tempfile.mkdtemp(prefix='yoke-owned-claude-help-'));(job/'.env').write_bytes(b'')
env={k:os.environ[k] for k in ['PATH','SystemRoot','SystemDrive','WINDIR','LANG'] if k in os.environ}
for key,name in [('HOME','home'),('USERPROFILE','home'),('APPDATA','roaming'),('LOCALAPPDATA','local'),('TEMP','tmp'),('TMP','tmp'),('CLAUDE_CONFIG_DIR','config')]:
    p=job/name;p.mkdir(exist_ok=True);env[key]=str(p)
env.update({'DISABLE_TELEMETRY':'1','DISABLE_ERROR_REPORTING':'1','CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC':'1','DO_NOT_TRACK':'1','CI':'1','NO_COLOR':'1'})
rows=[]
for name,args in [('root',['--help']),('mcp',['mcp','--help']),('mcp-list',['mcp','list','--help']),('mcp-get',['mcp','get','--help'])]:
    p=subprocess.run([str(exe),*args],cwd=job,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,creationflags=0x08000000)
    (out/(name+'.stdout')).write_bytes(p.stdout);(out/(name+'.stderr')).write_bytes(p.stderr);rows.append({'name':name,'args':args,'exitCode':p.returncode})
(out/'result.json').write_text(json.dumps({'job':str(job),'commands':rows,'modelTurnRequested':False,'accountLoginRequested':False},indent=2)+'\n')
print(json.dumps(rows))
