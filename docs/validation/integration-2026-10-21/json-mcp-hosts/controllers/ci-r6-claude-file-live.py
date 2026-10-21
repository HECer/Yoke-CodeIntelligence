from pathlib import Path
import ctypes,hashlib,json,msvcrt,os,subprocess,sys,tempfile,time,_winapi
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=w/'validation/integration-20261018/r6-json-mcp'/sys.argv[1];out.mkdir(exist_ok=False)
native=w/'target/debug/yoke.exe';client=Path(r'C:\Users\Lomi\AppData\Local\YokeValidation\tools\claude-2.1.306-win32-x64\claude.exe')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(client)=='f6bc9de38f507c36c9a3bb0085a59a27e32156e2208252bf1707c4e3ef309114'
jobroot=Path(tempfile.mkdtemp(prefix='yoke-owned-claude-mcp-'));source=jobroot/('source '+chr(0x1f30d));configroot=jobroot/('client '+chr(0x3c0));state=jobroot/'state';cwd=jobroot/'cwd'
for p in [source,configroot,state,cwd]:p.mkdir()
(cwd/'.env').write_bytes(b'');config=configroot/'.mcp.json';original=b'{\r\n  "mcpServers": {}\r\n}\r\n';config.write_bytes(original)
source_bytes=('// owned '+chr(0x1f30d)+'\r\nfn alpha(){beta();}\r\nfn beta(){}\r\n').encode();(source/'owned.rs').write_bytes(source_bytes)
policy=jobroot/'policy.json';policy.write_text(json.dumps({'projects':[{'name':'owned','root':str(source),'readOnly':True}],'modes':['planning','query-projects'],'notesPolicy':{'writable':False}}))
selection=jobroot/'selection.json';selection.write_text(json.dumps({'installation':'owned-json-fixture','targetRoots':[str(configroot)],'executable':str(native),'policy':str(policy),'stateRoot':str(state),'adapter':{'kind':'json_mcp_v1','client':'claude_code_explicit','sourceRoot':str(source)}}))
env={k:os.environ[k] for k in ['PATH','SystemRoot','SystemDrive','WINDIR','LANG'] if k in os.environ}
for key,name in [('HOME','home'),('USERPROFILE','home'),('APPDATA','roaming'),('LOCALAPPDATA','local'),('TEMP','tmp'),('TMP','tmp'),('CLAUDE_CONFIG_DIR','private-client-state')]:
    p=jobroot/name;p.mkdir(exist_ok=True);env[key]=str(p)
env.update({'DISABLE_TELEMETRY':'1','DISABLE_ERROR_REPORTING':'1','CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC':'1','DO_NOT_TRACK':'1','CI':'1','NO_COLOR':'1','MCP_TIMEOUT':'20000'})
(configroot/'.env').write_bytes(b'')
(Path(env['CLAUDE_CONFIG_DIR'])/'settings.json').write_text(json.dumps({'enabledMcpjsonServers':['yoke_owned-json-fixture']}))
k=ctypes.WinDLL('kernel32',use_last_error=True)
class Basic(ctypes.Structure):_fields_=[('processTime',ctypes.c_int64),('jobTime',ctypes.c_int64),('flags',ctypes.c_uint32),('minWorking',ctypes.c_size_t),('maxWorking',ctypes.c_size_t),('activeLimit',ctypes.c_uint32),('affinity',ctypes.c_size_t),('priority',ctypes.c_uint32),('scheduling',ctypes.c_uint32)]
class Extended(ctypes.Structure):_fields_=[('basic',Basic),('io',ctypes.c_uint64*6),('processMemory',ctypes.c_size_t),('jobMemory',ctypes.c_size_t),('peakProcessMemory',ctypes.c_size_t),('peakJobMemory',ctypes.c_size_t)]
for name,args,result in [('CreateJobObjectW',[ctypes.c_void_p,ctypes.c_wchar_p],ctypes.c_void_p),('SetInformationJobObject',[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_uint32],ctypes.c_int),('AssignProcessToJobObject',[ctypes.c_void_p,ctypes.c_void_p],ctypes.c_int),('QueryInformationJobObject',[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_uint32,ctypes.c_void_p],ctypes.c_int),('TerminateJobObject',[ctypes.c_void_p,ctypes.c_uint32],ctypes.c_int),('ResumeThread',[ctypes.c_void_p],ctypes.c_uint32),('CloseHandle',[ctypes.c_void_p],ctypes.c_int)]:
    f=getattr(k,name);f.argtypes=args;f.restype=result
