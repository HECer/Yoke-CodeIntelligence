"""Two real CLI owners racing the same saved job revision."""
from pathlib import Path
import subprocess,tempfile,json,hashlib,os,sqlite3,datetime
root=Path.cwd();out=root/'validation/integration-20261018/history-jobs-development';exe=(root/'target/debug/yoke.exe').resolve()
with exe.open('rb') as f:binary_hash=hashlib.file_digest(f,'sha256').hexdigest()
with tempfile.TemporaryDirectory(prefix='yoke-history-job-race-') as name:
 temp=Path(name).resolve();assert temp.parent==Path(tempfile.gettempdir()).resolve()
 repo=temp/'repo';repo.mkdir();host=temp/'host';host.mkdir()
 env=os.environ.copy();env.update({'GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':'NUL' if os.name=='nt' else '/dev/null','GIT_CONFIG_SYSTEM':'NUL' if os.name=='nt' else '/dev/null'})
 def git(*args):
  r=subprocess.run(['git','-c','init.templateDir=','-c','core.autocrlf=false','-c','commit.gpgsign=false','-c','user.name=Race Fixture','-c','user.email=fixture@example.invalid',*args],cwd=repo,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
  assert r.returncode==0,r.stderr.decode(errors='replace');return r.stdout.decode().strip()
 git('init','-q');(repo/'.gitignore').write_text('.yoke/\n',encoding='utf-8')
 source=repo/'café.rs'
 for i in range(5):
  source.write_bytes(('// café \U0001f980\r\npub fn value()->u32{'+str(i)+'}\r\n').encode('utf-8'))
  git('add','--all');git('commit','-q','-m',f'owned race {i}')
 options=host/'git.json';options.write_text(json.dumps({'baseRef':'HEAD~1','headRef':'HEAD','historyLimit':1,'branchLimit':0}),encoding='utf-8')
 def call(label,args,expect=True):
  r=subprocess.run([str(exe),'--root',str(repo),'--timeout','120000','--max-bytes','4000000',*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60)
  (out/(label+'.stdout.log')).write_bytes(r.stdout);(out/(label+'.stderr.log')).write_bytes(r.stderr)
  assert (r.returncode==0)==expect,(label,r.returncode,r.stderr.decode(errors='replace'))
  return json.loads(r.stdout) if expect else r
 analysis=call('race-fixture-git-analyze',['git','analyze','--trust-git','--options',str(options)])
 profile=host/'app.json';profile.write_text(json.dumps({'schemaVersion':1,'appId':123,'secretCredential':'history-race-no-such-key','grants':[{'installationId':42,'repositoryId':9,'repository':'org/repo'}]}),encoding='utf-8')
 request=host/'request.json';request.write_text(json.dumps({'schemaVersion':1,'snapshotId':analysis['headSnapshotId'],'installationId':42,'repositoryId':9,'repository':'org/repo'}),encoding='utf-8')
 base=['digest-history-job','--allow-jobs','--profile',str(profile),'--id','8'*64,'--max-batch-visits','2']
 created=call('race-job-create',['--allow-writes',*base,'--allow-capture','create','--request',str(request)])
 assert created['revision']==0
 source_before=source.read_bytes();index_before=(repo/'.git/index').read_bytes();ref_before=git('rev-parse','HEAD')
 def state():
  with sqlite3.connect('file:'+str(repo/'.yoke/index.sqlite').replace('\\','/')+'?mode=ro',uri=True) as db:
   latest=db.execute("SELECT value FROM meta WHERE key='latest'").fetchone()[0]
   rows=db.execute('SELECT value FROM records WHERE kind=?',[ 'native_digest_history_job_v1']).fetchall()
   return latest,[json.loads(r[0]) for r in rows]
 latest,_=state()
 command=[str(exe),'--root',str(repo),'--timeout','120000','--max-bytes','4000000','--allow-writes',*base,'--allow-capture','--trust-git','step','--revision','0']
 children=[subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE) for _ in range(2)];results=[]
 try:
  for i,p in enumerate(children):
   stdout,stderr=p.communicate(timeout=60)
   (out/f'race-owner-{i}.stdout.log').write_bytes(stdout);(out/f'race-owner-{i}.stderr.log').write_bytes(stderr)
   results.append({'owner':i,'exitCode':p.returncode,'stdout':stdout,'stderr':stderr})
 finally:
  for p in children:
   if p.poll() is None:
    if os.name=='nt':subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    else:p.kill()
    p.wait(timeout=10)
 winners=[r for r in results if r['exitCode']==0];losers=[r for r in results if r['exitCode']!=0]
 assert len(winners)==len(losers)==1,[(r['exitCode'],r['stderr'].decode(errors='replace')) for r in results]
 assert not losers[0]['stdout'] and b'stale_revision' in losers[0]['stderr']
 result=json.loads(winners[0]['stdout']);assert result['revision']==1 and result['visitedCommits']==2
 after_latest,jobs=state();assert after_latest==latest and len(jobs)==1 and jobs[0]['revision']==1 and len(jobs[0]['proof']['visited'])==2
 assert source.read_bytes()==source_before and (repo/'.git/index').read_bytes()==index_before and git('rev-parse','HEAD')==ref_before
 status=call('race-final-readonly-status',[*base,'status']);assert status['revision']==1 and status['visitedCommits']==2
with exe.open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()==binary_hash
report={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeBinary':'target/debug/yoke.exe','nativeBinarySha256':binary_hash,
 'allCasesPassed':True,'competingCliProcesses':2,'successfulAtomicSteps':1,'staleRevisionRefusals':1,'finalRevision':1,'visitedCommits':2,
 'sourceIndexRefAndLatestUnchanged':True,'reopenedReadonlyStatusAgrees':True,
 'scope':'Owned actual Git, source capture and two independent real CLI processes. Both request revision0; exactly one step publishes and the other refuses stale revision without stdout. This separate process-race gate is not added to Cargo result-group totals.'}
(out/'real-cli-race.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps(report))
