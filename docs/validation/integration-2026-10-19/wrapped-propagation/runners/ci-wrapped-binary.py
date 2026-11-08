from pathlib import Path
import json,hashlib,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=root/'validation/integration-20261018/wrapped-propagation/final'
assert not (out/'build.log').exists()
log=(out/'native.log').read_text(encoding='utf-8',errors='replace')
assert 'Running unittests' in log and 'Finished `test` profile' in log
binary=root/'target/debug/yoke.exe';raw=binary.read_bytes();sha=hashlib.sha256(raw).hexdigest()
assert hashlib.sha256(binary.read_bytes()).hexdigest()==sha
(out/'native-test-binary.json').write_text(json.dumps({'asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'path':str(binary),'bytes':len(raw),'sha256':sha,'observedAfterTestCompilationAndBeforeFinalBuild':True,
    'allTestsFinishedAtProbeTime':False},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'sha256':sha,'bytes':len(raw)}))
