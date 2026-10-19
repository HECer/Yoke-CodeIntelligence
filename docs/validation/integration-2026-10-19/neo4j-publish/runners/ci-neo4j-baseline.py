from pathlib import Path
import tempfile,subprocess,json,os,socket,time,ssl,urllib.request,urllib.error,base64,secrets,hashlib,ctypes,shutil,concurrent.futures
primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/neo4j-publish/baseline';out.mkdir(parents=True,exist_ok=False);home=Path.home()/'AppData/Local/YokeValidation/neo4j-2026.09.0/neo4j-community-2026.09.0';jhome=Path(r'C:\Program Files\Android\Android Studio\jbr');java=jhome/'bin/java.exe';hidden=subprocess.CREATE_NO_WINDOW
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
 settings={'server.directories.data':root/'data','server.directories.logs':root/'logs','server.directories.run':root/'run','server.directories.import':root/'import','server.directories.plugins':root/'plugins','server.directories.transaction.logs.root':root/'transactions','server.default_listen_address':'127.0.0.1','server.default_advertised_address':'localhost','server.http.enabled':'true','server.http.listen_address':f'127.0.0.1:{http}','server.http.advertised_address':f'localhost:{http}','server.bolt.enabled':'true','server.bolt.listen_address':f'127.0.0.1:{bolt}','server.bolt.advertised_address':f'localhost:{bolt}','server.https.enabled':'false','dbms.security.auth_enabled':'true','server.memory.heap.initial_size':'256m','server.memory.heap.max_size':'512m','server.memory.pagecache.size':'64m','db.tx_log.preallocate':'false','db.tx_log.rotation.size':'16m','db.tx_log.rotation.retention_policy':'1 files','dbms.usage_report.enabled':'false','server.http_enabled_modules':'QUERY_API_ENDPOINTS','db.query.default_language':'CYPHER_5'}
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
   marker='owned-'+secrets.token_hex(16);echo=query('unicode-query','RETURN $text AS owned',{'text':marker+' '+chr(0x1f4a1)});assert echo['data']['values'][0][0]==marker+' '+chr(0x1f4a1)
   query('schema','CREATE CONSTRAINT yoke_native_scope_key_v1 IF NOT EXISTS FOR (s:YokeNativeScope) REQUIRE s.key IS UNIQUE')
   scope='owned-scope-'+secrets.token_hex(16);query('namespace','CREATE (s:YokeNativeScope {key:$key, activeRevision:"absent", lockCounter:0}) RETURN s.key AS key',{'key':scope})
   statement='MATCH (s:YokeNativeScope {key:$key}) SET s.lockCounter=s.lockCounter+1 WITH s FOREACH (deny IN CASE WHEN s.activeRevision=$expected OR s.activeRevision=$revision THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$key})) SET s.activeRevision=$revision RETURN s.activeRevision AS revision, s.lockCounter AS counter'
   first=query('first-cas',statement,{'key':scope,'expected':'absent','revision':'revision-one'});assert first['data']['values']==[['revision-one',1]]
   repeat=query('idempotent-cas',statement,{'key':scope,'expected':'absent','revision':'revision-one'});assert repeat['data']['values']==[['revision-one',2]]
   query('stale-cas',statement,{'key':scope,'expected':'wrong','revision':'revision-two'},False)
   unchanged=query('stale-state-unchanged','MATCH (s:YokeNativeScope {key:$key}) RETURN s.activeRevision AS revision,s.lockCounter AS counter',{'key':scope});assert unchanged['data']['values']==[['revision-one',2]]
   def compete(label):
    return query('concurrent-'+label,statement,{'key':scope,'expected':'revision-one','revision':label},success=True)
   # Store both responses without predicting which contender wins.
   def contender(label):
    data=json.dumps({'statement':'CYPHER 5 '+statement,'parameters':{'key':scope,'expected':'revision-one','revision':label}}).encode();req=urllib.request.Request(endpoint,data=data,headers={'Authorization':auth,'Content-Type':'application/json'},method='POST')
    with opener.open(req,timeout=20) as response:body=response.read(128000);assert response.status==202
    assert password.encode() not in body and auth.encode() not in body;(out/('concurrent-'+label+'.json')).write_bytes(body);return json.loads(body)
   with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:competing=list(pool.map(contender,['revision-A','revision-B']))
   assert sum(not r.get('errors') for r in competing)==1
   result={'actualServer':'Neo4j Community2026.09.0','queryApiV2':True,'cypher5Explicit':True,'httpStatus202EvenQueryError':True,'unicodeParameters':True,'fixedConstraintStaleGuardRolledBack':True,'staleLockCounterUnchanged':True,'sameRevisionIdempotent':True,'twoConcurrentCasExactlyOneSucceeded':True,'java':'JBR25.0.3','vendorJvmQualificationClaimed':False,'ownedPid':child.pid,'listenerPidVerified':True,'serverPortLoopbackOnly':True,'schemaAndDataAreOwnedFixtureOnly':True,'globalServiceOrPathChange':False}
  finally:
   if child.poll() is None:child.terminate()
   try:child.wait(timeout=15)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   result_stop=child.poll() is not None
for artifact in out.rglob('*'):
 if artifact.is_file():assert password.encode() not in artifact.read_bytes() and auth.encode() not in artifact.read_bytes(),'Private fixture authentication leaked'
result['ownedServerStopped']=result_stop;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
