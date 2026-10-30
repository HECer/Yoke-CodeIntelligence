from pathlib import Path
import urllib.request,ssl,socket,json,hashlib,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/source-acquisition/arxiv-frontend-investigation';out.mkdir(parents=True,exist_ok=False)
url='https://arxiv.org/abs/2301.00001';addresses=sorted({r[4][0] for r in socket.getaddrinfo('arxiv.org',443,type=socket.SOCK_STREAM)});start=time.monotonic();result={'url':url,'resolvedAddresses':addresses,'nativeImplementation':False,'singleRequest':True,'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat()}
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context()))
try:
    request=urllib.request.Request(url,headers={'User-Agent':'Yoke-CodeIntelligence/0.5 source-acquisition-validation','Accept-Encoding':'identity'})
    with opener.open(request,timeout=15) as response:
        body=response.read(262145);assert len(body)<=262144;(out/'response.body').write_bytes(body);result.update({'success':True,'status':response.status,'finalUrl':response.url,'bytes':len(body),'sha256':hashlib.sha256(body).hexdigest(),'contentType':response.headers.get('Content-Type')})
except Exception as error:result.update({'success':False,'errorType':type(error).__name__,'status':getattr(error,'code',None)})
result['seconds']=time.monotonic()-start;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
