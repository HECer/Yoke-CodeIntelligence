from pathlib import Path
import tempfile,subprocess,socket,os,time,json,hashlib,threading
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/postgres-catalog/live-deadline';out.mkdir(parents=True,exist_ok=False)
binary=primary/'target/debug/yoke.exe';client=Path.home()/'AppData/Local/YokeValidation/postgres-18.6-5/pgsql/bin/psql.exe'
before=json.loads((out.parent/'r8/source-before.json').read_bytes())
assert all(hashlib.sha256((work/n).read_bytes()).hexdigest()==h for n,h in before.items())
state={'accepted':False,'startupBytes':0,'peerClosed':False}
listener=socket.socket();listener.bind(('127.0.0.1',0));listener.listen(1);listener.settimeout(6);port=listener.getsockname()[1]
def server():
    try:
        conn,_=listener.accept();state['accepted']=True
        with conn:
            conn.settimeout(5)
            while True:
                try:data=conn.recv(65536)
                except ConnectionResetError:state['peerClosed']=True;break
                if not data:state['peerClosed']=True;break
                state['startupBytes']+=len(data)
    finally:listener.close()
thread=threading.Thread(target=server,daemon=True);thread.start()
with tempfile.TemporaryDirectory(prefix='yoke-owned-pg-stall-') as folder:
    root=Path(folder);config=root/'config.json';output=root/'must-not-exist.json'
    spec={'client':str(client),'clientSha256':hashlib.sha256(client.read_bytes()).hexdigest(),'address':'127.0.0.1','port':port,'database':'postgres','user':'reader','security':{'kind':'loopback_trust'}};config.write_text(json.dumps(spec),encoding='utf-8')
    command=[str(binary),'--timeout','1500','postgres-catalog','capture','--config',str(config),'--output',str(output),'--trust-client','--allow-network','--allow-output','--allow-loopback-trust'];start=time.monotonic()
    p=subprocess.run(command,cwd=work,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=10,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0);elapsed=time.monotonic()-start;thread.join(timeout=6)
    (out/'stdout').write_bytes(p.stdout);(out/'stderr').write_bytes(p.stderr)
    assert p.returncode!=0 and not output.exists() and not p.stdout and elapsed<5 and state['accepted'] and state['startupBytes']>0 and state['peerClosed'],(elapsed,state)
    assert b'postgres_catalog_deadline_exceeded' in p.stderr
unchanged=all(hashlib.sha256((work/n).read_bytes()).hexdigest()==h for n,h in before.items());assert unchanged
result={'exitCode':p.returncode,'seconds':elapsed,'deadlineMs':1500,'outputCreated':False,'state':state,'binaryHash':hashlib.sha256(binary.read_bytes()).hexdigest(),'sourceUnchanged':unchanged,'inputs':len(before)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
