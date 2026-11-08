from pathlib import Path
import urllib.request,hashlib,json,re,os,subprocess,tempfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=w/'validation/integration-20261018/r6-json-mcp/client-prep-r1';out.mkdir(parents=True,exist_ok=False)
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
base='https://downloads.claude.ai/claude-code-releases'
def get(url,cap):
    with opener.open(url,timeout=45) as response:
        assert response.geturl().startswith(base+'/')
        data=response.read(cap+1);assert len(data)<=cap;return data
version_bytes=get(base+'/latest',128);version=version_bytes.decode().strip();assert re.fullmatch(r'\d+\.\d+\.\d+',version)
manifest_bytes=get(base+'/'+version+'/manifest.json',1000000);manifest=json.loads(manifest_bytes)
entry=manifest['platforms']['win32-x64'];expected=entry['checksum'];assert re.fullmatch('[a-fA-F0-9]{64}',expected)
(out/'version.txt').write_bytes(version_bytes);(out/'manifest.json').write_bytes(manifest_bytes)
tool=Path.home()/'AppData/Local/YokeValidation/tools'/('claude-'+version+'-win32-x64');tool.mkdir(exist_ok=False)
program=tool/'claude.exe';url=base+'/'+version+'/win32-x64/claude.exe'
digest=hashlib.sha256();count=0
with opener.open(url,timeout=45) as response,program.open('xb') as f:
    assert response.geturl().startswith(base+'/')
    while chunk:=response.read(1_048_576):
        count+=len(chunk);assert count<=512_000_000;digest.update(chunk);f.write(chunk)
assert digest.hexdigest()==expected.lower()
with tempfile.TemporaryDirectory(prefix='yoke-owned-claude-version-') as raw:
    job=Path(raw);config=job/'private-client-state';config.mkdir();(job/'.env').write_bytes(b'')
    env={k:os.environ[k] for k in ['PATH','SystemRoot','SystemDrive','WINDIR','USERPROFILE','HOME','TEMP','TMP','LANG'] if k in os.environ}
    env.update({'CLAUDE_CONFIG_DIR':str(config),'DISABLE_TELEMETRY':'1','DISABLE_ERROR_REPORTING':'1','CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC':'1','DO_NOT_TRACK':'1','CI':'1','NO_COLOR':'1'})
    p=subprocess.run([str(program),'--version'],cwd=job,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,creationflags=0x08000000)
    (out/'version.stdout').write_bytes(p.stdout);(out/'version.stderr').write_bytes(p.stderr)
    assert p.returncode==0 and version.encode() in p.stdout
result={'url':url,'version':version,'manifestSha256':hashlib.sha256(manifest_bytes).hexdigest(),'executable':str(program),
    'executableBytes':count,'executableSha256':digest.hexdigest(),'manifestChecksumMatched':True,'versionExitCode':p.returncode,
    'installerExecuted':False,'pathOrShellIntegrationChanged':False,'modelTurnRequested':False,'accountLoginRequested':False,
    'clientMcpConnectionValidated':False,'isolatedConfigDirectory':True}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
