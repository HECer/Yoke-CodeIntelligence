from pathlib import Path
import urllib.request,urllib.parse,json,re,hashlib,concurrent.futures
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/falkor-publish/runtime-metadata-r2';out.mkdir(parents=True,exist_ok=False)
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
def fetch(url,headers=None):
 request=urllib.request.Request(url,headers={'User-Agent':'Yoke-owned-validation-probe/0.5',**(headers or {})})
 with opener.open(request,timeout=30) as response:
  raw=response.read(8_000_001);assert len(raw)<=8_000_000;return raw,{k.lower():v for k,v in response.headers.items()}
def qemu():
 raw,_=fetch('https://qemu.weilnetz.de/w64/');(out/'qemu-index.html').write_bytes(raw);links=re.findall(r'href="([^"]+)"',raw.decode());candidates=sorted(x for x in links if re.search(r'qemu-w64-setup-\d+\.exe$',x))
 chosen=candidates[-1] if candidates else None
 return {'installer':urllib.parse.urljoin('https://qemu.weilnetz.de/w64/',chosen) if chosen else None,'sidecars':[urllib.parse.urljoin('https://qemu.weilnetz.de/w64/',x) for x in links if chosen and x.startswith(chosen+'.')],'lastCandidates':candidates[-3:]}
def image():
 token_raw,_=fetch('https://auth.docker.io/token?service=registry.docker.io&scope=repository:falkordb/falkordb:pull');token=json.loads(token_raw)['token'];headers={'Authorization':'Bearer '+token,'Accept':'application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json'}
 for tag in ['v6.0.2','6.0.2']:
  try:raw,meta=fetch('https://registry-1.docker.io/v2/falkordb/falkordb/manifests/'+tag,headers)
  except urllib.error.HTTPError as e:
   if e.code==404:continue
   raise
  value=json.loads(raw);(out/'image-index.json').write_bytes(raw);digest=meta.get('docker-content-digest');assert digest=='sha256:'+hashlib.sha256(raw).hexdigest()
  if 'manifests' in value:
   selected=next(x for x in value['manifests'] if x.get('platform',{}).get('os')=='linux' and x['platform'].get('architecture')=='amd64');raw,meta=fetch('https://registry-1.docker.io/v2/falkordb/falkordb/manifests/'+selected['digest'],headers);assert 'sha256:'+hashlib.sha256(raw).hexdigest()==selected['digest'];value=json.loads(raw);manifest_digest=selected['digest']
  else:manifest_digest=digest
  (out/'image-manifest.json').write_bytes(raw)
  return {'tag':tag,'indexDigest':digest,'manifestDigest':manifest_digest,'config':value['config'],'compressedBytes':sum(x['size'] for x in value['layers']),'layers':[{'digest':x['digest'],'bytes':x['size']} for x in value['layers']]}
 raise RuntimeError('Expected official image tag not available')
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
 a=pool.submit(qemu);b=pool.submit(image);result={'qemu':a.result(),'image':b.result()}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
