from pathlib import Path
import tempfile,subprocess,json,hashlib,os,datetime
root=Path.cwd();out=root/'validation/integration-20261018/git-areas-development';exe=root/'target/debug/yoke.exe';binary_hash=hashlib.sha256(exe.read_bytes()).hexdigest();receipts=[]
options={'details':{'maxCandidatePairs':100000,'maxSourceBytes':128000000,'maxSimilarityLines':200000,'maxSimilarityWork':10000000,'maxRows':200000,'maxOutputBytes':4000000,'maxQuoteBytes':16384,'maxTotalQuoteBytes':1000000},'limits':{'maxPaths':20000,'maxOccurrences':200000,'maxSnapshotFiles':20000,'maxConcepts':2048,'maxEvidenceRows':16384,'maxSupportReferences':32768,'maxMemberships':32768,'maxAssignments':65536,'maxMeaningBytes':8000000,'maxMetadataBytes':16000000,'maxOutputBytes':4000000}}
with tempfile.TemporaryDirectory(prefix='yoke-area-host-') as name:
 temp=Path(name).resolve();assert temp.parent==Path(tempfile.gettempdir()).resolve();source=temp/'source';source.mkdir();host=temp/'host';host.mkdir();good=host/'options.json';good.write_bytes(json.dumps(options).encode());bad=host/'bad.json';sentinel=host/'untouched.txt';sentinel.write_bytes(b'owned area host sentinel\r\n')
 def run(label,path=good,extra=None,globals=None,error='input_invalid'):
  cmd=[str(exe),'--root',str(source),*(globals or []),'git','areas','7'*64]
  if path is not None:cmd+=['--options',str(path)]
  cmd+=extra or [];r=subprocess.run(cmd,capture_output=True,timeout=30);text=r.stderr.decode(errors='replace')
  assert r.returncode!=0 and not r.stdout and 'git_areas_'+error in text,(label,r.returncode,text)
  assert 'fixture_private_marker' not in text and not (source/'.yoke').exists();assert sentinel.read_bytes()==b'owned area host sentinel\r\n';(out/(label+'.stderr.log')).write_bytes(r.stderr)
  receipts.append({'case':label,'command':cmd,'exitCode':r.returncode,'expectedError':'git_areas_'+error,'stdoutEmpty':True,'stateNotInitialized':True,'hostSentinelUnchanged':True})
 run('host-options-positive',error='store_unavailable');run('host-default-positive',path=None,error='store_unavailable');run('host-options-relative',path=Path('relative.json'));run('host-options-parent',path=host/'..'/'host'/'options.json')
 if os.name=='nt':
  run('host-options-device-prefix',path=Path('\\\\?\\'+str(good)));run('host-options-ads',path=Path(str(good)+':fixture_private_marker'))
  link=temp/'host-junction';script=temp/'make-junction.ps1';script.write_text('param([string]$Link,[string]$Target)\nNew-Item -ItemType Junction -Path $Link -Target $Target -ErrorAction Stop | Out-Null\n',encoding='utf-8');pwsh=Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64/pwsh.exe';made=subprocess.run([str(pwsh),'-NoProfile','-File',str(script),'-Link',str(link),'-Target',str(host)],capture_output=True,timeout=30);assert made.returncode==0,made.stderr
  try:run('host-options-real-junction',path=link/'options.json')
  finally:os.rmdir(link)
 for part in ['root','details','limits']:
  value=json.loads(json.dumps(options));target=value if part=='root' else value[part];target['allowWrites']=True;bad.write_bytes(json.dumps(value).encode());run('host-unknown-'+part,path=bad)
 raw=json.dumps(options);bad.write_bytes(raw.replace('"details": {','"details": {}, "details": {',1).encode());run('host-duplicate-root',path=bad)
 bad.write_bytes(raw.replace('"maxPaths": 20000','"maxPaths": 20000, "maxPaths": 1').encode());run('host-duplicate-nested',path=bad)
 value=json.loads(raw);del value['limits']['maxPaths'];bad.write_bytes(json.dumps(value).encode());run('host-missing-limit',path=bad)
 value=json.loads(raw);value['details']['maxSourceBytes']=128000001;bad.write_bytes(json.dumps(value).encode());run('host-detail-hard-max',path=bad,error='capacity_exceeded')
 value=json.loads(raw);value['limits']['maxPaths']=20001;bad.write_bytes(json.dumps(value).encode());run('host-limit-hard-max',path=bad,error='capacity_exceeded')
 padded=raw.encode().ljust(8192,b' ');assert len(padded)==8192;bad.write_bytes(padded);run('host-options-exact-byte-cap',path=bad,error='store_unavailable');bad.write_bytes(padded+b' ');run('host-options-over-byte-cap',path=bad)
 value=json.loads(raw);value['fixture_private_marker']='never echo host bytes';bad.write_bytes(json.dumps(value).encode());run('host-unknown-value-not-echoed',path=bad)
 run('host-duplicate-meaning-selection',extra=['--meaning','1'*64,'--meaning','1'*64]);run('host-too-many-meaning-selections',extra=[x for i in range(1,6) for x in ['--meaning',str(i)*64]])
 run('host-unused-write-authority',globals=['--allow-writes']);run('host-unused-snapshot-authority',globals=['--snapshot','1'*64]);run('host-zero-timeout',globals=['--timeout','0']);run('host-zero-output',globals=['--max-bytes','0'],error='capacity_exceeded')
assert hashlib.sha256(exe.read_bytes()).hexdigest()==binary_hash
report={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeBinary':'target/debug/yoke.exe','nativeBinarySha256':binary_hash,'casesPassed':len(receipts),'allCasesPassed':True,'scope':'Actual native CLI host-input/protocol admission, positive controls and exact byte-cap boundary on exclusively owned fixtures. Real Windows junction needs no privilege changes. No source Store, credential/provider lookup, Git/network/model call or system setting change. Separate from Cargo totals.','cases':receipts};(out/'host-path-gates.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps({'casesPassed':len(receipts),'allCasesPassed':True,'nativeBinarySha256':binary_hash}))
