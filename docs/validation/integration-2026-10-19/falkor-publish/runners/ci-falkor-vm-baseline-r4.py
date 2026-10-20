from pathlib import Path
import json,hashlib,gzip,stat,tempfile,subprocess,os,socket,time,secrets,ctypes
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';tool=json.loads((base/'runtime-build/result.json').read_bytes());out=base/'vm-baseline-r4';out.mkdir(exist_ok=False);qemu=Path(tool['qemu']);cache=qemu.parent.parent;sha=lambda b:hashlib.sha256(b).hexdigest()
assert sha(qemu.read_bytes())==tool['qemuSha256'] and sha(Path(tool['baseInitramfs']).read_bytes())==tool['baseInitramfsSha256']
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
 folder=Path(directory);port_socket=socket.socket();port_socket.bind(('127.0.0.1',0));port=port_socket.getsockname()[1];port_socket.close()
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
protected-mode yes
daemonize no
logfile ""
save ""
appendonly no
dir /tmp
aclfile /etc/owned-users.acl
loadmodule /var/lib/falkordb/bin/falkordb.so
''';acl=('user default on #'+password_hash+' ~owned:* +@all\n').encode()
 overlay=entry('init',init,stat.S_IFREG|0o755)+entry('etc/owned-redis.conf',config)+entry('etc/owned-users.acl',acl)+entry('TRAILER!!!',b'',0)
 image=folder/'private-initramfs.gz';image.write_bytes(Path(tool['baseInitramfs']).read_bytes()+gzip.compress(overlay,compresslevel=1))
 args=[str(qemu),'-L',str(qemu.parent),'-machine','pc,accel=tcg','-cpu','max','-smp','2','-m','768','-nodefaults','-display','none','-monitor','none','-serial','stdio','-no-reboot','-kernel',tool['kernel'],'-initrd',str(image),'-append','console=ttyS0 panic=1 rdinit=/init net.ifnames=0','-netdev',f'user,id=n0,restrict=on,hostfwd=tcp:127.0.0.1:{port}-10.0.2.15:6379','-device','virtio-net-pci,netdev=n0,romfile=','-object','rng-builtin,id=rng0','-device','virtio-rng-pci,rng=rng0']
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
    text='quoted '+chr(0x1f4a1)+' \' " \\ \r\n';query('unicode-parameter','CYPHER value='+json.dumps(text,ensure_ascii=False)+' RETURN $value AS value')
    query('parallel-create',"CREATE (a:Owned {id:'a'})-[r:LINK {id:'one'}]->(b:Owned {id:'b'}), (a)-[s:LINK {id:'two'}]->(b) RETURN count(r),count(s)")
    query('parallel-read','MATCH (a:Owned)-[r:LINK]->(b:Owned) RETURN a.id,r.id,b.id ORDER BY r.id')
    query('foreach-support','FOREACH (v IN [1,2] | CREATE (:Loop {id:v})) RETURN 1',None)
    query('subquery-support','CALL { RETURN 1 AS n } RETURN n',None)
    query('division-error','RETURN 0/0 AS n',None)
    result={'actualImageTag':'6.0.2','manifestDigest':json.loads((base/'acquire/result.json').read_bytes())['image']['manifestDigest'],'ownedQemuPid':child.pid,'listenerPidVerified':True,'bootSeconds':round(time.monotonic()-start,2),'unicodePasswordAuthentication':True,'qemuUserNetworkRestricted':True,'hostForwardsLoopbackOnly':True,'serverDefaultModelOrBrowserStarted':False,'projectCodeExecuted':False,'globalInstallOrWslFeatureChange':False}
  finally:
   if child.poll() is None:child.terminate()
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   (out/'cleanup.json').write_text(json.dumps({'ownedPid':child.pid,'ownedQemuStopped':child.poll() is not None})+'\n')
 if result is not None:result['ownedQemuStopped']=child.poll() is not None
for file in out.rglob('*'):
 if file.is_file():assert password.encode() not in file.read_bytes() and password_hash.encode() not in file.read_bytes()
(out/'responses.json').write_text(json.dumps(responses,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)

