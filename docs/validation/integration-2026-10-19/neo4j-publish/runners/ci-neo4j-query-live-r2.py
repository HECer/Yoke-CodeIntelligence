from pathlib import Path
import tempfile,subprocess,json,os,socket,time,ssl,urllib.request,urllib.error,base64,secrets,hashlib,ctypes,shutil,concurrent.futures
primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/neo4j-publish/query-live-r2';out.mkdir(parents=True,exist_ok=False);home=Path.home()/'AppData/Local/YokeValidation/neo4j-2026.09.0/neo4j-community-2026.09.0';jhome=Path(r'C:\Program Files\Android\Android Studio\jbr');java=jhome/'bin/java.exe';hidden=subprocess.CREATE_NO_WINDOW
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
   target={'baseUrl':f'http://127.0.0.1:{http}/','address':f'127.0.0.1:{http}','database':'neo4j','namespace':'owned graph '+chr(0x1f4a1),'user':'neo4j','useSystemRoots':False}
   target_path=out/'public-target.json';target_path.write_text(json.dumps(target),encoding='utf-8')
   example=primary/'target/debug/examples/neo4j_owned_probe.exe'
   generated=run([example,target_path,out/'generated'],timeout=40)
   (out/'generator.log').write_bytes(generated.stdout+generated.stderr);assert generated.returncode==0
   bootstrap=json.loads((out/'generated/bootstrap-requests.json').read_bytes());publication=json.loads((out/'generated/publish-requests.json').read_bytes())[0]
   def submit(name,request,success=True):
    assert request['statement'].startswith('CYPHER 5 ')
    return query(name,request['statement'][9:],request['parameters'],success)
   for i,request in enumerate(bootstrap):submit('bootstrap-'+str(i),request)
   first=submit('publish-first',publication);repeat=submit('publish-repeat',publication)
   assert first['data']==repeat['data']
   receipt=first['data']['values'][0][0];params=publication['parameters'];assert receipt['observationCount']==len(params['observations'])
   def frozen_state(label):
    return query(label,'MATCH (s:YokeNativeScope {key:$key}) RETURN s.activeRevision,s.lockCounter,COUNT { MATCH (n:YokeNativeElement {scopeKey:$key}) },COUNT { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$key}]->() }',{'key':params['scopeKey']})['data']['values']
   before=frozen_state('before-denials')
   stale=json.loads(json.dumps(publication));stale['parameters']['expectedRevision']='wrong';stale['parameters']['revisionId']='0'*64
   submit('stale-denied',stale,False);assert frozen_state('after-stale')==before
   altered=json.loads(json.dumps(publication));altered['parameters']['entities'][0]['props']['label']='altered server row'
   submit('immutable-row-denied',altered,False);assert frozen_state('after-row-denied')==before
   read=query('roundtrip-elements','MATCH (n:YokeNativeElement {scopeKey:$key}) RETURN n.key,n.rowHash,n.rowJson ORDER BY n.key',{'key':params['scopeKey']})
   expected={r['key']:(r['rowHash'],r['props']['rowJson']) for category in ['entities','evidence','unresolved'] for r in params[category]};expected[params['revision']['key']]=(params['revision']['rowHash'],params['revision']['props']['rowJson'])
   assert {r[0]:(r[1],r[2]) for r in read['data']['values']}==expected
   relationships=query('roundtrip-observations','MATCH (a:YokeNativeElement)-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$key}]->(b:YokeNativeElement) RETURN r.key,a.key,b.key,r.rowHash,r.rowJson ORDER BY r.key',{'key':params['scopeKey']})
   assert {r[0]:r[1:] for r in relationships['data']['values']}=={r['key']:[r['sourceKey'],r['targetKey'],r['rowHash'],r['props']['rowJson']] for r in params['observations']}
   assert len(params['unresolved'])>0
   parallel={}
   for r in params['observations']:
    pair=(r['sourceKey'],r['targetKey']);parallel[pair]=parallel.get(pair,0)+1
   assert max(parallel.values())>=2
   discovery=query('fleet-disabled','SHOW SETTINGS YIELD name,value WHERE name=$name RETURN value',{'name':'server.fleet_discovery.enabled'});assert discovery['data']['values']==[['false']]
   result={'actualServer':'Neo4j Community2026.09.0','nativeGeneratedFixedCypher':True,'nativeTransportTested':False,'firstPublicationAndReplayIdentical':True,'staleTransactionRolledBack':True,'immutableRowConflictRolledBack':True,'allElementRowJsonAndHashesExact':True,'allObservationDirectionsParallelKeysAndJsonExact':True,'unresolvedTargetsExplicit':True,'entityCount':len(params['entities']),'evidenceCount':len(params['evidence']),'observationCount':len(params['observations']),'unresolvedCount':len(params['unresolved']),'fleetDiscoveryConfiguredDisabled':True,'ownedPid':child.pid,'listenerPidVerified':True,'java':'JBR25.0.3','vendorJvmQualificationClaimed':False}
  finally:
   if child.poll() is None:child.terminate()
   try:child.wait(timeout=15)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   result_stop=child.poll() is not None
for artifact in out.rglob('*'):
 if artifact.is_file():assert password.encode() not in artifact.read_bytes() and auth.encode() not in artifact.read_bytes(),'Private fixture authentication leaked'
result['ownedServerStopped']=result_stop;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)

