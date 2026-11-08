from pathlib import Path
import ssl,base64,json,hashlib,gzip,stat,tempfile,subprocess,os,socket,time,secrets,ctypes
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';tool=json.loads((base/'runtime-build/result.json').read_bytes());out=base/'native-final-r2';out.mkdir(exist_ok=False);qemu=Path(tool['qemu']);cache=qemu.parent.parent;sha=lambda b:hashlib.sha256(b).hexdigest()
assert sha(qemu.read_bytes())==tool['qemuSha256'] and sha(Path(tool['baseInitramfs']).read_bytes())==tool['baseInitramfsSha256']
work=root.parent/'Yoke-CI-r1-attach'
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
source_before=inventory();(out/'source-before.json').write_text(json.dumps(source_before,indent=2)+'\n',encoding='utf-8');binary_before=sha((root/'target/debug/yoke.exe').read_bytes())
password=secrets.token_urlsafe(32)+' '+chr(0x1f4a1);password_hash=sha(password.encode());responses={}
class ReplyError(Exception):pass
def command(stream,parts):
 encoded=[p.encode() if isinstance(p,str) else p for p in parts];wire=b'*'+str(len(encoded)).encode()+b'\r\n'+b''.join(b'$'+str(len(p)).encode()+b'\r\n'+p+b'\r\n' for p in encoded);stream.sendall(wire)
 def receive(depth=0):
  assert depth<=16
  def line():
   b=bytearray()
   while not b.endswith(b'\r\n'):
    chunk=stream.recv(1)
    if not chunk:raise EOFError('Owned fixture peer closed')
    b+=chunk;assert len(b)<=4096
   return bytes(b[:-2])
  header=line();prefix=header[:1];data=header[1:]
  if prefix==b'+':return data.decode()
  if prefix==b'-':raise ReplyError(data.decode())
  if prefix==b':':return int(data)
  if prefix==b'$':
   n=int(data)
   if n==-1:return None
   assert 0<=n<=1_000_000;b=bytearray()
   while len(b)<n+2:
    chunk=stream.recv(n+2-len(b))
    if not chunk:raise EOFError('Owned fixture peer closed')
    b+=chunk
   assert b[-2:]==b'\r\n';return bytes(b[:-2]).decode()
  if prefix==b'*':
   n=int(data)
   if n==-1:return None
   assert 0<=n<=10000;return [receive(depth+1) for _ in range(n)]
  raise ValueError('Owned fixture reply type unsupported')
 return receive()
def entry(n,content,mode=stat.S_IFREG|0o644,inode=1):
 name=n.encode()+b'\0';header=b'070701'+b''.join(f'{v:08x}'.encode() for v in [inode,mode,0,0,1,0,len(content),0,0,0,0,len(name),0]);return header+name+b'\0'*((-110-len(name))%4)+content+b'\0'*((-len(content))%4)
