from pathlib import Path
import subprocess,tempfile,json,hashlib,sqlite3,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/function-body-inline';out=base/'eval-native-green';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();binary=root/'target/debug/yoke.exe';before=sha(binary);rows=[]
requests=out/'requests.json';requests.write_text(json.dumps([{'operation':'refactor.prepare','args':{'operation':'inline','arguments':{'relativePath':'main.ts','namePath':'helper'}}}]))
shutil.copyfile(__file__,out/'runner.py')
for index,request in enumerate([*[json.loads((base/'wrapped-eval-worker-red'/f'request-{i}.json').read_bytes()) for i in range(3)],json.loads((base/'eval-instantiation-worker-red/request.json').read_bytes())]):
 fixture=Path(tempfile.mkdtemp(prefix='ci-inline-eval-green-'))
 for name,value in request['contents'].items():(fixture/name).write_text(value,encoding='utf-8')
 command=[str(binary),'--root',str(fixture),'session','--id','eval-green','--host-profile',str(base/'wrapped-eval-native-red/host.json'),'--requests',str(requests)]
 p=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60);(out/f'stdout-{index}.json').write_bytes(p.stdout);(out/f'stderr-{index}.log').write_bytes(p.stderr)
 response=json.loads(p.stdout) if p.stdout else {};wire=json.dumps(response)+p.stderr.decode(errors='replace');assert 'Body context has dynamic eval authority' in wire,wire
 db=fixture/'.yoke/index.sqlite';assert db.is_file()
 with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
  plans=connection.execute("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal','native_compiler_report_v1')").fetchone()[0]
  derived=connection.execute("SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'").fetchone()[0]
 assert plans==derived==0
 assert all((fixture/n).read_text(encoding='utf-8')==v for n,v in request['contents'].items())
 rows.append({'case':index,'exitCode':p.returncode,'directEvalRefused':True,'planApprovalReceiptJournalReportRecords':plans,'derivedSnapshots':derived,'sourceUnchanged':True,'fixture':str(fixture)})
result={'cases':rows,'allChecksPassed':True,'analyzedCodeExecuted':False,'binaryHashBefore':before,'binaryHashAfter':sha(binary),'binaryUnchanged':before==sha(binary)};assert result['binaryUnchanged'];(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
