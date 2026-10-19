from pathlib import Path
import urllib.request,urllib.error,ssl,json,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/neo4j-publish/distribution-probe';out.mkdir(parents=True,exist_ok=False)
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context()));name='neo4j-community-2026.09.0-windows.zip';base='https://dist.neo4j.org/';results=[]
for suffix in ['','.sha256']:
 url=base+name+suffix
 try:
  with opener.open(urllib.request.Request(url,method='HEAD' if not suffix else 'GET'),timeout=20) as response:
   body=response.read(4096) if suffix else b'';row={'url':url,'status':response.status,'finalUrl':response.url,'contentLength':response.headers.get('content-length'),'contentType':response.headers.get('content-type')}
   if suffix:(out/'vendor-checksum.txt').write_bytes(body);row['checksumText']=body.decode('ascii').strip()
   results.append(row)
 except urllib.error.HTTPError as error:results.append({'url':url,'status':error.code})
 except Exception as error:results.append({'url':url,'unavailable':type(error).__name__})
receipt={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'version':'2026.09.0','edition':'community','requests':results,'proxyUsed':False};(out/'result.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
