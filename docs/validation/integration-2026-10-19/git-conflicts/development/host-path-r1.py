"""Additional actual CLI admission gates; exclusively owned disposable host files."""
from pathlib import Path
import tempfile,subprocess,json,hashlib,os,datetime
root=Path.cwd();out=root/'validation/integration-20261018/git-conflicts-development'
exe=(root/'target/debug/yoke.exe').resolve();binary_hash=hashlib.sha256(exe.read_bytes()).hexdigest();receipts=[]
limits={'maxPaths':2000,'maxStageRows':6000,'maxIndexBytes':2000000,'maxBlobObjects':6000,'maxBlobBytes':4000000,'maxSourceBytes':32000000,'maxQuoteBytes':16384,'maxTotalQuoteBytes':256000,'maxScopeFiles':20000,'maxScopeMetadataBytes':16000000,'maxOutputBytes':4000000}
page={'schemaVersion':1,'expectedIndexHash':'a'*64,'path':'source.rs','stage':2,'startByte':0,'maxBytes':16}
with tempfile.TemporaryDirectory(prefix='yoke-conflicts-host-') as name:
 temp=Path(name).resolve();assert temp.parent==Path(tempfile.gettempdir()).resolve();source=temp/'source';source.mkdir();host=temp/'host';host.mkdir()
 lp=host/'limits.json';lp.write_text(json.dumps(limits),encoding='utf-8');pp=host/'page.json';pp.write_text(json.dumps(page),encoding='utf-8');sentinel=host/'untouched.txt';sentinel.write_bytes(b'owned host sentinel\r\n')
 def run(label,limit=lp,page_file=None,error='input_invalid'):
  command=[str(exe),'--root',str(source),'git','conflicts','--allow-index-read','--trust-git','--limits',str(limit)]
  if page_file is not None:command+=['--allow-git-sources','--scope-snapshot','3'*64,'--read-source',str(page_file)]
  r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30);text=r.stderr.decode('utf-8',errors='replace')
  assert r.returncode!=0 and not r.stdout and error in text,(label,r.returncode,text)
  assert 'fixture_private_marker' not in text and not (source/'.yoke').exists(),label
  assert sentinel.read_bytes()==b'owned host sentinel\r\n';(out/(label+'.stderr.log')).write_bytes(r.stderr)
  receipts.append({'case':label,'command':command,'exitCode':r.returncode,'expectedError':error,'stdoutEmpty':True,'missingStoreNotInitialized':True,'hostSentinelUnchanged':True})
 run('host-limits-positive-control',error='store_unavailable');run('host-page-positive-control',page_file=pp,error='store_unavailable')
 run('host-limits-relative',limit=Path('relative.json'));run('host-page-relative',page_file=Path('relative.json'));run('host-limits-parent',limit=host/'..'/'host'/'limits.json')
 if os.name=='nt':
  run('host-limits-device-prefix',limit=Path('\\\\?\\'+str(lp)));run('host-limits-ads',limit=Path(str(lp)+':fixture_private_marker'))
  link=temp/'host-junction';script=temp/'make-junction.ps1';script.write_text('param([string]$Link,[string]$Target)\nNew-Item -ItemType Junction -Path $Link -Target $Target -ErrorAction Stop | Out-Null\n',encoding='utf-8')
  pwsh=Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64/pwsh.exe';r=subprocess.run([str(pwsh),'-NoProfile','-File',str(script),'-Link',str(link),'-Target',str(host)],capture_output=True,timeout=30);assert r.returncode==0,r.stderr
  try:
   run('host-limits-real-junction-ancestor',limit=link/'limits.json');run('host-page-real-junction-ancestor',page_file=link/'page.json')
  finally:os.rmdir(link)
 bad=host/'bad.json'
 for label,value,error in [('host-limits-authority',dict(limits,allowGitSources=True),'input_invalid'),('host-limits-missing-field',{k:v for k,v in limits.items() if k!='maxPaths'},'input_invalid'),('host-limits-above-hard-max',dict(limits,maxPaths=2001),'capacity_exceeded')]:
  bad.write_text(json.dumps(value),encoding='utf-8');run(label,limit=bad,error=error)
 bad.write_text(json.dumps(limits).replace('"maxPaths": 2000','"maxPaths": 2000, "maxPaths": 1'),encoding='utf-8');run('host-limits-duplicate-key',limit=bad)
 bad.write_bytes(b' '*8193);run('host-limits-oversized',limit=bad)
 for label,value in [('host-page-authority',dict(page,allowGitSources=True)),('host-page-unknown-value-not-echoed',dict(page,fixture_private_marker='never echo host bytes')),('host-page-utf8-byte-budget-hard-max',dict(page,maxBytes=16385))]:
  bad.write_text(json.dumps(value),encoding='utf-8');run(label,page_file=bad)
 bad.write_text(json.dumps(page).replace('"stage": 2','"stage": 2, "stage": 3'),encoding='utf-8');run('host-page-duplicate-key',page_file=bad)
assert hashlib.sha256(exe.read_bytes()).hexdigest()==binary_hash
report={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeBinary':'target/debug/yoke.exe','nativeBinarySha256':binary_hash,'casesPassed':len(receipts),'allCasesPassed':True,'scope':'Actual CLI admission with current valid index/Git/source host flags and exclusively owned files. Positive limits/page controls reach missing-existing-Store refusal; malformed, authority-bearing, over-limit and guarded path inputs refuse earlier. Real unprivileged Windows junction; no settings/privilege changes, source Store, credentials, network/model or project execution. Separate from Cargo totals.','cases':receipts}
(out/'host-path-gates.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps({'casesPassed':len(receipts),'allCasesPassed':True,'nativeBinarySha256':binary_hash}))