assert ctypes.sizeof(Extended)==144
rows=[]
def run(name,args,expected=0,timeout=60):
    argv=[str(x) for x in args];job=k.CreateJobObjectW(None,None);assert job
    limits=Extended();limits.basic.flags=0x2000;assert k.SetInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits))
    hp=ht=None;start=time.monotonic();row={'name':name,'command':argv,'startedSuspended':True,'assignedBeforeResume':False}
    def active():
        info=ctypes.create_string_buffer(48);assert k.QueryInformationJobObject(job,1,info,48,None);return int.from_bytes(info.raw[40:44],'little')
    try:
        with open(os.devnull,'rb') as inp,(out/(name+'.stdout')).open('wb') as stdout,(out/(name+'.stderr')).open('wb') as stderr:
            handles=[msvcrt.get_osfhandle(f.fileno()) for f in [inp,stdout,stderr]]
            for handle in handles:os.set_handle_inheritable(handle,True)
            si=subprocess.STARTUPINFO();si.dwFlags=subprocess.STARTF_USESTDHANDLES;si.hStdInput,si.hStdOutput,si.hStdError=handles;si.lpAttributeList={'handle_list':handles}
            hp,ht,pid,tid=_winapi.CreateProcess(None,subprocess.list2cmdline(argv),None,None,True,subprocess.CREATE_NO_WINDOW|0x00000004,env,str(configroot if argv[0]==str(client) else cwd),si)
            row['pid']=pid;assert k.AssignProcessToJobObject(job,hp);row['assignedBeforeResume']=True;assert k.ResumeThread(ht)!=0xffffffff
            while _winapi.WaitForSingleObject(hp,100)==258:
                if time.monotonic()-start>timeout:raise TimeoutError(name)
                if sum((out/(name+suffix)).stat().st_size for suffix in ['.stdout','.stderr'])>16_000_000:raise RuntimeError('Owned output capacity')
            row['exitCode']=_winapi.GetExitCodeProcess(hp);row['activeAtParentExit']=active()
    finally:
        if hp is not None and not row['assignedBeforeResume']:_winapi.TerminateProcess(hp,124)
        assert k.TerminateJobObject(job,125);until=time.monotonic()+10
        while active() and time.monotonic()<until:time.sleep(.05)
        row['activeAfterCleanup']=active();row['seconds']=round(time.monotonic()-start,3)
        for handle in [ht,hp,job]:
            if handle is not None:k.CloseHandle(handle)
        rows.append(row);(out/'progress.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(row),flush=True)
    assert row['activeAfterCleanup']==0
    if expected is not None:assert row['exitCode']==expected,(name,row['exitCode'])
    return (out/(name+'.stdout')).read_bytes()
result={'fixtureRoot':str(jobroot),'cliSha256':sha(native),'clientSha256':sha(client),'clientVersion':'2.1.306','modelTurnRequested':False,'accountLoginRequested':False,'userConfigurationSelected':False}
try:
    plan=run('install-plan',[native,'--timeout','60000','host-install','plan','--selection',selection]);planfile=jobroot/'plan.json';planfile.write_bytes(plan)
    assert json.loads(plan)['profile']=='claude-explicit-mcp-v1' and config.read_bytes()==original and list(state.iterdir())==[] and not(source/'.yoke').exists()
    run('install-apply',[native,'--timeout','60000','host-install','apply','--selection',selection,'--plan',planfile,'--allow-host-writes'])
    installed=config.read_bytes();(out/'installed-config.json').write_bytes(installed);entry=json.loads(installed)['mcpServers']['yoke_owned-json-fixture'];assert entry['command']==str(native)
    run('index-owned-source',[native,'--root',source,'index'])
    flags=[client,'mcp']
    reply=run('actual-client-get',[*flags,'get','yoke_owned-json-fixture'],expected=None,timeout=45)
    result['clientGetExitCode']=rows[-1]['exitCode'];result['clientHealthConnected']=b'Connected' in reply and b'Failed to connect' not in reply
    run('actual-client-list',[*flags,'list'],expected=None,timeout=45)
    assert result['clientHealthConnected'], 'Actual private client entry health did not connect'
    result['healthGateUsesPrivateUserImport']=False
    result['healthGateLoadsManagedJsonFileFromOwnedCwd']=True
    result['onlyOwnedServerApprovedInPrivateSettings']=True
    result['explicitMcpConfigSessionValidated']=False
    unplan=run('uninstall-plan',[native,'--timeout','60000','host-install','uninstall-plan','--selection',selection]);unfile=jobroot/'remove.json';unfile.write_bytes(unplan)
    run('uninstall-apply',[native,'--timeout','60000','host-install','apply','--selection',selection,'--plan',unfile,'--allow-host-writes'])
    result['originalConfigBytesRestored']=config.read_bytes()==original;result['sourceBytesUnchanged']=(source/'owned.rs').read_bytes()==source_bytes;result['nativeExecutableUnchanged']=sha(native)==result['cliSha256'];result['completed']=True
except BaseException as error:
    result['errorType']=type(error).__name__;result['error']=str(error);result['completed']=False;raise
finally:
    result['commands']=rows;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({key:value for key,value in result.items() if key!='commands'}),flush=True)

