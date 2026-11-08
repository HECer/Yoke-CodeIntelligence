from pathlib import Path
import subprocess,json,hashlib
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/app-history-integration'
sha=lambda b:hashlib.sha256(b).hexdigest()
for phase,filename in [('final-r2','source-before.json'),('native-final-r5','source-after.json'),('native-job-output-r6','source-after.json'),('live-cli-r3','source-after.json'),('retained-r2','source-after.json')]:
 files=json.loads((raw/phase/filename).read_text())
 for name,digest in files.items():assert sha((w/name).read_bytes())==digest,(phase,name)
normal=json.loads((raw/'native-final-r5/result.json').read_text());extra=json.loads((raw/'native-job-output-r6/result.json').read_text());quality=json.loads((raw/'final-r2/result.json').read_text());live=json.loads((raw/'live-cli-r3/result.json').read_text());retained=json.loads((raw/'retained-r2/result.json').read_text())
assert all(x['allChecksPassed'] and x['sourceUnchanged'] for x in [normal,extra,live,retained])
assert all(r['exitCode']==0 for r in quality['commands'] if r['name']!='native')
assert normal['commands'][0]['totals']=={'passed':287,'failed':0,'ignored':6}
assert extra['commands'][-1]['totals']=={'passed':13,'failed':0,'ignored':0}
assert sha((w/'native/tests/fixtures/postgres-catalog-response.jsonl').read_bytes())=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
paths=['docs/ARCHITECTURE.md','docs/REMAINING_WORK.md','docs/STATUS.md','docs/parity/native-reconciliation-2026-10-15.json','native/README.md','native/docs/github-app-workers.md','native/docs/app-history-integration.md','native/src/github_app_digest_acquisition.rs','native/src/github_app_digest_history_jobs.rs','native/src/github_app_worker_cli.rs','native/src/github_app_worker_history.rs','native/src/main.rs','native/src/github_app_history_cli.rs','native/tests/github_app_history_bridge.rs','native/tests/github_app_history_cli.rs','native/tests/support/github_app_worker_history_fixture.rs']
assert not subprocess.check_output(['git','diff','--cached','--name-only'],cwd=w).strip()
subprocess.run(['git','add','--',*paths],cwd=w,check=True)
actual=set(subprocess.check_output(['git','diff','--cached','--name-only','-z'],cwd=w).decode().strip('\0').split('\0'))
assert actual==set(paths),(actual-set(paths),set(paths)-actual)
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=w,check=True)
subprocess.run(['git','commit','-m','feat(github-app): bind worker completion to durable history execution'],cwd=w,check=True,stdout=subprocess.DEVNULL)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()
result={'sourceCommit':head,'base':'a6bbe2c4d6cae58f6ce3118d0f0fdb34ff01ea34','nativeGroups':14,'nativePassed':300,'nativeFailed':0,'nativeDefaultIgnored':6,'nativeInputs':758,'retainedInputs':759,'nativeSourceUnchangedAcrossFinalChecks':True,'allRetainedInputsStillMatch':True,'actualWindowsCliGate':'passed; source and binaries unchanged, temporary credential removal and absence verified','wholeNativeRerun':False,'qualification':'13 normal Cargo groups plus unchanged history-job target built with explicit test profile and separate output suffix due held crashed-linker lock. Previous failures preserved.'}
(raw/'final-scope.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result))
