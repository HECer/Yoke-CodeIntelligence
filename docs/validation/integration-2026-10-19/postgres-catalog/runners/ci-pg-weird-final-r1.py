import urllib.parse
from pathlib import Path
import tempfile,subprocess,socket,os,time,json,hashlib,ast
base=Path.home()/'AppData/Local/YokeValidation';bins=base/'postgres-18.6-5/pgsql/bin';work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/postgres-catalog/quoted-endpoint-final-r1';out.mkdir(parents=True,exist_ok=False)
env={k:v for k,v in os.environ.items() if not k.upper().startswith(('PG','PSQL','SSL','OPENSSL'))};env.update({'PGCLIENTENCODING':'UTF8','PGSSLMODE':'disable'})
hidden=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0
binary=primary/'target/debug/yoke.exe'
files=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
def inventory():return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in files if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
def run(cmd,input=None,timeout=30):
    p=subprocess.run([str(c) for c in cmd],input=input,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=timeout,creationflags=hidden,cwd=work);assert len(p.stdout)<=32_000_000 and len(p.stderr)<=64000;return p
def cli(name,args,success=True):
    p=run([binary,'--timeout','20000','--max-bytes','1000000','postgres-catalog',*args]);(out/(name+'.stdout')).write_bytes(p.stdout);(out/(name+'.stderr')).write_bytes(p.stderr)
    assert (p.returncode==0)==success,(name,p.returncode,p.stderr.decode(errors='replace')[:300]);return json.loads(p.stdout) if success else None
