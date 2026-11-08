from pathlib import Path
import tempfile,subprocess,socket,os,time,json,hashlib
base=Path.home()/'AppData/Local/YokeValidation';bins=base/'postgres-18.6-5/pgsql/bin';work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');out=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\postgres-catalog\baseline');out.mkdir(parents=True,exist_ok=False)
env={k:v for k,v in os.environ.items() if not k.upper().startswith(('PG','PSQL','SSL','OPENSSL'))};env.update({'PGCLIENTENCODING':'UTF8','PGSSLMODE':'disable'})
hidden=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0
def run(cmd,input=None,cap=4_000_000):
    p=subprocess.run(cmd,input=input,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=30,creationflags=hidden);assert len(p.stdout)<=cap and len(p.stderr)<=64000;return p
seed='''CREATE ROLE yoke_reader LOGIN;
CREATE SCHEMA other;
CREATE TABLE public.parent(a integer,b integer,PRIMARY KEY(a,b));
CREATE TABLE public.child(a integer,b integer,CONSTRAINT same_ref FOREIGN KEY(a,b) REFERENCES public.parent(a,b));
ALTER TABLE public.child ADD CONSTRAINT parallel_ref FOREIGN KEY(a,b) REFERENCES public.parent(a,b);
CREATE TABLE other.child(a integer,b integer,CONSTRAINT same_ref FOREIGN KEY(a,b) REFERENCES public.parent(a,b));
CREATE VIEW public."view 💡" AS SELECT a FROM public.parent;
CREATE FUNCTION public.overload(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1';
CREATE FUNCTION public.overload(text) RETURNS text LANGUAGE sql AS 'SELECT $1';
CREATE FUNCTION public.must_not_run() RETURNS integer LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'owned fixture must not execute'; END$$;
GRANT USAGE ON SCHEMA public,other TO yoke_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public,other TO yoke_reader;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO yoke_reader;
'''
with tempfile.TemporaryDirectory(prefix='yoke-owned-pg-') as folder:
    root=Path(folder);data=root/'data';empty=root/'empty.pgpass';empty.write_bytes(b'');env['PGPASSFILE']=str(empty);env['PGSERVICEFILE']=str(empty);env['PGSYSCONFDIR']=str(root)
    p=run([str(bins/'initdb.exe'),'-D',str(data),'-U','yoke_admin','-A','trust','--encoding=UTF8','--locale=C']);(out/'initdb.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
    s=socket.socket();s.bind(('127.0.0.1',0));port=s.getsockname()[1];s.close()
    with (out/'server.log').open('wb') as log:
        child=subprocess.Popen([str(bins/'postgres.exe'),'-D',str(data),'-h','127.0.0.1','-p',str(port)],stdout=log,stderr=subprocess.STDOUT,env=env,creationflags=hidden)
        try:
            for _ in range(100):
                if child.poll() is not None:raise RuntimeError('Owned server exited during readiness')
                try:
                    with socket.create_connection(('127.0.0.1',port),timeout=.1):break
                except OSError:time.sleep(.1)
            else:raise RuntimeError('Owned server readiness timeout')
            common=[str(bins/'psql.exe'),'-X','-w','-q','-A','-t','--set=ON_ERROR_STOP=1','--set=FETCH_COUNT=1','-h','127.0.0.1','-p',str(port),'-d','postgres']
            p=run(common+['-U','yoke_admin','-f','-'],seed.encode());(out/'seed.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
            env['PGOPTIONS']='-c default_transaction_read_only=on -c search_path=pg_catalog -c statement_timeout=10000 -c lock_timeout=1000'
            script=(work/'native/src/postgres_catalog_query.sql').read_text(encoding='utf-8').replace('__CELL_BYTES__','16384').replace('__RELATION_ROWS__','257').replace('__ROUTINE_ROWS__','257').replace('__FOREIGN_KEY_ROWS__','257')
            p=run(common+['-U','yoke_reader','-f','-'],script.encode());(out/'response.jsonl').write_bytes(p.stdout);(out/'capture.stderr').write_bytes(p.stderr);assert p.returncode==0
            records=[json.loads(line) for line in p.stdout.splitlines() if line.strip()];assert records[0]['readOnly'] and records[0]['isolation']=='serializable' and records[-1]['section']=='end'
            counts={k:sum(r['section']==k for r in records) for k in ['relation','routine','foreignKey']};assert counts=={'relation':4,'routine':3,'foreignKey':3},counts
            p=run(common+['-U','yoke_reader','-f','-'],b'BEGIN READ ONLY; CREATE TEMP TABLE forbidden(x integer); ROLLBACK;');assert p.returncode!=0;(out/'readonly-denial.log').write_bytes(p.stderr)
            fixture=work/'native/tests/fixtures/postgres-catalog-response.jsonl';fixture.write_bytes((out/'response.jsonl').read_bytes())
            result={'serverVersion':run([str(bins/'postgres.exe'),'--version']).stdout.decode().strip(),'counts':counts,'responseBytes':fixture.stat().st_size,'responseSha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),'queryTemplateSha256':hashlib.sha256((work/'native/src/postgres_catalog_query.sql').read_bytes()).hexdigest(),'readonlyWriteDenied':True,'serverStopped':False}
            print(json.dumps(result),flush=True)
        finally:
            stop=run([str(bins/'pg_ctl.exe'),'-D',str(data),'stop','-m','fast','-w','-t','10']);(out/'stop.log').write_bytes(stop.stdout+stop.stderr)
            try:child.wait(timeout=10)
            except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
    result['serverStopped']=True;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print('OWNED SERVER STOPPED',flush=True)
