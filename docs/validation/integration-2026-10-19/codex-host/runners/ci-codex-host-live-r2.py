from pathlib import Path
import subprocess,tempfile,json,hashlib,os,time
primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=primary.parent/'Yoke-CI-r1-attach';base=primary/'validation/integration-20261018/codex-host';out=base/'client-live-r2';out.mkdir(exist_ok=False);native=primary/'target/debug/yoke.exe';codex=Path(r'C:\Users\Lomi\AppData\Roaming\npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe');sha=lambda b:hashlib.sha256(b).hexdigest();commands=[]
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
before=inventory();binary=sha(native.read_bytes());(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
with tempfile.TemporaryDirectory(prefix='yoke-owned-codex-host-') as directory:
 root=Path(directory);home=root/('Codex home '+chr(0x1f4a1));source=root/('source '+chr(0x1f4a1));state=root/'state'
 for path in [home,source,state]:path.mkdir()
 original=('# human '+chr(0x1f4a1)+'\r\n[mcp_servers.human]\r\ncommand = "unrelated-host-tool"\r\nenabled = false\r\n').encode();config=home/'config.toml';config.write_bytes(original);source_bytes=('// source '+chr(0x1f4a1)+'\r\nfn alpha(){beta();beta();}\r\nfn beta(){}\r\n').encode();(source/'owned.rs').write_bytes(source_bytes);(out/'owned-fixture.rs').write_bytes(source_bytes);policy=root/'policy.json';policy.write_text(json.dumps({'projects':[{'name':'owned','root':str(source),'readOnly':True}],'modes':['planning','query-projects'],'notesPolicy':{'writable':False}}),encoding='utf-8')
 env={k:v for k,v in os.environ.items() if not k.upper().startswith(('CODEX_','OPENAI_','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY'))};env['CODEX_HOME']=str(home)
 def run(name,argv,stdin=None,success=True,timeout=60):
  command=[str(x) for x in argv];started=time.monotonic();p=subprocess.run(command,input=stdin,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=source,env=env,timeout=timeout,creationflags=subprocess.CREATE_NO_WINDOW);(out/(name+'.stdout')).write_bytes(p.stdout);(out/(name+'.stderr')).write_bytes(p.stderr);commands.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-started})
  if success is not None:assert (p.returncode==0)==success,(name,p.returncode,'see owned stderr')
  return p
 version=run('codex-version',[codex,'--version']).stdout.decode().strip();assert version=='codex-cli 0.163.0'
 selection=root/'selection.json';selection.write_text(json.dumps({'installation':'owned-fixture','targetRoots':[str(home)],'executable':str(native),'policy':str(policy),'stateRoot':str(state),'adapter':{'kind':'codex_mcp_v1','sourceRoot':str(source)}}),encoding='utf-8')
 cli=lambda name,args:run(name,[native,'--timeout','60000','--max-bytes','4000000','host-install',*args])
 planned=cli('plan',['plan','--selection',selection]);plan=json.loads(planned.stdout);assert plan['profile']=='codex-mcp-v1' and plan['files'][0]['path']=='config.toml';assert not (source/'.yoke').exists() and config.read_bytes()==original and list(state.iterdir())==[];plan_path=root/'approved.json';plan_path.write_bytes(planned.stdout)
 denied=run('missing-write-grant',[native,'host-install','apply','--selection',selection,'--plan',plan_path],success=False);assert b'allow-host-writes' in denied.stderr and config.read_bytes()==original
 cli('apply',['apply','--selection',selection,'--plan',plan_path,'--allow-host-writes']);installed=config.read_bytes();assert installed.startswith(original);(out/'installed-config.toml').write_bytes(installed)
 client=run('codex-get-installed',[codex,'mcp','get','yoke_owned-fixture','--json']);entry=json.loads(client.stdout);transport=entry['transport'];assert transport['type']=='stdio' and transport['command']==str(native) and transport['cwd']==str(source);assert entry['enabled'] is True
 argv=[transport['command'],*transport['args']];expected=['--root',str(source),'serve-session','--id','owned-fixture','--host-profile',str(policy),'--profile','native'];assert transport['args']==expected
 assert set(entry['enabled_tools'])=={'ci_find','ci_read','ci_trace','ci_context','ci_retrieve','ci_graph'} and not (source/'.yoke').exists()
 run('source-index',[native,'--root',source,'index']);requests=[{'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'yoke-owned-host-fixture','version':'0.5.0'}}},{'jsonrpc':'2.0','method':'notifications/initialized','params':{}},{'jsonrpc':'2.0','id':2,'method':'tools/list','params':{}},{'jsonrpc':'2.0','id':3,'method':'tools/call','params':{'name':'ci_find','arguments':{'query':'alpha'}}},{'jsonrpc':'2.0','id':4,'method':'tools/call','params':{'name':'ci_read','arguments':{'path':'owned.rs','startByte':0,'endByte':len(source_bytes)}}}]
 result=run('native-mcp-exact-client-argv',argv,stdin=b''.join(json.dumps(x).encode()+b'\n' for x in requests));replies={x['id']:x for x in [json.loads(line) for line in result.stdout.splitlines()] if 'id' in x};assert set(replies)=={1,2,3,4} and not any('error' in x for x in replies.values());assert set(entry['enabled_tools'])<={t['name'] for t in replies[2]['result']['tools']};assert 'alpha' in json.dumps(replies[3]) and not replies[3]['result'].get('isError')
 read=json.loads(replies[4]['result']['content'][0]['text']);assert not replies[4]['result'].get('isError')
 def strings(value):
  if isinstance(value,str):yield value
  elif isinstance(value,list):
   for item in value:yield from strings(item)
  elif isinstance(value,dict):
   for item in value.values():yield from strings(item)
 assert source_bytes.decode() in set(strings(read)),'Exact source byte response missing'
 import threading,queue,ctypes
 from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
 model_requests=[]
 class OfflineModel(BaseHTTPRequestHandler):
  def log_message(self,*args):pass
  def do_GET(self):
   model_requests.append({'method':'GET','path':self.path});body=b'{"object":"list","data":[]}';self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  def do_POST(self):
   model_requests.append({'method':'POST','path':self.path});self.send_response(503);self.send_header('Content-Length','0');self.end_headers()
 trap=ThreadingHTTPServer(('127.0.0.1',0),OfflineModel);trap_thread=threading.Thread(target=trap.serve_forever,daemon=True);trap_thread.start()
 kernel=ctypes.WinDLL('kernel32',use_last_error=True)
 class BasicLimit(ctypes.Structure):_fields_=[('processTime',ctypes.c_int64),('jobTime',ctypes.c_int64),('flags',ctypes.c_uint32),('minWorking',ctypes.c_size_t),('maxWorking',ctypes.c_size_t),('activeLimit',ctypes.c_uint32),('affinity',ctypes.c_size_t),('priority',ctypes.c_uint32),('scheduling',ctypes.c_uint32)]
 class ExtendedLimit(ctypes.Structure):_fields_=[('basic',BasicLimit),('io',ctypes.c_uint64*6),('processMemory',ctypes.c_size_t),('jobMemory',ctypes.c_size_t),('peakProcessMemory',ctypes.c_size_t),('peakJobMemory',ctypes.c_size_t)]
 assert ctypes.sizeof(ExtendedLimit)==144
 kernel.CreateJobObjectW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p];kernel.CreateJobObjectW.restype=ctypes.c_void_p
 kernel.SetInformationJobObject.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_uint32];kernel.SetInformationJobObject.restype=ctypes.c_int
 kernel.AssignProcessToJobObject.argtypes=[ctypes.c_void_p,ctypes.c_void_p];kernel.AssignProcessToJobObject.restype=ctypes.c_int
 kernel.QueryInformationJobObject.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_uint32,ctypes.c_void_p];kernel.QueryInformationJobObject.restype=ctypes.c_int
 kernel.TerminateJobObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32];kernel.TerminateJobObject.restype=ctypes.c_int
 kernel.CloseHandle.argtypes=[ctypes.c_void_p];kernel.CloseHandle.restype=ctypes.c_int
 job=kernel.CreateJobObjectW(None,None);assert job;limits=ExtendedLimit();limits.basic.flags=0x2000;assert kernel.SetInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits));messages=queue.Queue(512);transcript=[];app=None
 def active_members():
  info=ctypes.create_string_buffer(48);assert kernel.QueryInformationJobObject(job,1,info,48,None);return int.from_bytes(info.raw[40:44],'little')
 try:
  argv=[str(codex),'-c','model_provider="owned_offline"','-c','model="owned-model"','-c','model_providers.owned_offline.name="Owned offline fixture"','-c',f'model_providers.owned_offline.base_url="http://127.0.0.1:{trap.server_address[1]}/v1"','-c','model_providers.owned_offline.wire_api="responses"','app-server','--listen','stdio://']
  with (out/'appserver.stderr').open('wb') as app_log:
   app=subprocess.Popen(argv,cwd=source,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=app_log,creationflags=subprocess.CREATE_NO_WINDOW)
   assert kernel.AssignProcessToJobObject(job,int(app._handle)),'Owned app-server job assignment failed'
   def receive():
    try:
     total=0
     for _ in range(512):
      line=app.stdout.readline(4_000_001)
      if not line:messages.put(None);return
      total+=len(line);assert len(line)<=4_000_000 and total<=32_000_000;value=json.loads(line);transcript.append({'direction':'received','message':value});messages.put(value)
     messages.put({'fixtureError':'Owned app-server frame capacity exceeded'})
    except Exception as error:messages.put({'fixtureError':type(error).__name__})
   reader=threading.Thread(target=receive,daemon=True);reader.start()
   def rpc(method,params):
    request={'id':len([v for v in transcript if v['direction']=='sent'])+100,'method':method,'params':params};transcript.append({'direction':'sent','message':request});app.stdin.write(json.dumps(request).encode()+b'\n');app.stdin.flush();until=time.monotonic()+45
    while time.monotonic()<until:
     value=messages.get(timeout=max(.01,until-time.monotonic()));assert value is not None,'Owned app-server closed';assert 'fixtureError' not in value,value
     if value.get('id')==request['id']:assert 'error' not in value,(method,value.get('error'));return value['result']
     assert not ('id' in value and 'method' in value),'Unexpected permission/elicitation request in read-only fixture'
    raise TimeoutError('Owned app-server RPC deadline')
   rpc('initialize',{'clientInfo':{'name':'yoke-owned-mcp-fixture','title':'Owned Yoke fixture','version':'0.5.0'},'capabilities':{'experimentalApi':True}});app.stdin.write(b'{"method":"initialized"}\n');app.stdin.flush()
   started=rpc('thread/start',{'cwd':str(source),'model':'owned-model','modelProvider':'owned_offline','approvalPolicy':'never','sandbox':'read-only','ephemeral':True,'experimentalRawEvents':False,'baseInstructions':'','developerInstructions':''});thread_id=started['thread']['id']
   tool=rpc('mcpServer/tool/call',{'threadId':thread_id,'server':'yoke_owned-fixture','tool':'ci_read','arguments':{'path':'owned.rs','startByte':0,'endByte':len(source_bytes)}});assert not tool.get('isError');assert source_bytes.decode() in set(strings(json.loads(tool['content'][0]['text'])))
   status=rpc('mcpServerStatus/list',{'threadId':thread_id,'serverName':'yoke_owned-fixture','detail':'toolsAndAuthOnly'});(out/'appserver-mcp-status.json').write_text(json.dumps(status,indent=2)+'\n',encoding='utf-8')
   assert not any(v['method']=='POST' for v in model_requests),'A generation request occurred without a requested model turn'
   rpc('thread/unsubscribe',{'threadId':thread_id});app.stdin.close();app.wait(timeout=15);reader.join(timeout=3)
   appserver_result={'actualCodexMcpClientStartedConfiguredServerAndDispatchedRead':True,'ephemeralOwnedFixtureThread':True,'modelTurnsStarted':0,'modelGenerationRequests':0,'readExactUnicodeCrLfSource':True,'ownedAppserverPid':app.pid,'nativeBinarySha256':binary}
 finally:
  if app is not None and app.poll() is None:app.terminate();app.wait(timeout=10)
  remaining=active_members()
  if remaining:
   assert kernel.TerminateJobObject(job,23);until=time.monotonic()+5
   while active_members() and time.monotonic()<until:time.sleep(.01)
  stopped=active_members()==0;kernel.CloseHandle(job);trap.shutdown();trap.server_close();trap_thread.join(timeout=3)
  (out/'appserver-transcript.json').write_text(json.dumps(transcript,indent=2)+'\n',encoding='utf-8');(out/'offline-model-requests.json').write_text(json.dumps(model_requests,indent=2)+'\n',encoding='utf-8');(out/'appserver-cleanup.json').write_text(json.dumps({'ownedRootStopped':app is not None and app.poll() is not None,'remainingJobMembers':0 if stopped else None,'cleanupProven':stopped,'scopedJobTerminationNeeded':bool(remaining)})+'\n',encoding='utf-8');assert stopped
 appserver_result['ownedJobEmptyAfterCleanup']=True;(out/'appserver-result.json').write_text(json.dumps(appserver_result,indent=2)+'\n',encoding='utf-8')
 appendix=b'\r\n[mcp_servers.appended]\r\ncommand = "appended-host-tool"\r\n';config.write_bytes(installed+appendix);remove=cli('uninstall-plan',['uninstall-plan','--selection',selection]);remove_path=root/'uninstall.json';remove_path.write_bytes(remove.stdout);cli('uninstall-apply',['apply','--selection',selection,'--plan',remove_path,'--allow-host-writes']);remaining=config.read_bytes();(out/'remaining-config.toml').write_bytes(remaining);assert remaining.startswith(original) and remaining.endswith(appendix) and b'yoke_owned-fixture' not in remaining
 missing=run('codex-get-after-uninstall',[codex,'mcp','get','yoke_owned-fixture','--json'],success=False);assert b'yoke_owned-fixture' in missing.stderr
 result={'actualClient':version,'clientBinarySha256':sha(codex.read_bytes()),'nativeBinarySha256':binary,'isolatedCodexHome':True,'realHomeOrRealConfigModified':False,'installedCommandArgsAndCwdParsedByActualClient':True,'sixExplicitToolsParsedByClient':True,'nativeMcpExactConfiguredArgvHandshakeAndReads':True,'actualCodexModelTurnTested':False,'actualCodexMcpClientDispatchTested':True,'originalUnicodeCrLfSourceReadExact':True,'unownedHumanTablesAndAppendixPreserved':True,'clientEntryAbsentAfterUninstall':True,'planningAndClientInspectionCreatedNoSourceStore':True,'modelCalls':0,'projectCodeExecution':False,'configuredUnrelatedServersExecuted':False}
 (out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
(out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n',encoding='utf-8');after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');assert before==after and sha(native.read_bytes())==binary;(out/'frozen-inputs.json').write_text(json.dumps({'sourceUnchanged':True,'binaryUnchanged':True,'inputs':len(before),'binarySha256':binary})+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
