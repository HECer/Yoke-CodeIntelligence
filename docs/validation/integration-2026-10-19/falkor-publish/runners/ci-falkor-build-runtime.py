from pathlib import Path,PurePosixPath
import json,subprocess,tarfile,gzip,hashlib,stat,os
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';receipt=json.loads((base/'acquire/result.json').read_bytes());cache=Path(receipt['cache']);out=base/'runtime-build';out.mkdir(exist_ok=False);sha=lambda b:hashlib.sha256(b).hexdigest()
for record in receipt['records']:assert sha(Path(record['file']).read_bytes())==record['sha256']
qemu=cache/'qemu';qemu.mkdir();paths=json.loads((base/'runtime-inspect/qemu-files.json').read_bytes());selected=[p for p in paths if ('\\' not in p and '/' not in p and (p.endswith('.dll') or p=='qemu-system-x86_64.exe')) or p=='share\\bios-256k.bin'];assert 'qemu-system-x86_64.exe' in selected
extract=subprocess.run([r'C:\Program Files\7-Zip\7z.exe','e',str(cache/'qemu-installer.exe'),'-o'+str(qemu),'-y',*selected],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60,creationflags=subprocess.CREATE_NO_WINDOW);(out/'extract.log').write_bytes(extract.stdout+extract.stderr);assert extract.returncode==0
env=os.environ.copy();env['PATH']=str(qemu)+os.pathsep+env.get('PATH','');version=subprocess.run([str(qemu/'qemu-system-x86_64.exe'),'--version'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=10,creationflags=subprocess.CREATE_NO_WINDOW);(out/'qemu-version.txt').write_bytes(version.stdout+version.stderr);assert version.returncode==0
files={};hardlinks={};virtual_bytes=0
def name(text):
 while text.startswith('./'):text=text[2:]
 if text in ['', '.']:return None
 p=PurePosixPath(text);assert not p.is_absolute() and '..' not in p.parts and '\\' not in text and '\0' not in text;return str(p)
def wanted(n):return n in ['lib64','usr/lib64','usr/local/bin/redis-server','usr/local/bin/redis-cli','var/lib/falkordb/bin/falkordb.so'] or n.startswith(('usr/lib/','lib/','lib64/','usr/lib64/'))
for layer in receipt['image']['layers']:
 path=cache/'image'/(layer['digest'].split(':')[1]+'.blob')
 with tarfile.open(path,'r:*') as archive:
  for m in archive:
   n=name(m.name)
   if n is None:continue
   if PurePosixPath(n).name.startswith('.wh.'):
    if n.endswith('/.wh..wh..opq'):
     prefix=str(PurePosixPath(n).parent)+'/'
     for key in list(files):
      if key.startswith(prefix):files.pop(key)
    else:
     target=str(PurePosixPath(n).parent/PurePosixPath(n).name[4:]);files.pop(target,None)
    continue
   if not wanted(n):continue
   if m.isdir():files[n]=(stat.S_IFDIR|m.mode,b'',0,0)
   elif m.issym():files[n]=(stat.S_IFLNK|m.mode,m.linkname.encode(),0,0)
   elif m.islnk():hardlinks[n]=name(m.linkname)
   elif m.isfile():
    assert m.size<=160_000_000;content=archive.extractfile(m).read();assert len(content)==m.size;virtual_bytes+=len(content);assert virtual_bytes<900_000_000;files[n]=(stat.S_IFREG|m.mode,content,0,0)
for n,target in hardlinks.items():
 assert target in files,(n,target);files[n]=files[target]
# Overlay Alpine's vendor-release initramfs tools and kernel modules in memory.
# Linux symlinks/devices are represented in cpio, never created on Windows.
data=gzip.decompress((cache/'initramfs-virt').read_bytes());assert len(data)<128_000_000;pos=0
while pos+110<=len(data):
 header=data[pos:pos+110];assert header[:6] in [b'070701',b'070702'];v=[int(header[6+i*8:14+i*8],16) for i in range(13)];size=v[6];namesize=v[11];pos+=110;n=data[pos:pos+namesize-1].decode();pos=(pos+namesize+3)&~3;content=data[pos:pos+size];pos=(pos+size+3)&~3
 if n=='TRAILER!!!':break
 n=name(n)
 if n and n!='init':files[n]=(v[1],content,v[9],v[10])
files['lib/x86_64-linux-gnu']=(stat.S_IFLNK|0o777,b'../usr/lib/x86_64-linux-gnu',0,0)
for n in list(files):
 for parent in reversed(PurePosixPath(n).parents):
  p=str(parent)
  if p!='.' and p not in files:files[p]=(stat.S_IFDIR|0o755,b'',0,0)
for n in ['dev','proc','sys','tmp','run','etc']:files[n]=(stat.S_IFDIR|0o755,b'',0,0)
files['dev/console']=(stat.S_IFCHR|0o600,b'',5,1);files['dev/null']=(stat.S_IFCHR|0o666,b'',1,3)
cpio=cache/'base.cpio.gz';manifest=[]
with gzip.open(cpio,'wb',compresslevel=1) as target:
 for inode,(n,(mode,content,major,minor)) in enumerate(sorted(files.items()),1):
  encoded=n.encode()+b'\0';header=b'070701'+b''.join(f'{v:08x}'.encode() for v in [inode,mode,0,0,1,0,len(content),0,0,major,minor,len(encoded),0]);target.write(header+encoded+b'\0'*((-110-len(encoded))%4));target.write(content+b'\0'*((-len(content))%4));manifest.append({'path':n,'mode':mode,'bytes':len(content),'sha256':sha(content)})
 encoded=b'TRAILER!!!\0';target.write(b'070701'+b''.join(f'{v:08x}'.encode() for v in [0,0,0,0,1,0,0,0,0,0,0,len(encoded),0])+encoded+b'\0'*((-110-len(encoded))%4))
(out/'guest-files.json').write_text(json.dumps(manifest,indent=2)+'\n');result={'qemu':str(qemu/'qemu-system-x86_64.exe'),'qemuSha256':sha((qemu/'qemu-system-x86_64.exe').read_bytes()),'qemuVersion':version.stdout.decode().splitlines()[0],'kernel':str(cache/'vmlinuz-virt'),'kernelSha256':sha((cache/'vmlinuz-virt').read_bytes()),'baseInitramfs':str(cpio),'baseInitramfsBytes':cpio.stat().st_size,'baseInitramfsSha256':sha(cpio.read_bytes()),'guestFiles':len(files),'guestFileBytes':sum(len(v[1]) for v in files.values()),'linuxSymlinksMaterializedOnHost':False,'globalInstallationPerformed':False,'serverStarted':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