with tempfile.TemporaryDirectory(prefix='yoke-owned-falkor-vm-') as directory:
 folder=Path(directory);certdir=folder/'certs';certdir.mkdir();certgen=subprocess.run([str(root/'target/debug/yoke-owned-pg-fixture-certgen.exe'),str(certdir)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW);assert certgen.returncode==0;root_der_base64=base64.b64encode(ssl.PEM_cert_to_DER_cert((certdir/'root.pem').read_text())).decode();(out/'root.pem').write_bytes((certdir/'root.pem').read_bytes());(out/'server.pem').write_bytes((certdir/'server.pem').read_bytes());wrong=folder/'wrong-root';wrong.mkdir();certgen=subprocess.run([str(root/'target/debug/yoke-owned-pg-fixture-certgen.exe'),str(wrong)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW);assert certgen.returncode==0;wrong_root_der_base64=base64.b64encode(ssl.PEM_cert_to_DER_cert((wrong/'root.pem').read_text())).decode();port_socket=socket.socket();port_socket.bind(('127.0.0.1',0));port=port_socket.getsockname()[1];port_socket.close();tls_socket=socket.socket();tls_socket.bind(('127.0.0.1',0));tls_port=tls_socket.getsockname()[1];tls_socket.close();assert tls_port!=port
 init=b'''#!/bin/busybox sh
export PATH=/bin:/sbin:/usr/bin:/usr/sbin:/usr/local/bin
/bin/busybox mount -t proc proc /proc
/bin/busybox mount -t sysfs sysfs /sys
/bin/busybox mount -t devtmpfs dev /dev
/bin/busybox mkdir -p /run /tmp
/bin/busybox chmod 1777 /tmp
/bin/busybox modprobe virtio_pci
/bin/busybox modprobe virtio_net
/bin/busybox ifconfig lo 127.0.0.1 up
/bin/busybox ifconfig eth0 10.0.2.15 netmask 255.255.255.0 up
echo OWNED_FALKOR_LINUX_START
/usr/local/bin/redis-server /etc/owned-redis.conf
echo OWNED_FALKOR_SERVER_RETURNED
/bin/busybox poweroff -f
'''
 config=b'''bind 0.0.0.0
port 6379
tls-port 6380
tls-cert-file /etc/owned-server.pem
tls-key-file /etc/owned-server.key
tls-ca-cert-file /etc/owned-root.pem
tls-auth-clients no
protected-mode yes
daemonize no
logfile ""
save ""
appendonly no
dir /tmp
aclfile /etc/owned-users.acl
loadmodule /var/lib/falkordb/bin/falkordb.so
''';acl=('user default on #'+password_hash+' ~owned:* ~yoke:native:v1:* +@all\n').encode()
 overlay=entry('init',init,stat.S_IFREG|0o755)+entry('etc/owned-redis.conf',config)+entry('etc/owned-users.acl',acl)+entry('etc/owned-server.pem',(certdir/'server.pem').read_bytes())+entry('etc/owned-server.key',(certdir/'server.key').read_bytes(),stat.S_IFREG|0o600)+entry('etc/owned-root.pem',(certdir/'root.pem').read_bytes())+entry('TRAILER!!!',b'',0)
 image=folder/'private-initramfs.gz';image.write_bytes(Path(tool['baseInitramfs']).read_bytes()+gzip.compress(overlay,compresslevel=1))
 args=[str(qemu),'-L',str(qemu.parent),'-machine','pc,accel=tcg','-cpu','max','-smp','2','-m','768','-nodefaults','-display','none','-monitor','none','-serial','stdio','-no-reboot','-kernel',tool['kernel'],'-initrd',str(image),'-append','console=ttyS0 panic=1 rdinit=/init net.ifnames=0','-netdev',f'user,id=n0,restrict=on,hostfwd=tcp:127.0.0.1:{port}-10.0.2.15:6379,hostfwd=tcp:127.0.0.1:{tls_port}-10.0.2.15:6380','-device','virtio-net-pci,netdev=n0,romfile=','-object','rng-builtin,id=rng0','-device','virtio-rng-pci,rng=rng0']
 env=os.environ.copy();env['PATH']=str(qemu.parent)+os.pathsep+env.get('PATH','');start=time.monotonic();result=None
 with (out/'guest.log').open('wb') as log:
  child=subprocess.Popen(args,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
  try:
   print(json.dumps({'ownedQemuPid':child.pid,'loopbackPort':port}),flush=True);ready=False
   while time.monotonic()-start<120:
    if child.poll() is not None:break
    try:
     with socket.create_connection(('127.0.0.1',port),timeout=.5) as stream:
      stream.settimeout(.5);assert command(stream,['AUTH','default',password])=='OK';assert command(stream,['PING'])=='PONG';ready=True;break
    except (OSError,EOFError):time.sleep(.25)
   assert ready,'Owned guest not ready; inspect bounded local guest log'
   owner=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',f'Get-NetTCPConnection -LocalPort {port} -State Listen | Select-Object -ExpandProperty OwningProcess'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20,creationflags=subprocess.CREATE_NO_WINDOW);assert child.pid in [int(v) for v in owner.stdout.decode().split()];(out/'listener-owner.txt').write_bytes(owner.stdout)
   with socket.create_connection(('127.0.0.1',port),timeout=5) as stream:
    stream.settimeout(15);assert command(stream,['AUTH','default',password])=='OK';responses['modules']=command(stream,['MODULE','LIST']);key='owned:'+secrets.token_hex(16)
    def query(label,text,success=True):
     try:value=command(stream,['GRAPH.QUERY',key,text,'TIMEOUT','10000']);okay=True
     except ReplyError as error:value={'error':str(error)};okay=False
     assert password not in json.dumps(value,ensure_ascii=False) and password_hash not in json.dumps(value);responses[label]=value
     if success is not None:assert okay==success,label
     return value
    import socketserver,threading,concurrent.futures
    init_race={'key':None};proxy_events=[];drop_next={'armed':False};race={'armed':False};barrier=threading.Barrier(2)
    def frame(peer,depth=0):
     assert depth<=16
     line=bytearray()
     while not line.endswith(b'\r\n'):
      b=peer.recv(1)
      if not b:raise EOFError('Owned proxy peer closed')
      line+=b;assert len(line)<=4096
     raw=bytes(line);kind=raw[:1];body=raw[1:-2]
     if kind in (b'+',b'-'):return raw,body
     if kind==b':':return raw,int(body)
     if kind==b'$':
      n=int(body)
      if n==-1:return raw,None
      assert 0<=n<=32_000_000;data=bytearray()
      while len(data)<n+2:
       chunk=peer.recv(min(n+2-len(data),65536))
       if not chunk:raise EOFError('Owned proxy peer closed')
       data+=chunk
      assert data[-2:]==b'\r\n';return raw+data,bytes(data[:-2])
     assert kind==b'*';n=int(body)
     if n==-1:return raw,None
     assert 0<=n<=100000;items=[]
     for _ in range(n):
      child,value=frame(peer,depth+1);raw+=child;items.append(value);assert len(raw)<=32_000_000
     return raw,items
    class Proxy(socketserver.BaseRequestHandler):
     def handle(self):
      self.request.settimeout(30)
      with socket.create_connection(('127.0.0.1',port),timeout=30) as upstream:
       while True:
        try:wire,parts=frame(self.request)
        except (EOFError,OSError):return
        publication=parts[0]==b'GRAPH.QUERY' and b'MATCH (scope:YokeNativeScope' in parts[2] and b'SET scope.lockCounter=' in parts[2]
        if publication and race['armed']:barrier.wait(timeout=20)
        if parts[0]==b'EXEC' and init_race['key'] is not None:
         key=init_race['key'];init_race['key']=None
         with socket.create_connection(('127.0.0.1',port),timeout=10) as interference:
          assert command(interference,['AUTH','default',password])=='OK';command(interference,['GRAPH.QUERY',key,'CREATE (:Foreign {preserve:7})'])
        upstream.sendall(wire);response,_=frame(upstream);dropped=publication and drop_next['armed'];proxy_events.append({'command':parts[0].decode('ascii'),'publication':publication,'responseHash':sha(response),'upstreamError':response.startswith(b'-'),'droppedAfterUpstreamResponse':dropped})
        if dropped:
         drop_next['armed']=False;assert not response.startswith(b'-');self.request.shutdown(socket.SHUT_RDWR);return
        self.request.sendall(response)
    class ProxyServer(socketserver.ThreadingTCPServer):daemon_threads=True
    proxy=ProxyServer(('127.0.0.1',0),Proxy);proxy_port=proxy.server_address[1];proxy_thread=threading.Thread(target=proxy.serve_forever,daemon=True);proxy_thread.start()
    native=root/'target/debug/yoke.exe';commands=[];created_credentials=[];native_hash=sha(native.read_bytes());owned_source=folder/'owned-source';owned_source.mkdir();source_bytes=('// owned '+chr(0x1f4a1)+'\r\nfn alpha(){beta();beta();missing();}\r\nfn beta(){alpha();}\r\n').encode();(owned_source/'owned.rs').write_bytes(source_bytes);(out/'owned-fixture.rs').write_bytes(source_bytes)
    def cli(name,args,private=None,success=True,lost_stdout=False):
     argv=[str(native),'--timeout','60000','--max-bytes','200000']+[str(a) for a in args];started=time.monotonic()
     if lost_stdout:
      p=subprocess.Popen(argv,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW);p.stdout.close();p.stdout=None;_,stderr=p.communicate(timeout=75);stdout=b'';code=p.returncode
     else:
      p=subprocess.run(argv,input=private,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=75,creationflags=subprocess.CREATE_NO_WINDOW);stdout=p.stdout;stderr=p.stderr;code=p.returncode
     assert password.encode() not in stdout+stderr and password_hash.encode() not in stdout+stderr
     (out/(name+'.stdout')).write_bytes(stdout);(out/(name+'.stderr')).write_bytes(stderr);commands.append({'name':name,'command':argv,'exitCode':code,'seconds':time.monotonic()-started,'privateStdin':private is not None,'stdoutDiscarded':lost_stdout})
     if success is not None:assert (code==0)==success,(name,code,'see local bounded stderr')
     return json.loads(stdout) if code==0 else stderr.decode(errors='replace')
    def run(label,args,success=True):
     try:value=command(stream,args);okay=True
     except ReplyError as error:value={'error':str(error)};okay=False
     assert password not in json.dumps(value,ensure_ascii=False) and password_hash not in json.dumps(value);responses[label]=value
     if success is not None:assert okay==success,label
     return value
    def do_apply(name,mode,path,plan,journal,connect,success=True,extra=None):
     return cli(name,['falkor-publish',mode,'--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-journal']+(['--allow-write'] if mode=='apply' else ['--allow-recovery'])+connect+(extra or []),success=success)
    def approval(name,path,plan,journal,lost_stdout=False):
     return cli(name,['falkor-publish','approve','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-approval','--allow-journal'],lost_stdout=lost_stdout,success=not lost_stdout)
    snapshot=cli('source-index',['--root',owned_source,'index'])['snapshotId']
    try:
     workflows=[]
     for prefix,selected_port,encrypted in [('plain',port,False),('tls',tls_port,True),('proxy',proxy_port,False)]:
      dest=out/prefix;dest.mkdir();target={'baseUrl':f'redis://127.0.0.1:{selected_port}/','address':f'127.0.0.1:{selected_port}','namespace':'owned native '+prefix+' '+chr(0x1f4a1),'user':'default','useSystemRoots':False}
      if encrypted:target.update({'baseUrl':f'rediss://localhost:{selected_port}/','rootCertificatesDerBase64':[root_der_base64]})
      config=dest/'target.json';config.write_text(json.dumps(target),encoding='utf-8');bootstrap_path=dest/'bootstrap.json';cli(prefix+'-bootstrap-plan',['falkor-publish','bootstrap-plan','--config',config,'--output',bootstrap_path,'--allow-output']);bootstrap=json.loads(bootstrap_path.read_bytes());inspection=cli(prefix+'-offline-inspect',['falkor-publish','inspect','--plan',bootstrap_path]);binding=inspection['credentialBinding']
      cli(prefix+'-credential-import',['falkor-publish','import-password','--config',config,'--allow-credential-create'],private=password.encode());created_credentials.append('yoke-code-intelligence/service/falkor/'+binding)
      connect=['--allow-network','--allow-credential-lookup']+([] if encrypted else ['--allow-loopback-plaintext']);probe=cli(prefix+'-empty-probe',['falkor-publish','probe','--plan',bootstrap_path]+connect);assert not probe['graphExists']
      bootstrap_journal=dest/('falkor-'+bootstrap['planId']+'.jsonl');approval(prefix+'-bootstrap-approve',bootstrap_path,bootstrap,bootstrap_journal)
      denied=do_apply(prefix+'-schema-grant-denied','apply',bootstrap_path,bootstrap,bootstrap_journal,connect,False);assert 'schema_permission_required' in denied
      do_apply(prefix+'-bootstrap-apply','apply',bootstrap_path,bootstrap,bootstrap_journal,connect,extra=['--allow-schema']);state=cli(prefix+'-bootstrap-journal',['falkor-publish','journal','--plan',bootstrap_path,'--journal',bootstrap_journal,'--allow-journal']);assert state['completedOperations']==9 and state['eventCount']==19
      probe=cli(prefix+'-ready-probe',['falkor-publish','probe','--plan',bootstrap_path]+connect);assert probe['schema']['allRequiredPresent'] and probe['schema']['allObservedOperational']
      path=dest/'publication.json';cli(prefix+'-publication-prepare',['falkor-publish','prepare','--config',config,'--source-root',owned_source,'--snapshot-id',snapshot,'--expected-revision','absent','--output',path,'--allow-output','--allow-source-coordination']);plan=json.loads(path.read_bytes());journal=dest/('falkor-'+plan['planId']+'.jsonl');approval(prefix+'-publication-approve',path,plan,journal,lost_stdout=True)
      if prefix=='proxy':
       drop_next['armed']=True;denied=do_apply(prefix+'-publication-response-lost','apply',path,plan,journal,connect,False);assert 'host_operation_failed' in denied
       historical_dir=dest/'historical-pending';historical_dir.mkdir();historical=historical_dir/journal.name;historical.write_bytes(journal.read_bytes());assert json.loads(journal.read_bytes().splitlines()[-1])['phase']=='pending'
       before=len(proxy_events);denied=do_apply(prefix+'-unknown-result-resend-blocked','apply',path,plan,journal,connect,False);assert 'previous_result_unknown_recovery_required' in denied and not any(e['publication'] for e in proxy_events[before:])
       do_apply(prefix+'-lost-result-read-recovery','recover',path,plan,journal,connect);assert not any(e['publication'] for e in proxy_events[before:])
      else:do_apply(prefix+'-publication-apply','apply',path,plan,journal,connect)
      do_apply(prefix+'-publication-idempotent-journal','apply',path,plan,journal,connect)
      state=cli(prefix+'-publication-journal',['falkor-publish','journal','--plan',path,'--journal',journal,'--allow-journal']);assert state['completedOperations']==1 and state['eventCount']==3
      key=plan['graphKey'];payload=plan['payload'];revision=plan['revisionId'];scope=plan['scopeKey'];params='CYPHER revision='+json.dumps(revision)+' scope='+json.dumps(scope)+' '
      elements=run(prefix+'-original-elements',['GRAPH.RO_QUERY',key,params+'MATCH (n:YokeNativeElement {scopeKey:$scope,revisionId:$revision}) RETURN n.category,n.rowJson'])
      for category,field in [('entity','entities'),('evidence','evidence')]:assert {json.loads(r[1])['id']:json.loads(r[1]) for r in elements[1] if r[0]==category}=={r['id']:r for r in payload[field]}
      header=next(json.loads(r[1]) for r in elements[1] if r[0]=='revision')
      for field in ['sourceFiles','coverage','diagnostics','graphHash','payloadHash']:assert header[field]==payload[field]
      observed=run(prefix+'-original-observations',['GRAPH.RO_QUERY',key,params+'MATCH (a:YokeNativeElement)-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$scope,revisionId:$revision}]->(b:YokeNativeElement) RETURN a.sourceId,b.sourceId,b.sourceTargetWasNull,r.rowJson ORDER BY r.key'])
      assert {json.loads(r[3])['id']:json.loads(r[3]) for r in observed[1]}=={r['id']:r for r in payload['observations']}
      for row in observed[1]:
       raw=json.loads(row[3]);assert row[0]==raw['source'];assert (row[1]==raw['target'] and row[2] is None) if raw['target'] is not None else (row[1] is None and row[2]=='true')
      counter=run(prefix+'-same-journal-no-resend',['GRAPH.RO_QUERY',key,'MATCH (s:YokeNativeScope) RETURN s.lockCounter']);assert counter[1]==[[1]]
      # An independent reviewed journal tests actual same-revision replay and count guards.
      replay=dest/'replay';replay.mkdir();replay_journal=replay/('falkor-'+plan['planId']+'.jsonl');approval(prefix+'-replay-approve',path,plan,replay_journal);do_apply(prefix+'-same-revision-replay','apply',path,plan,replay_journal,connect)
      extra=secrets.token_hex(32);run(prefix+'-insert-owned-count-conflict',['GRAPH.QUERY',key,'CYPHER key='+json.dumps(extra)+' revision='+json.dumps(revision)+' scope='+json.dumps(scope)+' CREATE (:YokeNativeElement {key:$key,scopeKey:$scope,revisionId:$revision})'])
      conflict=dest/'count-conflict';conflict.mkdir();conflict_journal=conflict/('falkor-'+plan['planId']+'.jsonl');approval(prefix+'-count-conflict-approve',path,plan,conflict_journal)
      denied=do_apply(prefix+'-count-conflict-denied','apply',path,plan,conflict_journal,connect,False);assert 'query_refused' in denied
      denied=do_apply(prefix+'-pending-resend-blocked','apply',path,plan,conflict_journal,connect,False);assert 'previous_result_unknown_recovery_required' in denied
      denied=do_apply(prefix+'-count-conflict-recovery-denied','recover',path,plan,conflict_journal,connect,False);assert 'outcome_unconfirmed_no_automatic_resend' in denied
      counter=run(prefix+'-count-conflict-counter-unchanged',['GRAPH.RO_QUERY',key,'MATCH (s:YokeNativeScope) RETURN s.lockCounter']);assert counter[1]==[[2]]
      run(prefix+'-remove-only-owned-count-extra',['GRAPH.QUERY',key,'CYPHER key='+json.dumps(extra)+' MATCH (n:YokeNativeElement {key:$key}) DELETE n']);do_apply(prefix+'-positive-recovery-after-restored-postcondition','recover',path,plan,conflict_journal,connect)
      workflows.append({'tls':encrypted,'planId':plan['planId'],'revisionId':revision,'allOriginalRowsAndEvidenceExact':True,'sameJournalNoResend':True,'sameRevisionReplay':True,'countGuardRolledBackCounter':True,'readOnlyRecoveryRefusedUntilPostconditionRestored':True})
      if prefix=='proxy':
       race_plans=[]
       for label in ['A','B']:
        code=('fn candidate_'+label+'(){}\r\n').encode();(owned_source/('race-'+label+'.rs')).write_bytes(code);(out/('owned-race-'+label+'.rs')).write_bytes(code);snapshot_race=cli('race-'+label+'-source-index',['--root',owned_source,'index'])['snapshotId'];race_dir=out/('race-'+label);race_dir.mkdir();race_path=race_dir/'publication.json'
        cli('race-'+label+'-prepare',['falkor-publish','prepare','--config',config,'--source-root',owned_source,'--snapshot-id',snapshot_race,'--expected-revision',revision,'--output',race_path,'--allow-output','--allow-source-coordination']);race_plan=json.loads(race_path.read_bytes());race_journal=race_dir/('falkor-'+race_plan['planId']+'.jsonl');approval('race-'+label+'-approve',race_path,race_plan,race_journal);race_plans.append((label,race_path,race_plan,race_journal))
       race['armed']=True;before=len(proxy_events)
       def contender(item):
        label,path2,plan2,journal2=item
        return do_apply('race-'+label+'-apply','apply',path2,plan2,journal2,connect,None)
       with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:outcomes=list(pool.map(contender,race_plans))
       race['armed']=False;assert sum(isinstance(x,dict) for x in outcomes)==1;assert sum(e['publication'] for e in proxy_events[before:])==2
       winner=race_plans[next(i for i,r in enumerate(outcomes) if isinstance(r,dict))];loser=race_plans[next(i for i,r in enumerate(outcomes) if not isinstance(r,dict))]
       state=run('race-final-state',['GRAPH.RO_QUERY',key,'CYPHER loser='+json.dumps(loser[2]['revisionId'])+' MATCH (s:YokeNativeScope) CALL { MATCH (n:YokeNativeElement {revisionId:$loser}) RETURN count(n) AS nodes } CALL { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {revisionId:$loser}]->() RETURN count(r) AS edges } RETURN s.activeRevision,s.lockCounter,nodes,edges']);assert state[1]==[[winner[2]['revisionId'],3,0,0]]
       before=len(proxy_events);denied=do_apply('race-loser-read-recovery-refused','recover',loser[1],loser[2],loser[3],connect,False);assert 'outcome_unconfirmed_no_automatic_resend' in denied and not any(e['publication'] for e in proxy_events[before:])
       do_apply('historical-recovery-after-later-active','recover',path,plan,historical,connect);assert not any(e['publication'] for e in proxy_events[before:])
       run('insert-owned-wrong-index',['GRAPH.QUERY',key,'CREATE INDEX FOR (n:YokeNativeElement) ON (n.wrongProperty)']);denied=cli('wrong-schema-refused',['falkor-publish','probe','--plan',path]+connect,success=False);assert 'schema_conflict' in denied
       wrong=run('wrong-schema-preserved',['GRAPH.RO_QUERY',key,'CALL db.indexes() YIELD label,properties UNWIND properties AS property RETURN label,property ORDER BY label,property']);assert ['YokeNativeElement','wrongProperty'] in wrong[1]
     for label,hostname,root_der in [('wrong-host','untrusted.invalid',root_der_base64),('wrong-root','localhost',wrong_root_der_base64)]:
      dest=out/label;dest.mkdir();target={'baseUrl':f'rediss://{hostname}:{tls_port}/','address':f'127.0.0.1:{tls_port}','namespace':'owned native tls '+chr(0x1f4a1),'user':'default','useSystemRoots':False,'rootCertificatesDerBase64':[root_der]};config=dest/'target.json';config.write_text(json.dumps(target),encoding='utf-8');path=dest/'plan.json';cli(label+'-plan',['falkor-publish','bootstrap-plan','--config',config,'--output',path,'--allow-output']);binding=cli(label+'-inspect',['falkor-publish','inspect','--plan',path])['credentialBinding'];cli(label+'-import',['falkor-publish','import-password','--config',config,'--allow-credential-create'],private=password.encode());created_credentials.append('yoke-code-intelligence/service/falkor/'+binding)
      cli(label+'-tls-refused',['falkor-publish','probe','--plan',path,'--allow-network','--allow-credential-lookup'],success=False)
     # Existing foreign graph data is never adopted or schematically repaired.
     foreign_dir=out/'foreign-graph';foreign_dir.mkdir();foreign_target=dict(target);foreign_target.update({'baseUrl':f'redis://127.0.0.1:{port}/','address':f'127.0.0.1:{port}','namespace':'owned foreign admission','rootCertificatesDerBase64':[]});foreign_config=foreign_dir/'target.json';foreign_config.write_text(json.dumps(foreign_target),encoding='utf-8');foreign_path=foreign_dir/'bootstrap.json';cli('foreign-bootstrap-plan',['falkor-publish','bootstrap-plan','--config',foreign_config,'--output',foreign_path,'--allow-output']);foreign_plan=json.loads(foreign_path.read_bytes());foreign_key=foreign_plan['graphKey'];run('insert-owned-foreign-graph',['GRAPH.QUERY',foreign_key,'CREATE (:Foreign {preserve:1})']);foreign_journal=foreign_dir/('falkor-'+foreign_plan['planId']+'.jsonl');approval('foreign-bootstrap-approve',foreign_path,foreign_plan,foreign_journal)
     denied=do_apply('foreign-bootstrap-refused','apply',foreign_path,foreign_plan,foreign_journal,['--allow-network','--allow-credential-lookup','--allow-loopback-plaintext'],False,extra=['--allow-schema']);assert 'namespace_already_exists' in denied
     preserved=run('foreign-graph-unchanged',['GRAPH.RO_QUERY',foreign_key,'MATCH (n:Foreign) RETURN n.preserve']);assert preserved[1]==[[1]];assert len(foreign_journal.read_bytes().splitlines())==1
     # Create the watched graph from another connection after the native queue,
     # immediately before its EXEC. Only the native initialization is discarded.
     watched_dir=out/'watched-race';watched_dir.mkdir();watched_target={'baseUrl':f'redis://127.0.0.1:{proxy_port}/','address':f'127.0.0.1:{proxy_port}','namespace':'owned watched admission','user':'default','useSystemRoots':False};watched_config=watched_dir/'target.json';watched_config.write_text(json.dumps(watched_target),encoding='utf-8');watched_path=watched_dir/'bootstrap.json';cli('watched-bootstrap-plan',['falkor-publish','bootstrap-plan','--config',watched_config,'--output',watched_path,'--allow-output']);watched_plan=json.loads(watched_path.read_bytes());watched_journal=watched_dir/('falkor-'+watched_plan['planId']+'.jsonl');approval('watched-bootstrap-approve',watched_path,watched_plan,watched_journal);init_race['key']=watched_plan['graphKey']
     denied=do_apply('watched-exec-conflict-refused','apply',watched_path,watched_plan,watched_journal,['--allow-network','--allow-credential-lookup','--allow-loopback-plaintext'],False,extra=['--allow-schema']);assert 'atomic_initialization_unconfirmed' in denied and init_race['key'] is None
     preserved=run('watched-only-foreign-write',['GRAPH.RO_QUERY',watched_plan['graphKey'],'MATCH (n) RETURN labels(n)[0],n.preserve']);assert preserved[1]==[['Foreign',7]]
     denied=do_apply('watched-no-positive-root-recovery','recover',watched_path,watched_plan,watched_journal,['--allow-network','--allow-credential-lookup','--allow-loopback-plaintext'],False);assert 'outcome_unconfirmed_no_automatic_resend' in denied
     assert json.loads(watched_journal.read_bytes().splitlines()[-1])['phase']=='pending'
     result={'actualImageTag':'6.0.2','manifestDigest':json.loads((base/'acquire/result.json').read_bytes())['image']['manifestDigest'],'ownedQemuPid':child.pid,'listenerPidVerified':True,'nativeTransportTested':True,'nativeBinarySha256':native_hash,'nativeTlsStaticCa':True,'wrongHostnameRefused':True,'wrongStaticRootRefused':True,'privateUnicodePassword':True,'schemaGrantIndependent':True,'bootstrapOperations':9,'bootstrapJournalEvents':19,'lostStdoutIdentityRetained':True,'responseLostAfterActualCommit':True,'forcedConcurrentUpdatesExactlyOneCommit':True,'loserCreatedNoRowsOrCounterChanges':True,'historicalReadRecoveryAfterLaterActive':True,'wrongSchemaRefusedWithoutRepair':True,'foreignExistingGraphRefusedWithoutChanges':True,'nativeWatchExecConflictKeepsOnlyForeignWriteAndPendingIntent':True,'workflows':workflows,'qemuUserNetworkRestricted':True,'hostForwardsLoopbackOnly':True,'projectCodeExecuted':False,'defaultModelOrBrowserStarted':False}
    finally:
     proxy.shutdown();proxy.server_close();proxy_thread.join(timeout=3);(out/'proxy-events.json').write_text(json.dumps(proxy_events,indent=2)+'\n',encoding='utf-8')
     (out/'responses-partial.json').write_text(json.dumps(responses,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
     removed=0;advapi=ctypes.WinDLL('Advapi32',use_last_error=True);advapi.CredDeleteW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32];advapi.CredDeleteW.restype=ctypes.c_int
     for name in created_credentials:assert advapi.CredDeleteW(name,1,0),'Owned fixture credential removal failed';removed+=1
     (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n',encoding='utf-8');(out/'credential-cleanup.json').write_text(json.dumps({'created':len(created_credentials),'removed':removed})+'\n',encoding='utf-8')
     assert sha(native.read_bytes())==native_hash
    result['ownedCredentialsRemoved']=removed
  finally:
   if child.poll() is None:child.terminate()
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   (out/'cleanup.json').write_text(json.dumps({'ownedPid':child.pid,'ownedQemuStopped':child.poll() is not None})+'\n')
 if result is not None:result['ownedQemuStopped']=child.poll() is not None
for file in out.rglob('*'):
 if file.is_file():assert password.encode() not in file.read_bytes() and password_hash.encode() not in file.read_bytes()
(out/'responses.json').write_text(json.dumps(responses,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)


source_after=inventory();(out/"source-after.json").write_text(json.dumps(source_after,indent=2)+"\n",encoding="utf-8");assert source_before==source_after;assert binary_before==sha((root/"target/debug/yoke.exe").read_bytes());(out/"frozen-inputs.json").write_text(json.dumps({"inputs":len(source_before),"sourceUnchanged":True,"binaryUnchanged":True,"binarySha256":binary_before})+"\n",encoding="utf-8")
