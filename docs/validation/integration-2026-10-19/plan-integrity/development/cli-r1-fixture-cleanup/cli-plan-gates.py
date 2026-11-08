from pathlib import Path
import tempfile, subprocess, sqlite3, hashlib, json, shutil, datetime
root=Path.cwd();out=root/'validation/integration-20261018/plan-integrity-development'
exe=root/'target/debug/yoke.exe';binary_hash=hashlib.sha256(exe.read_bytes()).hexdigest();cases=[]
before='pub fn original(){ /* café 🦀 */ }\r\n';after='pub fn reviewed(){ /* café 🦀 */ }\r\n'
with tempfile.TemporaryDirectory(prefix='yoke-plan-cli-') as name:
 temp=Path(name).resolve();source=temp/'source';(source/'src').mkdir(parents=True);path=source/'src/spec.rs';path.write_bytes(before.encode())
 host=temp/'host';host.mkdir();input_file=host/'edits.json'
 input_file.write_text(json.dumps({'edits':[{'kind':'replace','path':'src/spec.rs','expectedHash':hashlib.sha256(before.encode()).hexdigest(),'startByte':0,'endByte':len(before.encode()),'text':after}]}),encoding='utf-8')
 def run(label,args,ok=True):
  command=[str(exe),'--root',str(source),*args];r=subprocess.run(command,capture_output=True,timeout=60)
  (out/(label+'.stdout.log')).write_bytes(r.stdout);(out/(label+'.stderr.log')).write_bytes(r.stderr)
  assert (r.returncode==0)==ok,(label,r.returncode,r.stdout,r.stderr)
  cases.append({'case':label,'command':command,'exitCode':r.returncode,'expectedSuccess':ok})
  if ok:return json.loads(r.stdout)
 def data(value):return value.get('data',value)
 prepared=data(run('cli-prepare',['prepare','--input',str(input_file)]));plan=prepared['plan'];pid=plan['id'];dbpath=source/'.yoke/index.sqlite'
 def row(kind='plan'):
  with sqlite3.connect(dbpath) as db:return db.execute('SELECT value FROM records WHERE kind=? AND id=?',(kind,pid)).fetchone()
 def save_plan(value):
  with sqlite3.connect(dbpath) as db:db.execute("UPDATE records SET value=? WHERE kind='plan' AND id=?",(value,pid))
 valid=row()[0];run('cli-test-change-gate',['approve',pid],False);assert row('approval') is None
 approved=run('cli-approve-exact',['approve',pid,'--allow-test-changes']);assert approved['approvedPlanId']==pid
 for label,field,value in [('cli-review-corruption','diff','altered review'),('cli-warnings-corruption','warnings',[]),('cli-expiry-corruption','expiresAt',plan['expiresAt']+1)]:
  changed=json.loads(valid);changed[field]=value;save_plan(json.dumps(changed));prior=row('approval')
  run(label,['approve',pid,'--allow-test-changes'],False);assert row('approval')==prior;assert path.read_bytes()==before.encode();save_plan(valid)
 run('cli-zero-timeout',['--timeout','0','approve',pid,'--allow-test-changes'],False)
 changed=json.loads(valid);changed['diff']='altered review';save_plan(json.dumps(changed));marker=host/'verifier-started'
 program='require(\"fs\").writeFileSync('+json.dumps(str(marker))+',\"started\")'
 run('cli-invalid-before-verifier',['verify','--plan',pid,'--command',shutil.which('node'),'--arg=-e','--arg='+program,'--trust-project'],False)
 assert not marker.exists() and row('verification') is None;save_plan(valid)
 program='process.exit(require(\"fs\").readFileSync(\"src/spec.rs\",\"utf8\")==='+json.dumps(after)+'?0:1)'
 verified=run('cli-owned-executed-verification',['verify','--plan',pid,'--command',shutil.which('node'),'--arg=-e','--arg='+program,'--trust-project'])
 assert data(verified).get('status',data(verified).get('verification',{}).get('status'))=='passed',verified
 changed=json.loads(valid);changed['files'][0]['afterHash']='7'*64;save_plan(json.dumps(changed))
 run('cli-approved-effect-corruption',['--allow-writes','apply',pid,'--key','owned-corrupt','--mode','workspace'],False);assert path.read_bytes()==before.encode();save_plan(valid)
 applied=data(run('cli-workspace-apply',['--allow-writes','apply',pid,'--key','owned-valid','--mode','workspace']))
 receipt=applied.get('receipt',applied);journal=receipt['id'];assert path.read_bytes()==after.encode()
 changed=json.loads(valid);changed['files'][0]['beforeHash']='7'*64;save_plan(json.dumps(changed))
 run('cli-recovery-corruption',['--allow-writes','recover',journal],False);assert path.read_bytes()==after.encode()
 historical=data(run('cli-existing-idempotent-receipt',['--allow-writes','apply',pid,'--key','owned-valid','--mode','workspace']))
 assert historical.get('receipt',historical)['id']==journal;assert path.read_bytes()==after.encode();save_plan(valid)
 run('cli-conditional-recovery',['--allow-writes','recover',journal]);assert path.read_bytes()==before.encode()
assert hashlib.sha256(exe.read_bytes()).hexdigest()==binary_hash
report={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeBinarySha256':binary_hash,'casesPassed':len(cases),'allCasesPassed':True,'scope':'Actual native CLI on exclusively owned temporary source and raw SQLite fault injection. One explicitly authorized Node verifier reads only owned copied fixture bytes; no analyzed third-party code or compiler execution is claimed. Separate from Cargo counts.','cases':cases}
(out/'cli-gates.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps({'casesPassed':len(cases),'allCasesPassed':True,'nativeBinarySha256':binary_hash}))
