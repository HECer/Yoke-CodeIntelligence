from pathlib import Path
import subprocess,tempfile,json,hashlib,socket,os,threading
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');out=root/'validation/integration-20261018/source-acquisition/live-r2';out.mkdir(parents=True,exist_ok=False);binary=root/'target/debug/yoke.exe'
before=json.loads((out.parent/'final/source-before.json').read_bytes())
assert all(hashlib.sha256((work/n).read_bytes()).hexdigest()==h for n,h in before.items())
def run(name,args,success=True):
    p=subprocess.run([str(binary),'--timeout','20000','--max-bytes','1000000','source-acquisition',*args],cwd=work,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
    (out/(name+'.stdout')).write_bytes(p.stdout);(out/(name+'.stderr')).write_bytes(p.stderr)
    assert (p.returncode==0)==success,(name,p.returncode,p.stderr.decode(errors='replace')[:200]);return json.loads(p.stdout) if p.returncode==0 else None
results=[]
with tempfile.TemporaryDirectory(prefix='yoke-owned-source-live-') as folder:
    private=Path(folder)
    for name,url,kind in [('web','https://example.com/','html'),('pdf','https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf','pdf')]:
        import urllib.parse
        host=urllib.parse.urlsplit(url).hostname;address=next(r[4][0] for r in socket.getaddrinfo(host,443,type=socket.SOCK_STREAM) if r[0]==socket.AF_INET);config=private/(name+'-config.json');archive=private/(name+'.json')
        config.write_text(json.dumps({'url':url,'address':address+':443','format':kind,'useSystemRoots':True}),encoding='utf-8')
        captured=run(name+'-capture',['capture','--config',str(config),'--output',str(archive),'--allow-network','--allow-output']);raw=archive.read_bytes();(out/(name+'-archive.json')).write_bytes(raw);assert captured['archiveHash']==hashlib.sha256(raw).hexdigest()
        inspected=run(name+'-read',['read','--input',str(archive)]);assert inspected['networkConnections']==0 and inspected['sourceAuthority']==False and inspected['complete']==False
        run(name+'-refuse-existing',['capture','--config',str(config),'--output',str(archive),'--allow-network','--allow-output'],False);assert archive.read_bytes()==raw
        result={'name':name,'url':url,'hostSelectedAddress':address+':443','nativeCapture':True,'bodyBytes':inspected['bodyBytes'],'bodyHash':inspected['sourceHash'],'archiveHash':captured['archiveHash'],'hostSetupDNSPerformed':True,'frozenReadNetworkConnections':0}
        if kind=='pdf':
            text=run('pdf-native-text',['read','--input',str(archive),'--pdf-text']);assert text['networkConnections']==0 and text['sourceHash']==inspected['sourceHash'];result['nativePdfText']=True;result['textPages']=len(text['pdf']['segments']);assert any('Dummy' in p['text'] for p in text['pdf']['segments'])
        results.append(result);print(json.dumps(result),flush=True)
    listener=socket.socket();listener.bind(('127.0.0.1',0));listener.listen(1);port=listener.getsockname()[1]
    def fixture():
        try:
            stream,_=listener.accept()
            with stream:
                stream.settimeout(5);data=b''
                while b'\r\n\r\n' not in data:data+=stream.recv(4096)
                stream.sendall(b'HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbody')
        finally:listener.close()
    worker=threading.Thread(target=fixture,daemon=True);worker.start();config=private/'lost-config.json';archive=private/'lost-delivery.json';config.write_text(json.dumps({'url':f'http://127.0.0.1:{port}/owned','address':f'127.0.0.1:{port}','format':'text','useSystemRoots':False}),encoding='utf-8')
    command=[str(binary),'source-acquisition','capture','--config',str(config),'--output',str(archive),'--allow-network','--allow-output','--allow-loopback-http'];child=subprocess.Popen(command,cwd=work,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0);child.stdout.close();child.wait(timeout=15);error=child.stderr.read();child.stderr.close();worker.join(timeout=5)
    assert child.returncode!=0 and archive.exists() and b'source_acquisition_delivery_failed_after_publication' in error
    raw=archive.read_bytes();assert hashlib.sha256(raw).hexdigest().encode() in error;(out/'lost-delivery.stderr').write_bytes(error);(out/'lost-delivery-archive.json').write_bytes(raw)
    results.append({'name':'lost-stdout-owned-HTTP','artifactPreserved':True,'identityReported':True,'exitCode':child.returncode})
unchanged=all(hashlib.sha256((work/n).read_bytes()).hexdigest()==h for n,h in before.items());assert unchanged
result={'sourceUnchanged':unchanged,'inputs':len(before),'binaryHash':hashlib.sha256(binary.read_bytes()).hexdigest(),'results':results,'arxivNativeImplementation':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
