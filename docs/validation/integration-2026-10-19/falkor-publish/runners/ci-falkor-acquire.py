from pathlib import Path
import urllib.request,urllib.parse,urllib.error,json,hashlib,concurrent.futures,os,subprocess
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');meta=json.loads((root/'validation/integration-20261018/falkor-publish/runtime-metadata-r2/result.json').read_bytes());out=root/'validation/integration-20261018/falkor-publish/acquire';out.mkdir(parents=True,exist_ok=False);cache=Path.home()/'AppData/Local/YokeValidation/falkor-6.0.2-owned-vm';cache.mkdir(exist_ok=False)
class Redirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,request,fp,code,msg,headers,newurl):
  result=super().redirect_request(request,fp,code,msg,headers,newurl)
  if result is not None and urllib.parse.urlsplit(request.full_url).netloc!=urllib.parse.urlsplit(newurl).netloc:result.remove_header('Authorization')
  assert newurl.startswith('https://');return result
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),Redirect());records=[]
def fetch(url,path,expected=None,cap=400_000_000,headers=None):
 request=urllib.request.Request(url,headers={'User-Agent':'Yoke-owned-validation-tooling/0.5',**(headers or {})});path=cache/path;part=path.with_name(path.name+'.part');path.parent.mkdir(parents=True,exist_ok=True);size=0;digest=hashlib.sha256()
 with opener.open(request,timeout=30) as response,part.open('xb') as file:
  announced=response.headers.get('Content-Length');assert announced is None or int(announced)<=cap
  while chunk:=response.read(1024*1024):
   size+=len(chunk);assert size<=cap;digest.update(chunk);file.write(chunk)
  file.flush();os.fsync(file.fileno())
 actual=digest.hexdigest();assert expected is None or expected=='sha256:'+actual;part.rename(path);record={'file':str(path),'bytes':size,'sha256':actual,'expectedDigestMatched':expected is not None};records.append(record);print(json.dumps({'acquired':path.name,'bytes':size}),flush=True);return path
token_request=urllib.request.Request('https://auth.docker.io/token?service=registry.docker.io&scope=repository:falkordb/falkordb:pull',headers={'User-Agent':'Yoke-owned-validation-tooling/0.5'})
with opener.open(token_request,timeout=30) as response:token=json.load(response)['token']
authorization={'Authorization':'Bearer '+token}
def blob(entry):return fetch('https://registry-1.docker.io/v2/falkordb/falkordb/blobs/'+entry['digest'],Path('image')/(entry['digest'].split(':')[1]+'.blob'),entry['digest'],entry.get('bytes',entry.get('size',400_000_000))+1,authorization)
entries={x['digest']:x for x in meta['image']['layers']};entries[meta['image']['config']['digest']]=meta['image']['config']
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
 jobs=[pool.submit(blob,e) for e in entries.values()];jobs.append(pool.submit(fetch,meta['qemu']['installer'],Path('qemu-installer.exe')));jobs.append(pool.submit(fetch,'https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/x86_64/netboot/vmlinuz-virt',Path('vmlinuz-virt'),None,32_000_000));jobs.append(pool.submit(fetch,'https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/x86_64/netboot/initramfs-virt',Path('initramfs-virt'),None,64_000_000))
 for job in jobs:job.result()
qemu=cache/'qemu-installer.exe';listing=subprocess.run([r'C:\Program Files\7-Zip\7z.exe','l','-slt',str(qemu)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW);(out/'qemu-archive-list.txt').write_bytes(listing.stdout);assert listing.returncode==0
result={'cache':str(cache),'image':meta['image'],'qemuInstallerSource':meta['qemu']['installer'],'records':records,'globalInstallationPerformed':False,'serverStarted':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'cache':str(cache),'files':len(records),'totalBytes':sum(r['bytes'] for r in records)}),flush=True)
