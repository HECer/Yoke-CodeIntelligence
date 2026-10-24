from pathlib import Path
import urllib.request,urllib.error,ssl,socket,json,hashlib,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/source-acquisition/baseline';out.mkdir(parents=True,exist_ok=False)
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context()))
targets=[('web','https://example.com/'),('arxiv','https://export.arxiv.org/api/query?id_list=2301.00001&start=0&max_results=1'),('pdf','https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf')]
results=[]
for name,url in targets:
    parsed=urllib.parse.urlsplit(url);addresses=sorted({r[4][0] for r in socket.getaddrinfo(parsed.hostname,443,type=socket.SOCK_STREAM)});start=time.monotonic()
    request=urllib.request.Request(url,headers={'User-Agent':'Yoke-CodeIntelligence/0.5 source-acquisition-validation','Accept-Encoding':'identity'})
    try:
        with opener.open(request,timeout=15) as response:
            body=response.read(262145);assert len(body)<=262144;(out/(name+'.body')).write_bytes(body)
            result={'name':name,'url':url,'status':response.status,'finalUrl':response.url,'contentType':response.headers.get('Content-Type'),'bytes':len(body),'sha256':hashlib.sha256(body).hexdigest(),'resolvedAddresses':addresses,'seconds':time.monotonic()-start,'success':True}
    except Exception as error:
        result={'name':name,'url':url,'success':False,'errorType':type(error).__name__,'status':getattr(error,'code',None),'resolvedAddresses':addresses,'seconds':time.monotonic()-start}
    results.append(result);print(json.dumps(result),flush=True)
(out/'result.json').write_text(json.dumps({'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeImplementation':False,'requests':results},indent=2)+'\n',encoding='utf-8')
