"""Actual built CLI host-path/protocol admission; exclusively owned temp fixtures."""
from pathlib import Path
import tempfile,subprocess,json,hashlib,os,datetime
root=Path.cwd();out=root/'validation/integration-20261018/history-jobs-development'
exe=(root/'target/debug/yoke.exe').resolve()
with exe.open('rb') as f:binary_hash=hashlib.file_digest(f,'sha256').hexdigest()
receipts=[]
profile={'schemaVersion':1,'appId':123,'secretCredential':'history-jobs-no-such-key',
 'grants':[{'installationId':42,'repositoryId':9,'repository':'org/repo'}]}
request={'schemaVersion':1,'snapshotId':'3'*64,'installationId':42,'repositoryId':9,'repository':'org/repo'}
plan={'traversal':{'objects':16000,'rawBytes':16000000,'parentIds':64000,'frontier':4000},
 'maxCaptures':16000,'maxCandidateFiles':1000000,'maxSourceBytes':1000000000,
 'maxCommitBytes':256000000,'maxTreeBytes':256000000,'maxTreeEntries':1000000}
with tempfile.TemporaryDirectory(prefix='yoke-history-job-path-') as name:
 temp=Path(name).resolve();assert temp.parent==Path(tempfile.gettempdir()).resolve()
 source=temp/'source';source.mkdir();host=temp/'host';host.mkdir()
 profile_path=host/'app.json';request_path=host/'request.json';plan_path=host/'plan.json'
 profile_path.write_text(json.dumps(profile),encoding='utf-8');request_path.write_text(json.dumps(request),encoding='utf-8');plan_path.write_text(json.dumps(plan),encoding='utf-8')
 sentinel=host/'untouched.txt';sentinel.write_bytes(b'owned host sentinel\r\n');sentinel_before=sentinel.read_bytes()
 def run(label,profile_file=profile_path,request_file=request_path,plan_file=None,error='input_invalid',read_only=False):
  command=[str(exe),'--root',str(source),'--max-bytes','4000000']
  if not read_only:command+=['--allow-writes']
  command+=['digest-history-job','--allow-jobs','--id','7'*64,'--profile',str(profile_file)]
  if read_only:command+=['status']
  else:
   command+=['--allow-capture','create','--request',str(request_file)]
   if plan_file is not None:command+=['--plan',str(plan_file)]
  result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
  assert result.returncode!=0 and not result.stdout,(label,result.returncode,result.stdout)
  text=result.stderr.decode('utf-8',errors='replace');assert error in text,(label,text)
  assert 'fixture_private_marker' not in text and not (source/'.yoke').exists(),label
  assert sentinel.read_bytes()==sentinel_before
  (out/(label+'.stderr.log')).write_bytes(result.stderr)
  receipts.append({'case':label,'command':command,'exitCode':result.returncode,'expectedError':error,
   'stdoutEmpty':True,'missingStoreNotInitialized':True,'hostSentinelUnchanged':True})
 run('host-path-positive-admission-control',plan_file=plan_path,error='store_unavailable')
 run('host-path-relative-profile',profile_file=Path('relative-app.json'))
 run('host-path-relative-request',request_file=Path('relative-request.json'))
 run('host-path-relative-plan',plan_file=Path('relative-plan.json'))
 run('host-path-parent-component',profile_file=host/'..'/'host'/'app.json')
 if os.name=='nt':
  run('host-path-device-prefix',profile_file=Path('\\\\?\\'+str(profile_path)))
  run('host-path-ads',profile_file=Path(str(profile_path)+':fixture_private_marker'))
  link=temp/'host-junction';script=temp/'make-junction.ps1'
  script.write_text('param([string]$Link,[string]$Target)\nNew-Item -ItemType Junction -Path $Link -Target $Target -ErrorAction Stop | Out-Null\n',encoding='utf-8')
  pwsh=Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64/pwsh.exe'
  made=subprocess.run([str(pwsh),'-NoProfile','-File',str(script),'-Link',str(link),'-Target',str(host)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
  assert made.returncode==0,made.stderr.decode(errors='replace')
  try:run('host-path-real-junction-ancestor',profile_file=link/'app.json')
  finally:os.rmdir(link)
 bad_plan=host/'bad-plan.json';bad_plan.write_text(json.dumps(dict(plan,allowWrites=True)),encoding='utf-8')
 run('host-plan-cannot-grant-writes',plan_file=bad_plan)
 duplicate=host/'duplicate-plan.json';raw=json.dumps(plan);assert '"maxCaptures": 16000' in raw
 duplicate.write_text(raw.replace('"maxCaptures": 16000','"maxCaptures": 16000, "maxCaptures": 1'),encoding='utf-8')
 run('host-plan-duplicate-key',plan_file=duplicate)
 bad_request=host/'bad-request.json';bad_request.write_text(json.dumps(dict(request,fixture_private_marker='never echo host bytes')),encoding='utf-8')
 run('host-request-unknown-field-not-echoed',request_file=bad_request)
 bad_profile=host/'bad-profile.json';bad_profile.write_text(json.dumps(dict(profile,fixture_private_marker='never echo host bytes')),encoding='utf-8')
 run('host-profile-unknown-field-not-echoed',profile_file=bad_profile)
 run('status-does-not-initialize-missing-state',error='store_unavailable',read_only=True)
with exe.open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()==binary_hash
report={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeBinary':'target/debug/yoke.exe','nativeBinarySha256':binary_hash,
 'casesPassed':len(receipts),'allCasesPassed':True,'scope':'Actual CLI host-path and closed-plan/request admission with valid current host permissions. Positive host-file admission reaches an expected missing-existing-Store refusal; negatives refuse earlier. Real Windows junction fixture needs no privilege change. No source Store, credential lookup, network/model call, source write or system setting change. Separate from native result-group counts.','cases':receipts}
(out/'host-path-gates.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'casesPassed':len(receipts),'allCasesPassed':True,'nativeBinarySha256':binary_hash}))
