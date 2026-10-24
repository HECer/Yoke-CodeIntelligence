from pathlib import Path
import ctypes,hashlib,json,os,secrets,shutil,subprocess,time
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach'
out=root/'validation/integration-20261018/app-history-integration/live-cli-r3'
out.mkdir(parents=True,exist_ok=False)
def sha(b):return hashlib.sha256(b).hexdigest()
def inventory():
 names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')
 return {n:sha((work/n).read_bytes()) for n in sorted(set(names)) if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
shutil.copyfile(__file__,out/'runner.py')
binary=root/'target/debug/yoke.exe'
targets=list((root/'target/debug/deps').glob('github_app_history_bridge-*.exe'));assert len(targets)==1
test=targets[0]
binary_before={str(p):sha(p.read_bytes()) for p in [binary,test]}
name='test-app-history-'+secrets.token_hex(16)
target='yoke-code-intelligence/service/github-webhook/'+name
assert name.startswith('test-app-history-') and len(name)==49
api=ctypes.WinDLL('advapi32',use_last_error=True)
api.CredDeleteW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32];api.CredDeleteW.restype=ctypes.c_int
api.CredReadW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.POINTER(ctypes.c_void_p)];api.CredReadW.restype=ctypes.c_int
api.CredFree.argtypes=[ctypes.c_void_p]
created=False;cleanup=False;runs=[]
try:
 # This public fixture canary is defined in the owned test source, not a user secret.
 imported=subprocess.run([str(binary),'credentials','github-webhook-import',name],input=b'owned public app history signing fixture key',stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=work)
 assert imported.returncode==0,'owned fixture credential creation failed'
 created=True
 env=os.environ.copy();env['YOKE_APP_HISTORY_CREDENTIAL']=name
 env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
 cmd=[str(test),'actual_cli_lost_stdout_status_pause_resume_and_run_use_the_owned_os_key','--exact','--ignored','--nocapture','--test-threads=1']
 print('START APP HISTORY ACTUAL WINDOWS CLI',flush=True);start=time.monotonic()
 with (out/'native.log').open('wb') as log:r=subprocess.run(cmd,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
 runs.append({'command':cmd,'exitCode':r.returncode,'seconds':time.monotonic()-start})
finally:
 if created:
  assert api.CredDeleteW(target,1,0),'owned fixture credential cleanup failed'
  ptr=ctypes.c_void_p();exists=api.CredReadW(target,1,0,ctypes.byref(ptr))
  if exists:api.CredFree(ptr)
  cleanup=not exists and ctypes.get_last_error()==1168
  assert cleanup,'owned credential absence unproven'
 after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
 binary_after={str(p):sha(p.read_bytes()) for p in [binary,test]}
 result={'sourceUnchanged':before==after,'inputs':len(before),'binaryBefore':binary_before,'binaryAfter':binary_after,'binaryUnchanged':binary_before==binary_after,'ownedCredentialCreated':created,'ownedCredentialRemovedAndAbsenceVerified':cleanup,'commands':runs,'allChecksPassed':bool(runs) and all(r['exitCode']==0 for r in runs) and cleanup,'wholeNativeRerun':False}
 (out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)

assert result["allChecksPassed"] and result["sourceUnchanged"] and result["binaryUnchanged"]
