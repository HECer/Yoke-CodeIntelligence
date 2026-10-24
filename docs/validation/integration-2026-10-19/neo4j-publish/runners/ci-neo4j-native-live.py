from pathlib import Path
import tempfile,subprocess,json,os,socket,time,ssl,urllib.request,urllib.error,base64,secrets,hashlib,ctypes,shutil,concurrent.futures
primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/neo4j-publish/native-live-r1';out.mkdir(parents=True,exist_ok=False);home=Path.home()/'AppData/Local/YokeValidation/neo4j-2026.09.0/neo4j-community-2026.09.0';jhome=Path(r'C:\Program Files\Android\Android Studio\jbr');java=jhome/'bin/java.exe';hidden=subprocess.CREATE_NO_WINDOW
env={k:v for k,v in os.environ.items() if not k.upper().startswith(('NEO4J','JAVA','JDK','_JAVA','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY'))};env.update({'JAVA_HOME':str(jhome),'NEO4J_HOME':str(home)})
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}));password=secrets.token_urlsafe(32);auth='Basic '+base64.b64encode(('neo4j:'+password).encode()).decode()
def run(args,timeout=40):return subprocess.run([str(a) for a in args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=timeout,creationflags=hidden)
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def parse_windows(command):
 shell=ctypes.WinDLL('shell32');kernel=ctypes.WinDLL('kernel32');count=ctypes.c_int();shell.CommandLineToArgvW.argtypes=[ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_int)];shell.CommandLineToArgvW.restype=ctypes.POINTER(ctypes.c_wchar_p);kernel.LocalFree.argtypes=[ctypes.c_void_p];kernel.LocalFree.restype=ctypes.c_void_p
 values=shell.CommandLineToArgvW(command,ctypes.byref(count));assert values
 try:return [values[i] for i in range(count.value)]
 finally:kernel.LocalFree(values)
def query(name,statement,parameters=None,success=True):
 data=json.dumps({'statement':'CYPHER 5 '+statement,'parameters':parameters or {}},ensure_ascii=False).encode();request=urllib.request.Request(endpoint,data=data,headers={'Authorization':auth,'Content-Type':'application/json','Accept':'application/json'},method='POST')
 try:
  with opener.open(request,timeout=20) as response:status=response.status;body=response.read(128000)
 except urllib.error.HTTPError as error:status=error.code;body=error.read(128000)
 assert password.encode() not in body and auth.encode() not in body
 (out/(name+'.json')).write_bytes(body);value=json.loads(body);assert status==202,(name,status);assert (not value.get('errors'))==success,(name,value.get('errors'));return value
