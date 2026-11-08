from pathlib import Path
import tempfile,subprocess,socket,os,time,json,hashlib,ast,secrets,ctypes
base=Path.home()/'AppData/Local/YokeValidation';bins=base/'postgres-18.6-5/pgsql/bin';work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/postgres-catalog/tls-r4';out.mkdir(parents=True,exist_ok=False)
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
    root=Path(folder);data=root/'data';
    certgen=run([primary/'target/debug/yoke-owned-pg-fixture-certgen.exe',root]);assert certgen.returncode==0
    password=secrets.token_urlsafe(32);owned_credentials=[];
    empty=root/'empty.pgpass';empty.write_bytes(b'');env.update({'PGPASSFILE':str(empty),'PGSERVICEFILE':str(empty),'PGSYSCONFDIR':str(root)})
    p=run([bins/'initdb.exe','-D',data,'-U','yoke_admin','-A','trust','--encoding=UTF8','--locale=C']);(out/'initdb.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
    s=socket.socket();s.bind(('127.0.0.1',0));port=s.getsockname()[1];s.close()
    with (out/'server.log').open('wb') as log:
        child=subprocess.Popen([str(bins/'postgres.exe'),'-D',str(data),'-h','127.0.0.1','-p',str(port),'-c','ssl=on','-c','ssl_cert_file='+str(root/'server.pem'),'-c','ssl_key_file='+str(root/'server.key')],stdout=log,stderr=subprocess.STDOUT,env=env,creationflags=hidden)
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
            p=run(common+['-f','-'],seed.encode());(out/'seed.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
            spec={'client':str(bins/'psql.exe'),'clientSha256':hashlib.sha256((bins/'psql.exe').read_bytes()).hexdigest(),'address':'127.0.0.1','port':port,'database':'postgres','user':'yoke_reader','security':{'kind':'tls_scram','serverName':'localhost','rootCertificatePem':(root/'root.pem').read_text()}}
            # Set only the owned fixture role's password through private stdin, never a log/argument.
            altered=run(common+['-f','-'],("SET password_encryption='scram-sha-256'; ALTER ROLE yoke_reader PASSWORD '"+password+"';").encode());assert altered.returncode==0
            hba=data/'pg_hba.conf';hba.write_text('hostssl all yoke_reader 127.0.0.1/32 scram-sha-256\nhost all all 127.0.0.1/32 trust\n',encoding='ascii')
            reload=run([bins/'pg_ctl.exe','-D',data,'reload']);assert reload.returncode==0;time.sleep(.3)
            # Hostile ambient libpq variables and psqlrc must not affect the native child.
            profile=root/'hostile.psqlrc';marker=root/'profile-ran';profile.write_text(chr(92)+'! echo BAD > '+str(marker)+chr(10))
            env.update({'PGPASSWORD':'ambient wrong password','PGHOST':'192.0.2.1','PGPORT':'9','PGDATABASE':'wrong','PGUSER':'wrong','PGSERVICE':'unknown','PSQLRC':str(profile)})
            config=root/'config.json';config.write_text(json.dumps(spec),encoding='utf-8');archive=root/'catalog.json';args=['capture','--config',str(config),'--output',str(archive)]
            cli('denied-no-permissions',args,False);assert not archive.exists()
            cli('denied-missing-credential',args+['--trust-client','--allow-network','--allow-output'],False);assert not archive.exists()
            def credential(name):
                p=run([binary,'postgres-catalog','import-password','--config',config,'--allow-credential-create'],password.encode());assert p.returncode==0
                result=json.loads(p.stdout);owned_credentials.append('yoke-code-intelligence/service/postgres/'+result['credentialBinding']);(out/(name+'.json')).write_text(json.dumps(result)+'\n')
            credential('credential-import')
            flags=['--trust-client','--allow-network','--allow-output']
            captured=cli('capture',args+flags);assert archive.exists();raw=archive.read_bytes();(out/'catalog.json').write_bytes(raw)
            assert captured['archiveHash']==hashlib.sha256(raw).hexdigest() and captured['outputCreated'] and not captured['complete']
            inspect=cli('read',['read','--input',str(archive)]);assert len(inspect['nodes'])==7 and len(inspect['observations'])==3 and inspect['meta']['readOnly'] and inspect['meta']['tls']
            assert all(n['kind']!='column' for n in inspect['nodes']);assert all(n['definitionStatus']=='not_visible_to_role' for n in inspect['nodes'] if n['kind'] in ['function','view'])
            cli('denied-existing-output',args+flags,False);assert archive.read_bytes()==raw
            other=root/'bad.json';spec['security']['serverName']='wrong.example';config.write_text(json.dumps(spec),encoding='utf-8');credential('wrong-name-credential')
            cli('denied-tls-hostname',['capture','--config',str(config),'--output',str(other),*flags],False);assert not other.exists()
            spec['security']['serverName']='localhost';spec['clientSha256']='0'*64;config.write_text(json.dumps(spec),encoding='utf-8');cli('denied-client-hash',['capture','--config',str(config),'--output',str(other),*flags],False);assert not other.exists()
            # Frozen traversal must work after the database process is gone.
            stop=run([bins/'pg_ctl.exe','-D',data,'stop','-m','fast','-w','-t','10']);(out/'stop.log').write_bytes(stop.stdout+stop.stderr);child.wait(timeout=10)
            node=next(n for n in inspect['nodes'] if n['schema']=='public' and n['name']=='child');query={'operation':'trace','id':node['id'],'depth':2,'maxNodes':10}
            trace=cli('trace-server-stopped',['read','--input',str(archive),'--query',json.dumps(query)]);assert len(trace['observations'])==2 and trace['networkConnections']==0
            result={'liveClientCapture':True,'serverVersion':run([bins/'postgres.exe','--version']).stdout.decode().strip(),'nodeCount':len(inspect['nodes']),'foreignKeyCount':len(inspect['observations']),'serverStopped':True,'offlineTraceAfterStop':True,'archiveHash':hashlib.sha256(raw).hexdigest(),'binaryHash':hashlib.sha256(binary.read_bytes()).hexdigest(),'tlsScramValidated':True,'tlsHostnameMismatchRefused':True,'ambientConfigurationIgnored':not marker.exists()}
        finally:
            api=ctypes.WinDLL('advapi32',use_last_error=True);api.CredDeleteW.argtypes=[ctypes.c_wchar_p,ctypes.c_ulong,ctypes.c_ulong];api.CredDeleteW.restype=ctypes.c_int
            for target in owned_credentials:
                assert target.startswith('yoke-code-intelligence/service/postgres/') and len(target.rsplit('/',1)[-1])==64
                assert api.CredDeleteW(target,1,0),'Owned fixture credential cleanup failed'
            if child.poll() is None:
                stop=run([bins/'pg_ctl.exe','-D',data,'stop','-m','fast','-w','-t','10']);(out/'stop-finally.log').write_bytes(stop.stdout+stop.stderr)
                try:child.wait(timeout=10)
                except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result.update({'ownedFixtureCredentialsRemoved':len(owned_credentials),'sourceUnchanged':before==after,'inputs':len(before)});(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