tree=ast.parse((Path.home()/'AppData/Local/Temp/ci-pg-baseline.py').read_text(encoding='utf-8'))
seed=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='seed' for t in n.targets))
with tempfile.TemporaryDirectory(prefix='yoke-native-pg-live-') as folder:
    root=Path(folder);data=root/'data';empty=root/'empty.pgpass';empty.write_bytes(b'');env.update({'PGPASSFILE':str(empty),'PGSERVICEFILE':str(empty),'PGSYSCONFDIR':str(root)})
    p=run([bins/'initdb.exe','-D',data,'-U','yoke_admin','-A','trust','--encoding=UTF8','--locale=C']);(out/'initdb.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
    s=socket.socket();s.bind(('127.0.0.1',0));port=s.getsockname()[1];s.close()
    with (out/'server.log').open('wb') as log:
        child=subprocess.Popen([str(bins/'postgres.exe'),'-D',str(data),'-h','127.0.0.1','-p',str(port)],stdout=log,stderr=subprocess.STDOUT,env=env,creationflags=hidden)
        try:
            for _ in range(100):
                if child.poll() is not None:raise RuntimeError('Owned server exited')
                try:
                    with socket.create_connection(('127.0.0.1',port),timeout=.1):break
                except OSError:time.sleep(.1)
            else:raise RuntimeError('Owned server readiness timeout')
            common=[bins/'psql.exe','-X','-w','-q','-A','-t','--set=ON_ERROR_STOP=1','-h','127.0.0.1','-p',str(port),'-d','postgres','-U','yoke_admin']
            for _ in range(100):
                ready=run([bins/'pg_isready.exe','-h','127.0.0.1','-p',str(port),'-d','postgres','-U','yoke_admin'])
                if ready.returncode==0:break
                time.sleep(.1)
            else:raise RuntimeError('SQL readiness timeout')
            database="db = ' "+chr(92)+" 💡"
            def quoted(value):return "'"+str(value).replace(chr(92),chr(92)*2).replace("'",chr(92)+"'")+"'"
            create=run(common+['-f','-'],('CREATE DATABASE "'+database.replace('"','""')+'";').encode());assert create.returncode==0
            common=[bins/'psql.exe','-X','-w','-q','-A','-t','--set=ON_ERROR_STOP=1','--dbname',"postgresql:///?"+urllib.parse.urlencode({k:str(v) for k,v in [('hostaddr','127.0.0.1'),('host','127.0.0.1'),('port',port),('dbname',database),('user','yoke_admin'),('sslmode','disable')]},quote_via=urllib.parse.quote,safe='')]
            reader='reader '+chr(0x1f4a1)+chr(39)+chr(92);seed=seed.replace('yoke_reader',chr(34)+reader.replace(chr(34),chr(34)*2)+chr(34))
            p=run(common+['-f','-'],seed.encode());(out/'seed.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
            spec={'client':str(bins/'psql.exe'),'clientSha256':hashlib.sha256((bins/'psql.exe').read_bytes()).hexdigest(),'address':'127.0.0.1','port':port,'database':database,'user':reader,'security':{'kind':'loopback_trust'}}
            config=root/'config.json';config.write_text(json.dumps(spec),encoding='utf-8');archive=root/'catalog.json';args=['capture','--config',str(config),'--output',str(archive)]
            cli('denied-no-permissions',args,False);assert not archive.exists()
            cli('denied-no-loopback-permission',args+['--trust-client','--allow-network','--allow-output'],False);assert not archive.exists()
            flags=['--trust-client','--allow-network','--allow-output','--allow-loopback-trust']
            captured=cli('capture',args+flags);assert archive.exists();raw=archive.read_bytes();(out/'catalog.json').write_bytes(raw)
            assert captured['archiveHash']==hashlib.sha256(raw).hexdigest() and captured['outputCreated'] and not captured['complete']
            inspect=cli('read',['read','--input',str(archive)]);assert len(inspect['nodes'])==7 and len(inspect['observations'])==3 and inspect['meta']['readOnly'] and not inspect['meta']['tls']
            assert all(n['kind']!='column' for n in inspect['nodes']);assert all(n['definitionStatus']=='not_visible_to_role' for n in inspect['nodes'] if n['kind'] in ['function','view'])
            original_user=spec['user'];spec['user']='yoke_admin';config.write_text(json.dumps(spec),encoding='utf-8');owner_archive=root/'owner-catalog.json'
            cli('owner-capture',['capture','--config',str(config),'--output',str(owner_archive),*flags])
            owner=cli('owner-read',['read','--input',str(owner_archive)]);definitions=[n for n in owner['nodes'] if n['kind'] in ['function','view']]
            assert len(definitions)==4 and all(n['definitionStatus']=='available' for n in definitions)
            assert any(r['record'].get('name')=='must_not_run' and 'owned fixture must not execute' in r['record']['definition'] for r in owner['records'])
            (out/'owner-catalog.json').write_bytes(owner_archive.read_bytes());spec['user']=original_user;config.write_text(json.dumps(spec),encoding='utf-8')
            cli('denied-existing-output',args+flags,False);assert archive.read_bytes()==raw
            other=root/'bad.json';spec['clientSha256']='0'*64;config.write_text(json.dumps(spec),encoding='utf-8');cli('denied-client-hash',['capture','--config',str(config),'--output',str(other),*flags],False);assert not other.exists()
            # Frozen traversal must work after the database process is gone.
            stop=run([bins/'pg_ctl.exe','-D',data,'stop','-m','fast','-w','-t','10']);(out/'stop.log').write_bytes(stop.stdout+stop.stderr);child.wait(timeout=10)
            node=next(n for n in inspect['nodes'] if n['schema']=='public' and n['name']=='child');query={'operation':'trace','id':node['id'],'depth':2,'maxNodes':10}
            trace=cli('trace-server-stopped',['read','--input',str(archive),'--query',json.dumps(query)]);assert len(trace['observations'])==2 and trace['networkConnections']==0
            result={'liveClientCapture':True,'serverVersion':run([bins/'postgres.exe','--version']).stdout.decode().strip(),'nodeCount':len(inspect['nodes']),'foreignKeyCount':len(inspect['observations']),'serverStopped':True,'offlineTraceAfterStop':True,'archiveHash':hashlib.sha256(raw).hexdigest(),'binaryHash':hashlib.sha256(binary.read_bytes()).hexdigest(),'tlsScramValidated':False,'quotedUnicodeDatabase':True,'quotedUnicodeRole':True,'visibleOwnerDefinitionsWithoutExecution':True}
        finally:
            if child.poll() is None:
                stop=run([bins/'pg_ctl.exe','-D',data,'stop','-m','fast','-w','-t','10']);(out/'stop-finally.log').write_bytes(stop.stdout+stop.stderr)
                try:child.wait(timeout=10)
                except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result.update({'sourceUnchanged':before==after,'inputs':len(before)});(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