with tempfile.TemporaryDirectory(prefix='yoke-owned-neo4j-baseline-') as folder:
 root=Path(folder);conf=root/'conf';conf.mkdir();env['NEO4J_CONF']=str(conf)
 for name in ['server-logs.xml','user-logs.xml']:
  if (home/'conf'/name).exists():shutil.copyfile(home/'conf'/name,conf/name)
 http=port();bolt=port();assert http!=bolt
 settings={'server.directories.data':root/'data','server.directories.logs':root/'logs','server.directories.run':root/'run','server.directories.import':root/'import','server.directories.plugins':root/'plugins','server.directories.transaction.logs.root':root/'transactions','server.default_listen_address':'127.0.0.1','server.default_advertised_address':'localhost','server.http.enabled':'true','server.http.listen_address':f'127.0.0.1:{http}','server.http.advertised_address':f'localhost:{http}','server.bolt.enabled':'true','server.bolt.listen_address':f'127.0.0.1:{bolt}','server.bolt.advertised_address':f'localhost:{bolt}','server.https.enabled':'false','dbms.security.auth_enabled':'true','server.memory.heap.initial_size':'256m','server.memory.heap.max_size':'512m','server.memory.pagecache.size':'64m','db.tx_log.preallocate':'false','db.tx_log.rotation.size':'16m','db.tx_log.rotation.retention_policy':'1 files','dbms.usage_report.enabled':'false','server.fleet_discovery.enabled':'false','server.http_enabled_modules':'QUERY_API_ENDPOINTS','db.query.default_language':'CYPHER_5'}
 for value in ['data','logs','run','import','plugins','transactions']:(root/value).mkdir(exist_ok=True)
 (conf/'neo4j.conf').write_text('\n'.join(k+'='+str(v).replace('\\','/') for k,v in settings.items())+'\n',encoding='utf-8')
 # Fixture-only fresh random password, passed directly to a private child argv:
 # no shell/history, no literal value in source, request logs or public receipt.
 admin=run(['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',home/'bin/neo4j-admin.ps1','dbms','set-initial-password',password]);assert password.encode() not in admin.stdout+admin.stderr
 (out/'admin.log').write_bytes(admin.stdout+admin.stderr);assert admin.returncode==0,'Owned fixture initial authentication setup failed'
 dry=run(['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',home/'bin/neo4j.ps1','console','--dry-run']);(out/'dry-run.stdout').write_bytes(dry.stdout);(out/'dry-run.stderr').write_bytes(dry.stderr);assert dry.returncode==0
 line=next(v for v in dry.stdout.decode().splitlines() if 'org.neo4j.server.Neo4jCommunity' in v);args=parse_windows(line);assert Path(args[0])==java and 'org.neo4j.server.Neo4jCommunity' in args
 (out/'public-java-command.json').write_text(json.dumps(args,indent=2)+'\n',encoding='utf-8');endpoint=f'http://127.0.0.1:{http}/db/neo4j/query/v2'
 with (out/'server.log').open('wb') as log:
  child=subprocess.Popen(args,cwd=home,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,creationflags=hidden)
  try:
   print('STARTED OWNED NEO4J',child.pid,flush=True);ready=False
   for _ in range(240):
    if child.poll() is not None:raise RuntimeError('Owned Neo4j exited during startup; see bounded local log')
    try:
     with socket.create_connection(('127.0.0.1',http),timeout=.2):ready=True;break
    except OSError:time.sleep(.25)
   assert ready,'Owned Neo4j startup unavailable'
   owner=run(['powershell.exe','-NoProfile','-NonInteractive','-Command',f'Get-NetTCPConnection -LocalPort {http} -State Listen | Select-Object -ExpandProperty OwningProcess']);assert child.pid in [int(v) for v in owner.stdout.decode().split()];(out/'listener-owner.txt').write_bytes(owner.stdout)
   import threading
   from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
   native=primary/'target/debug/yoke.exe';owned_source=root/'owned-source';owned_source.mkdir()
   (owned_source/'owned.rs').write_bytes(('// owned '+chr(0x1f4a1)+'\r\nfn alpha(){beta();beta();missing();}\r\nfn beta(){alpha();}\r\n').encode())
   commands=[];created_credentials=[];proxy=None;proxy_thread=None;proxy_events=[];drop_next={'armed':False}
   def cli(name,args,secret_input=None,lost_stdout=False,success=True):
    command=[str(native),'--timeout','60000','--max-bytes','200000']+[str(a) for a in args]
    start=time.monotonic()
    if lost_stdout:
     p=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,creationflags=hidden)
     p.stdout.close();p.stdout=None;_,stderr=p.communicate(timeout=75);stdout=b'';code=p.returncode
    else:
     p=subprocess.run(command,input=secret_input,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=75,creationflags=hidden);stdout=p.stdout;stderr=p.stderr;code=p.returncode
    assert password.encode() not in stdout+stderr and auth.encode() not in stdout+stderr
    (out/(name+'.stdout')).write_bytes(stdout);(out/(name+'.stderr')).write_bytes(stderr)
    commands.append({'name':name,'command':command,'exitCode':code,'seconds':time.monotonic()-start,'privateStdinUsed':secret_input is not None,'stdoutDiscarded':lost_stdout})
    assert (code==0)==success,(name,code,'see bounded local stderr')
    return json.loads(stdout) if code==0 else stderr.decode(errors='replace')
   class Proxy(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_POST(self):
     length=int(self.headers.get('Content-Length','0'));assert 0<length<=32_000_000
     body=self.rfile.read(length);request=json.loads(body);statement=request['statement']
     forwarded=urllib.request.Request(endpoint,data=body,headers={'Authorization':self.headers['Authorization'],'Content-Type':'application/json','Accept':'application/json'},method='POST')
     try:
      with opener.open(forwarded,timeout=30) as upstream:status=upstream.status;data=upstream.read(32_000_001)
     except urllib.error.HTTPError as error:status=error.code;data=error.read(32_000_001)
     assert len(data)<=32_000_000 and password.encode() not in data and auth.encode() not in data
     is_publication=statement.startswith('CYPHER 5 MATCH (scope:YokeNativeScope')
     dropped=is_publication and drop_next['armed'];proxy_events.append({'publication':is_publication,'status':status,'responseHash':hashlib.sha256(data).hexdigest(),'droppedAfterUpstreamResponse':dropped})
     if dropped:
      drop_next['armed']=False;assert status==202 and not json.loads(data).get('errors')
      try:self.connection.shutdown(socket.SHUT_RDWR)
      except OSError:pass
      self.connection.close();self.close_connection=True;return
     self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.send_header('Connection','close');self.end_headers();self.wfile.write(data)
   def workflow(prefix,selected_port,lose_response=False):
    folder=out/prefix;folder.mkdir();target={'baseUrl':f'http://127.0.0.1:{selected_port}/','address':f'127.0.0.1:{selected_port}','database':'neo4j','namespace':'native owned '+chr(0x1f4a1),'user':'neo4j','useSystemRoots':False};config=folder/'target.json';config.write_text(json.dumps(target),encoding='utf-8')
    bootstrap_path=folder/'bootstrap.json';cli(prefix+'-bootstrap-plan',['neo4j-publish','bootstrap-plan','--config',config,'--output',bootstrap_path,'--allow-output']);bootstrap=json.loads(bootstrap_path.read_bytes())
    inspection=cli(prefix+'-offline-inspect',['neo4j-publish','inspect','--plan',bootstrap_path]);binding=inspection['target']['credentialBinding']
    cli(prefix+'-credential-import',['neo4j-publish','import-password','--config',config,'--allow-credential-create'],password.encode());created_credentials.append('yoke-code-intelligence/service/neo4j/'+binding)
    connect=['--allow-network','--allow-credential-lookup','--allow-loopback-http']
    bj=folder/('neo4j-'+bootstrap['planId']+'.jsonl')
    cli(prefix+'-bootstrap-approve',['neo4j-publish','approve','--plan',bootstrap_path,'--expected-plan-id',bootstrap['planId'],'--journal',bj,'--allow-approval','--allow-journal'])
    denied=cli(prefix+'-schema-gate',['neo4j-publish','apply','--plan',bootstrap_path,'--expected-plan-id',bootstrap['planId'],'--journal',bj,'--allow-write','--allow-journal']+connect,success=False);assert 'schema_permission_required' in denied;assert len(bj.read_text().splitlines())==1
    cli(prefix+'-bootstrap-apply',['neo4j-publish','apply','--plan',bootstrap_path,'--expected-plan-id',bootstrap['planId'],'--journal',bj,'--allow-write','--allow-schema','--allow-journal']+connect)
    query(prefix+'-await-indexes','CALL db.awaitIndexes(30)')
    probe=cli(prefix+'-probe',['neo4j-publish','probe','--plan',bootstrap_path]+connect);assert probe['schema']['allRequiredPresent'] and probe['schema']['allObservedIndexesOnline']
    publication_path=folder/'publication.json';cli(prefix+'-prepare',['neo4j-publish','prepare','--config',config,'--source-root',owned_source,'--snapshot-id',snapshot_id,'--expected-revision','absent','--output',publication_path,'--allow-output','--allow-source-coordination']);publication=json.loads(publication_path.read_bytes());pj=folder/('neo4j-'+publication['planId']+'.jsonl')
    cli(prefix+'-publish-approve',['neo4j-publish','approve','--plan',publication_path,'--expected-plan-id',publication['planId'],'--journal',pj,'--allow-approval','--allow-journal'])
    apply_args=['neo4j-publish','apply','--plan',publication_path,'--expected-plan-id',publication['planId'],'--journal',pj,'--allow-write','--allow-journal']+connect
    if lose_response:
     before=len(proxy_events);drop_next['armed']=True;failure=cli(prefix+'-commit-response-lost',apply_args,success=False);assert 'transport_failed' in failure and publication['planId'] in failure
     assert sum(e['publication'] for e in proxy_events[before:])==1 and sum(e['droppedAfterUpstreamResponse'] for e in proxy_events)==1
     journal=cli(prefix+'-pending-journal',['neo4j-publish','journal','--plan',publication_path,'--journal',pj,'--allow-journal']);assert journal['pendingOperation']=='publish' and journal['completedOperations']==0
     count=len(proxy_events);blocked=cli(prefix+'-pending-resend-blocked',apply_args,success=False);assert 'previous_result_unknown_recovery_required' in blocked and len(proxy_events)==count
     count=len(proxy_events);cli(prefix+'-recover',['neo4j-publish','recover','--plan',publication_path,'--expected-plan-id',publication['planId'],'--journal',pj,'--allow-recovery','--allow-journal']+connect);assert not any(e['publication'] for e in proxy_events[count:])
    else:cli(prefix+'-publish-apply',apply_args)
    count=len(proxy_events);cli(prefix+'-already-confirmed-no-resend',apply_args);assert len(proxy_events)==count
    journal=cli(prefix+'-confirmed-journal',['neo4j-publish','journal','--plan',publication_path,'--journal',pj,'--allow-journal']);assert journal['pendingOperation'] is None and journal['completedOperations']==1
    scope=publication['scopeKey'];state=query(prefix+'-verified-current','MATCH (s:YokeNativeScope {key:$key}) RETURN s.activeRevision,s.lockCounter,COUNT { MATCH (n:YokeNativeElement {scopeKey:$key}) },COUNT { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$key}]->() }',{'key':scope});assert state['data']['values'][0][0]==publication['revisionId'] and state['data']['values'][0][1]==1
    if not lose_response:
     lost=folder/'lost-stdout';lost.mkdir();lj=lost/('neo4j-'+publication['planId']+'.jsonl');cli(prefix+'-lost-stdout-approve',['neo4j-publish','approve','--plan',publication_path,'--expected-plan-id',publication['planId'],'--journal',lj,'--allow-approval','--allow-journal']);lostargs=['neo4j-publish','apply','--plan',publication_path,'--expected-plan-id',publication['planId'],'--journal',lj,'--allow-write','--allow-journal']+connect
     failure=cli(prefix+'-lost-stdout-apply',lostargs,lost_stdout=True,success=False);assert 'delivery_failed_after_effect' in failure and publication['planId'] in failure
     done=cli(prefix+'-lost-stdout-journal',['neo4j-publish','journal','--plan',publication_path,'--journal',lj,'--allow-journal']);assert done['completedOperations']==1 and done['pendingOperation'] is None
    return {'scopeKey':scope,'planId':publication['planId'],'revisionId':publication['revisionId'],'completedOperations':journal['completedOperations']}
   try:
    indexed=cli('native-index-owned-fixture',['--root',owned_source,'index']);snapshot_id=indexed['snapshotId']
    direct=workflow('direct-native',http)
    proxy=ThreadingHTTPServer(('127.0.0.1',0),Proxy);proxy.daemon_threads=True;proxy_thread=threading.Thread(target=proxy.serve_forever,daemon=True);proxy_thread.start()
    recovered=workflow('response-loss-native',proxy.server_address[1],True)
    result={'actualServer':'Neo4j Community2026.09.0','nativeCliTransport':True,'fixedSchemaAndNamespaceBootstrap':True,'separateSchemaPermissionEnforced':True,'existingSnapshotUsed':True,'osCredentialBindingUsed':True,'sameJournalConfirmedOperationNotResent':True,'lostStdoutConfirmedJournalRetained':True,'responseLostAfterActualCommitLeavesPending':True,'pendingApplyBlockedWithoutRequest':True,'recoveryOnlyReadsAndNoWriteReplay':True,'direct':direct,'recovered':recovered,'nativeBinarySha256':hashlib.sha256(native.read_bytes()).hexdigest(),'fleetDiscoveryConfiguredDisabled':True,'ownedPid':child.pid,'listenerPidVerified':True,'java':'JBR25.0.3','vendorJvmQualificationClaimed':False,'nativeTlsTested':False}
   finally:
    if proxy is not None:proxy.shutdown();proxy.server_close();proxy_thread.join(timeout=3)
    removed=0;advapi=ctypes.WinDLL('Advapi32',use_last_error=True);advapi.CredDeleteW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32];advapi.CredDeleteW.restype=ctypes.c_int
    for name in created_credentials:assert advapi.CredDeleteW(name,1,0),'Owned fixture credential removal failed';removed+=1
    (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n',encoding='utf-8');(out/'proxy-events.json').write_text(json.dumps(proxy_events,indent=2)+'\n',encoding='utf-8');(out/'credential-cleanup.json').write_text(json.dumps({'created':len(created_credentials),'removed':removed})+'\n')
   result['ownedCredentialsRemoved']=removed

  finally:
   if child.poll() is None:child.terminate()
   try:child.wait(timeout=15)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   result_stop=child.poll() is not None
for artifact in out.rglob('*'):
 if artifact.is_file():assert password.encode() not in artifact.read_bytes() and auth.encode() not in artifact.read_bytes(),'Private fixture authentication leaked'
result['ownedServerStopped']=result_stop;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
