from pathlib import Path,PurePosixPath
import urllib.request,ssl,json,hashlib,zipfile,stat,datetime,shutil
version='2026.09.0';name=f'neo4j-community-{version}-windows.zip';base=Path.home()/'AppData/Local/YokeValidation';cache=base/'neo4j-investigation';cache.mkdir(exist_ok=True);destination=base/('neo4j-'+version);assert not destination.exists();part=cache/(name+'.part');archive=cache/name;assert not archive.exists() and not part.exists()
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/neo4j-publish/tooling';out.mkdir(parents=True,exist_ok=False);expected='020929aded4ba31a3b111918f4a4c85aa4dfb55a866d4bb8f6e593878335626e';url='https://dist.neo4j.org/'+name;opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context()));digest=hashlib.sha256();size=0
print('START OFFICIAL COMMUNITY DOWNLOAD',flush=True)
with opener.open(url,timeout=40) as response,part.open('xb') as file:
 assert response.status==200 and response.url==url and int(response.headers['content-length'])==274827729
 while block:=response.read(1024*1024):
  size+=len(block);assert size<=274827729;digest.update(block);file.write(block)
assert size==274827729 and digest.hexdigest()==expected;part.rename(archive)
print('SHA256 MATCH; INSPECTING ZIP',flush=True)
with zipfile.ZipFile(archive) as package:
 infos=package.infolist();assert len(infos)<=10000 and sum(i.file_size for i in infos)<=1_500_000_000
 for info in infos:
  path=PurePosixPath(info.filename);assert not path.is_absolute() and path.parts and path.parts[0]=='neo4j-community-'+version and all(p not in ('..','.') and ':' not in p and not any(ord(c)<32 for c in p) for p in path.parts) and '\\' not in info.filename
  assert info.file_size<=128_000_000 and not stat.S_ISLNK(info.external_attr>>16)
  target=destination.joinpath(*path.parts);assert target.resolve().is_relative_to(destination.resolve())
 destination.mkdir()
 for info in infos:
  target=destination.joinpath(*PurePosixPath(info.filename).parts)
  if info.is_dir():target.mkdir(parents=True,exist_ok=True);continue
  target.parent.mkdir(parents=True,exist_ok=True)
  with package.open(info) as incoming,target.open('xb') as file:shutil.copyfileobj(incoming,file)
 files={p.relative_to(destination).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in destination.rglob('*') if p.is_file()}
result={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'version':version,'edition':'community','url':url,'downloadBytes':size,'sha256':expected,'vendorSha256Verified':True,'vendorChecksumUrl':url+'.sha256','installation':str(destination),'zipEntries':len(infos),'extractedFiles':len(files),'uncompressedBytes':sum(i.file_size for i in infos),'globalPathChanged':False,'serviceInstalled':False,'java':'existing Android Studio JBR25; actual functional validation, no Neo4j vendor/JVM qualification claimed'};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');(out/'files.json').write_text(json.dumps(files,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
